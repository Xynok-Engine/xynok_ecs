mod alloc_probe;
mod bevy_bench;
mod common;
mod report;
mod vec_bench;
mod xynok_bench;

use std::path::Path;
use std::time::Instant;

use common::{EcsBenchmark, ENTITY_COUNTS, MEASURED_ITERS, WARMUP_ITERS};
use report::{median_ns, AllocStats, BenchResult, QueryTiming};

#[global_allocator]
static ALLOC_PROBE: alloc_probe::CountingAllocator = alloc_probe::CountingAllocator;

fn run_benchmark<B: EcsBenchmark>(entity_count: usize) -> BenchResult
{
    let baseline = alloc_probe::snapshot();

    // --- setup: allocation is expected and measured here, not held against query speed ---
    let before_setup = alloc_probe::snapshot();
    let mut storage = B::setup(entity_count);
    let after_setup = alloc_probe::snapshot();
    let setup_delta = alloc_probe::delta(&before_setup, &after_setup);

    let mut query = B::prepare_query(&mut storage);

    // Warm up caches (query plans, first-touch pages, ...) outside the measured region so the
    // timed loop below reflects steady-state iteration, not one-time setup cost.
    for _ in 0..WARMUP_ITERS
    {
        B::run_query_once(&mut storage, &mut query);
    }

    // Pre-reserve the sample buffer *before* the measured region so pushing into it can't
    // register as "query allocation" and pollute the isolation check below.
    let mut samples: Vec<u128> = Vec::with_capacity(MEASURED_ITERS);

    let before_query = alloc_probe::snapshot();
    for _ in 0..MEASURED_ITERS
    {
        let t0 = Instant::now();
        B::run_query_once(&mut storage, &mut query);
        samples.push(t0.elapsed().as_nanos());
    }
    let after_query = alloc_probe::snapshot();
    let query_delta = alloc_probe::delta(&before_query, &after_query);

    // Compute stats from `samples` first, then drop every harness-owned allocation (including
    // `samples` itself) before the leak snapshot below — otherwise the harness's own bookkeeping
    // would be misreported as a leak in whichever library happened to run.
    let min_ns = *samples.iter().min().unwrap();
    let max_ns = *samples.iter().max().unwrap();
    let mean_ns = samples.iter().sum::<u128>() / samples.len() as u128;
    let median_ns = median_ns(&mut samples);

    drop(samples);
    drop(query);
    drop(storage);
    let after_drop = alloc_probe::snapshot();
    let leaked_bytes = alloc_probe::leaked_bytes(&baseline, &after_drop);

    BenchResult {
        library: B::NAME.to_string(),
        entity_count,
        setup_alloc: AllocStats {
            bytes: setup_delta.bytes,
            allocations: setup_delta.allocations,
        },
        query_alloc: AllocStats {
            bytes: query_delta.bytes,
            allocations: query_delta.allocations,
        },
        leaked_bytes: leaked_bytes as i64,
        query_timing: QueryTiming {
            warmup_iters: WARMUP_ITERS,
            measured_iters: MEASURED_ITERS,
            min_ns,
            max_ns,
            mean_ns,
            median_ns,
        },
    }
}

fn main()
{
    if cfg!(debug_assertions)
    {
        eprintln!("warning: running an unoptimized (debug) build; use `cargo run --release` for meaningful speed numbers.\n");
    }

    let mut results = Vec::new();
    for &entity_count in &ENTITY_COUNTS
    {
        results.push(run_benchmark::<xynok_bench::XynokBenchmark>(entity_count));
        results.push(run_benchmark::<bevy_bench::BevyBenchmark>(entity_count));
        results.push(run_benchmark::<vec_bench::VecBenchmark>(entity_count));
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
