//! Knobs the benchmarks read from the environment, and the one piece of process-global setup
//! neither library lets us avoid.

use std::thread::available_parallelism;

use bevy_tasks::{ComputeTaskPool, TaskPoolBuilder};

/// Worker threads to spawn for the parallel benchmark, on top of the calling thread.
///
/// `0` is meaningful: no worker is spawned and everything runs on the calling thread. Both
/// libraries count the same way here, so the same number describes both pools:
///
/// - `xynok_concurrency::pool::Config::threads` spawns that many workers, and the thread that calls
///   into the pool runs jobs alongside them.
/// - `TaskPoolBuilder::num_threads` spawns that many threads, and `TaskPool::scope` ticks the pool
///   executor on the calling thread too.
///
/// So `threads = N` means `N + 1` threads doing work in both cases.
///
/// The default is `cores - 1`, which puts one participant on each core. That also matches
/// `xynok_concurrency::pool::Config::default`, so the default benchmark run is the configuration a
/// real engine would ship with.
///
/// # Sweeping
///
/// This is read once per process because `ComputeTaskPool` is a global `OnceLock`: bevy's thread
/// count can be chosen exactly once and never changed. To get a scaling curve, run the benchmark
/// once per thread count. `scripts/parallel_scaling.sh` does that, and the thread count is part of
/// every benchmark id, so criterion files the runs side by side instead of overwriting them.
pub fn worker_threads() -> usize
{
    if let Ok(raw) = std::env::var(WORKER_THREADS_ENV)
        && let Ok(parsed) = raw.trim().parse::<usize>()
    {
        return parsed;
    }
    available_cores().saturating_sub(1)
}

pub const WORKER_THREADS_ENV: &str = "XYNOK_BENCH_THREADS";
pub const BATCHES_ENV: &str = "XYNOK_BENCH_BATCHES_PER_THREAD";

/// How many lots each participant should have to pick from in the `xynok_ecs` parallel query.
///
/// `1` is the default because it is what bevy's [`BatchingStrategy`] uses for `batches_per_thread`,
/// and comparing two libraries under two different splitting policies would say more about the
/// policies than about the libraries.
///
/// The other half of matching that policy is what the number divides, and it is not the total: bevy
/// divides the size of the **largest** matched table, so a layout split across five archetypes
/// yields five times as many batches as a single archetype one. `xynok::Parallel::setup` does the
/// same. Getting this wrong is not a small mis-tuning, it is measuring the two libraries under
/// different granularity and reporting the difference as a property of the schedulers.
///
/// Raise it to trade scheduling overhead for better load balancing. That is worth probing on a
/// machine with performance and efficiency cores, where one lot per participant means the pass ends
/// when the slowest core finishes and there is nothing left to hand to anyone else.
///
/// [`BatchingStrategy`]: bevy_ecs::batching::BatchingStrategy
pub fn batches_per_participant() -> usize
{
    if let Ok(raw) = std::env::var(BATCHES_ENV)
        && let Ok(parsed) = raw.trim().parse::<usize>()
        && parsed > 0
    {
        return parsed;
    }
    1
}

pub fn available_cores() -> usize
{
    available_parallelism().map(|n| n.get()).unwrap_or(1)
}

/// Builds bevy's global compute pool with `threads` workers, or checks that the one already built
/// has exactly that many.
///
/// `ComputeTaskPool` is a `OnceLock`, so the second call with a different count would be ignored in
/// silence and every later number would be attributed to the wrong thread count. Failing loudly
/// here is the only way that mistake stays visible.
pub fn init_bevy_task_pool(threads: usize) -> usize
{
    let pool = ComputeTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(threads).thread_name("bevy-bench".to_string()).build());

    let actual = pool.thread_num();
    assert_eq!(
        actual, threads,
        "bevy's ComputeTaskPool was already built with {actual} threads and cannot be rebuilt with {threads}. \
         One benchmark process measures one thread count: set {WORKER_THREADS_ENV} and run it again."
    );
    actual
}

/// Refuses to run in a debug build.
///
/// Without inlining and vectorisation the numbers are not wrong so much as unrelated to what the
/// libraries do in a real build, and they look every bit as real as the ones that matter. For the
/// binaries, where the user picks the profile, that is worth stopping over.
pub fn require_release_build()
{
    if cfg!(debug_assertions)
    {
        eprintln!("error: this is a debug build, and its numbers would not mean anything.");
        eprintln!("       run `cargo run --release` instead.");
        std::process::exit(1);
    }
}

/// The same check for the criterion targets, which only warn.
///
/// `cargo bench` always builds these with optimisations, so a debug build here means `cargo test
/// --benches`, where criterion runs one iteration of each benchmark purely to check it still
/// compiles and does not panic. Exiting would turn that useful check into a failing test suite.
pub fn warn_if_debug_build()
{
    if cfg!(debug_assertions)
    {
        eprintln!("warning: debug build, timings from this run are not meaningful. Use `cargo bench`.");
    }
}
