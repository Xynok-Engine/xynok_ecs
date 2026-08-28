//! Spreading one query pass across a thread pool: `xynok_ecs` against `bevy_ecs`.
//!
//! ```bash
//! cargo bench -p xynok_ecs_benches --bench parallel          # cores - 1 workers
//! XYNOK_BENCH_THREADS=4 cargo bench -p xynok_ecs_benches --bench parallel
//! ./benches/scripts/parallel_scaling.sh                      # the whole curve
//! ```
//!
//! No `std::Vec` here: there is no parallel query to compare against, and hand-rolling one would
//! only measure whichever splitting scheme this file happened to pick.
//!
//! # Reading the results
//!
//! Every scenario reports four series:
//!
//! | series | what it runs |
//! | --- | --- |
//! | `xynok_ecs_parallel` | `Query::par_for_each_chunk` on a `xynok_concurrency` pool |
//! | `bevy_ecs_parallel` | `QueryState::par_iter_mut` on bevy's `ComputeTaskPool` |
//! | `xynok_ecs_sequential` | `Query::for_each_chunk`, calling thread only |
//! | `bevy_ecs_sequential` | `QueryState::iter_mut`, calling thread only |
//!
//! The sequential pair is not filler. Speedup is what a parallel benchmark is actually about, and
//! comparing against each library's *own* single-threaded number is the only way to separate "spreads
//! work well" from "was faster to begin with". Both baselines are re-measured at every thread count
//! so they sit under the same thermal conditions as the parallel runs they are divided into.
//!
//! # Threads
//!
//! `XYNOK_BENCH_THREADS` is the number of worker threads to spawn, and it means the same thing to
//! both libraries: the calling thread runs jobs alongside those workers in either case, so `N` means
//! `N + 1` threads doing work. See `xynok_ecs_benches::config::worker_threads`.
//!
//! One process measures one thread count, because bevy's `ComputeTaskPool` is a global `OnceLock`
//! that can be sized exactly once. The count is baked into every benchmark id, so repeated runs at
//! different counts land next to each other in the criterion report rather than overwriting one
//! another. `scripts/parallel_scaling.sh` walks the usual counts.
//!
//! At `XYNOK_BENCH_THREADS=1` bevy's `par_iter_mut` sees a one-thread pool and deliberately falls
//! back to a plain sequential iterator, so `bevy_ecs_parallel` and `bevy_ecs_sequential` measure the
//! same thing there. That is bevy behaving sensibly, not a broken run.
//!
//! # The job size knob
//!
//! `xynok_ecs` takes the number of chunks per job as an argument; bevy works it out from its
//! `BatchingStrategy`. `XYNOK_BENCH_BATCHES_PER_THREAD` sets how many lots each participant gets to
//! pick from, and it defaults to 1 because that is what bevy's default strategy does, and comparing
//! two libraries under two different splitting policies would say more about the policies than about
//! the libraries.
//!
//! Note that bevy divides the size of the largest matched table, not the total, so the fragmented
//! layout gets five times the batches the single archetype one does. `xynok::Parallel::setup`
//! mirrors that.
//!
//! It is a diagnostic knob, not part of the benchmark id, so two runs that differ only in this value
//! write to the same criterion slot and the "change" line will read as a regression against the
//! previous setting. Use `--save-baseline <name>` when probing it.
//!
//! # Why the entity counts are large
//!
//! A `xynok_ecs` chunk is 16 KiB, and a `(Position, Velocity, Health)` row is around 32 bytes with
//! the entity id, so 100k entities is only a couple of hundred chunks. `position += velocity` is two
//! adds against memory that has to be fetched anyway, which puts the whole pass close to what the
//! memory bus can deliver. Below these sizes there is simply not enough work in a pass for splitting
//! it to pay for itself, and the benchmark would be reporting fork-join bookkeeping under the name
//! of parallel iteration.

use std::hint::black_box;
use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use xynok_ecs_benches::config::{batches_per_participant, init_bevy_task_pool, warn_if_debug_build, worker_threads};
use xynok_ecs_benches::workload::{ArchetypeLayout, PARALLEL_ENTITY_COUNTS, ParallelWorkload, count_label};
use xynok_ecs_benches::{bevy, xynok};

/// Both modes of one library, sharing a single storage so a 1M entity world is built once rather
/// than twice.
fn bench_library<P: ParallelWorkload>(group: &mut BenchmarkGroup<'_, WallTime>, entity_count: usize, layout: ArchetypeLayout, threads: usize)
{
    let mut storage = P::setup(entity_count, layout, threads);
    let label = count_label(entity_count);

    group.bench_function(BenchmarkId::new(format!("{}_parallel", P::NAME), &label), |b| {
        b.iter(|| P::run_parallel(black_box(&mut storage)));
    });
    group.bench_function(BenchmarkId::new(format!("{}_sequential", P::NAME), &label), |b| {
        b.iter(|| P::run_sequential(black_box(&mut storage)));
    });
}

fn parallel_benches(c: &mut Criterion)
{
    warn_if_debug_build();

    let threads = worker_threads();
    eprintln!(
        "parallel benchmark: {threads} worker threads + the calling thread, {} xynok job(s) per participant",
        batches_per_participant()
    );

    // Built here rather than lazily on the first bevy scenario. Bevy's pool is process-global and
    // outlives whatever created it, so leaving it to the first bevy `setup` would mean the first
    // xynok measurement runs on a machine with no bevy threads on it and every later one runs on a
    // machine that has them, parked but present. Creating it up front puts every measurement in the
    // run under the same conditions.
    init_bevy_task_pool(threads);

    for layout in ArchetypeLayout::ALL
    {
        let mut group = c.benchmark_group(format!("parallel/{}/{threads}_threads", layout.slug()));
        // A pass over 1M entities is milliseconds, not nanoseconds, so criterion needs longer than
        // its default window to collect a full set of samples without shrinking the sample count.
        group.warm_up_time(Duration::from_secs(2));
        group.measurement_time(Duration::from_secs(6));

        for &entity_count in &PARALLEL_ENTITY_COUNTS
        {
            group.throughput(Throughput::Elements(entity_count as u64));
            bench_library::<xynok::Parallel>(&mut group, entity_count, layout, threads);
            bench_library::<bevy::Parallel>(&mut group, entity_count, layout, threads);
        }

        group.finish();
    }
}

criterion_group!(parallel, parallel_benches);
criterion_main!(parallel);
