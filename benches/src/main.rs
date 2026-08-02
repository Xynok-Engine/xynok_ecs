mod alloc_probe;
mod bevy_bench;
mod common;
mod report;
mod vec_bench;
mod xynok_bench;

use std::hint::black_box;
use std::path::Path;
use std::time::{Duration, Instant};

use common::{ArchetypeLayout, EcsBenchmark, ENTITY_COUNTS, MEASURED_SAMPLES, MIN_SAMPLE_NANOS, WARMUP_ITERS};
use report::{mean_ns, percentile_of_sorted, stddev_ns, AllocStats, BenchResult, QueryTiming};

#[global_allocator]
static ALLOC_PROBE: alloc_probe::CountingAllocator = alloc_probe::CountingAllocator;

/// Tiny deterministic xorshift64* PRNG, seeded from the clock, used only to shuffle the order
/// competitors run in (see `run_in_random_order`) — no need for an external `rand` dependency for
/// that.
struct Rng(u64);

impl Rng
{
    fn seeded() -> Self
    {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos() as u64;
        Self(nanos | 1)
    }

    fn next_u64(&mut self) -> u64
    {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Fisher-Yates shuffle.
    fn shuffle<T>(&mut self, items: &mut [T])
    {
        for i in (1..items.len()).rev()
        {
            let j = (self.next_u64() % (i as u64 + 1)) as usize;
            items.swap(i, j);
        }
    }
}

/// Keeps the CPU out of a low-power idle state before the first real measurement, so the very
/// first benchmark run isn't unfairly slowed down by frequency ramp-up relative to later ones.
fn spin_warm_cpu(duration: Duration)
{
    let start = Instant::now();
    let mut acc: u64 = 0;
    while start.elapsed() < duration
    {
        acc = acc.wrapping_add(1);
        black_box(acc);
    }
}

fn run_benchmark<B: EcsBenchmark>(entity_count: usize, layout: ArchetypeLayout) -> BenchResult
{
    let baseline = alloc_probe::snapshot();

    // --- setup: allocation is expected and measured here, not held against query speed ---
    let before_setup = alloc_probe::snapshot();
    let mut storage = B::setup(entity_count, layout);
    let after_setup = alloc_probe::snapshot();
    let setup_delta = alloc_probe::delta(&before_setup, &after_setup);

    let mut query = B::prepare_query(&mut storage);

    // Warm up caches (query plans, first-touch pages, ...) outside the measured region so the
    // timed loop below reflects steady-state iteration, not one-time setup cost.
    for _ in 0..WARMUP_ITERS
    {
        B::run_query_once(black_box(&mut storage), black_box(&mut query));
    }

    // Calibrate how many `run_query_once` calls to batch into one timed sample: fast queries
    // (sub-microsecond, e.g. small entity counts) get batched so `Instant::now()`'s own overhead
    // (tens of ns per call, paid twice per sample) can't dominate the measurement; queries already
    // slower than `MIN_SAMPLE_NANOS` just get batch_size == 1.
    let calibration_start = Instant::now();
    B::run_query_once(black_box(&mut storage), black_box(&mut query));
    let calibration_ns = calibration_start.elapsed().as_nanos().max(1);
    let batch_size = ((MIN_SAMPLE_NANOS / calibration_ns) as usize).max(1);

    // Pre-reserve the sample buffer *before* the measured region so pushing into it can't
    // register as "query allocation" and pollute the isolation check below.
    let mut samples: Vec<f64> = Vec::with_capacity(MEASURED_SAMPLES);

    let before_query = alloc_probe::snapshot();
    for _ in 0..MEASURED_SAMPLES
    {
        let t0 = Instant::now();
        for _ in 0..batch_size
        {
            B::run_query_once(black_box(&mut storage), black_box(&mut query));
        }
        let elapsed_ns = t0.elapsed().as_nanos() as f64;
        samples.push(elapsed_ns / batch_size as f64);
    }
    let after_query = alloc_probe::snapshot();
    let query_delta = alloc_probe::delta(&before_query, &after_query);

    // Compute stats from `samples` first, then drop every harness-owned allocation (including
    // `samples` itself) before the leak snapshot below — otherwise the harness's own bookkeeping
    // would be misreported as a leak in whichever library happened to run.
    samples.sort_by(|a, b| a.partial_cmp(b).expect("timing samples must never be NaN"));
    let min_ns = samples[0];
    let max_ns = samples[samples.len() - 1];
    let mean_ns = mean_ns(&samples);
    let median_ns = percentile_of_sorted(&samples, 50.0);
    let p95_ns = percentile_of_sorted(&samples, 95.0);
    let p99_ns = percentile_of_sorted(&samples, 99.0);
    let stddev_ns = stddev_ns(&samples, mean_ns);

    drop(samples);
    drop(query);
    drop(storage);
    let after_drop = alloc_probe::snapshot();
    let leaked_bytes = alloc_probe::leaked_bytes(&baseline, &after_drop);

    BenchResult {
        library: B::NAME.to_string(),
        entity_count,
        component_count: B::COMPONENT_COUNT,
        archetype_layout: layout,
        setup_alloc: AllocStats {
            bytes:       setup_delta.bytes,
            allocations: setup_delta.allocations,
        },
        query_alloc: AllocStats {
            bytes:       query_delta.bytes,
            allocations: query_delta.allocations,
        },
        leaked_bytes: leaked_bytes as i64,
        query_timing: QueryTiming {
            warmup_iters: WARMUP_ITERS,
            measured_samples: MEASURED_SAMPLES,
            batch_size,
            min_ns,
            max_ns,
            mean_ns,
            median_ns,
            p95_ns,
            p99_ns,
            stddev_ns,
        },
    }
}

type Runner = Box<dyn Fn(usize, ArchetypeLayout) -> BenchResult>;

/// The 3 competitors for a given query arity, as boxed closures so they can be shuffled together
/// at runtime (each `run_benchmark::<B>` monomorphizes to a distinct, non-generic function once
/// captured in a closure with a fixed signature).
fn runners_for_arity(component_count: u8) -> Vec<Runner>
{
    match component_count
    {
        1 => vec![
            Box::new(|ec, l| run_benchmark::<xynok_bench::XynokBenchmark1>(ec, l)),
            Box::new(|ec, l| run_benchmark::<bevy_bench::BevyBenchmark1>(ec, l)),
            Box::new(|ec, l| run_benchmark::<vec_bench::VecBenchmark1>(ec, l)),
        ],
        2 => vec![
            Box::new(|ec, l| run_benchmark::<xynok_bench::XynokBenchmark2>(ec, l)),
            Box::new(|ec, l| run_benchmark::<bevy_bench::BevyBenchmark2>(ec, l)),
            Box::new(|ec, l| run_benchmark::<vec_bench::VecBenchmark2>(ec, l)),
        ],
        3 => vec![
            Box::new(|ec, l| run_benchmark::<xynok_bench::XynokBenchmark3>(ec, l)),
            Box::new(|ec, l| run_benchmark::<bevy_bench::BevyBenchmark3>(ec, l)),
            Box::new(|ec, l| run_benchmark::<vec_bench::VecBenchmark3>(ec, l)),
        ],
        _ => unreachable!("only 1..=3 component queries are benchmarked"),
    }
}

fn main()
{
    // A debug build's numbers are meaningless for a performance comparison (no inlining, no
    // vectorization, panics-as-checks left in) — refuse to run instead of silently producing
    // numbers that look real but aren't.
    if cfg!(debug_assertions)
    {
        eprintln!("error: refusing to run benchmarks in a debug build — the numbers would not be representative.");
        eprintln!("run with `cargo run --release` instead.");
        std::process::exit(1);
    }

    spin_warm_cpu(Duration::from_millis(200));

    let mut rng = Rng::seeded();
    let mut results = Vec::new();
    for layout in ArchetypeLayout::ALL
    {
        for &entity_count in &ENTITY_COUNTS
        {
            for component_count in [1u8, 2, 3]
            {
                // Randomize which competitor runs first/last within this (layout, entity_count,
                // component_count) group so systematic effects (thermal throttling, frequency
                // ramp drift across the whole run) don't always penalize the same library.
                let mut runners = runners_for_arity(component_count);
                rng.shuffle(&mut runners);
                for runner in runners
                {
                    results.push(runner(entity_count, layout));
                }
            }
        }
    }

    report::table::print(&results);

    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("output");
    let json_path = out_dir.join("results.json");
    let html_path = out_dir.join("report.html");

    report::json::write(&results, &json_path).expect("failed to write results.json");
    report::html::write(&results, &html_path).expect("failed to write report.html");

    println!("wrote {}", json_path.display());
    println!("wrote {}", html_path.display());
}
