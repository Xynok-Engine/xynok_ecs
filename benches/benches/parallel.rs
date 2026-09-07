//! Multi-threaded scheduling: `xynok_ecs` against `bevy_ecs`.
//!
//! ```bash
//! cargo bench -p xynok_ecs_benches --bench parallel
//! ```
//!
//! `benches/query.rs` measures one query walking its entities on one thread. This target measures
//! the layer above: a schedule handed a group of systems that cannot conflict, run once per
//! iteration, with the library free to spread them over its worker threads.
//!
//! One benchmark is one frame: the whole group runs, and the frame is done when the slowest system
//! in it is. The world and the schedule are built once, before the timed region, and the first
//! frame runs untimed so neither library pays its one-off setup inside the measurement.
//!
//! Benchmark ids read `parallel/<n>_systems/<library>/<entity count>`, the same shape the query
//! benchmark uses, so criterion puts the two libraries on one chart per scenario.
//!
//! What each axis is asking:
//!
//! - **systems**: 2 and 4. Both schedulers run four workers here, so a group of 4 is the largest
//!   one that still gets a thread each. Reading the two together says whether a scheduler charges
//!   its overhead once per group or once per system.
//! - **entity count**: 1k, 10k, 100k. At 1k each system is a few microseconds of work, which is
//!   the range where handing it to another thread can easily cost more than it saves. At 100k the
//!   work dominates and what is left is how well the group actually spreads.
//!
//! Throughput is set to `entity count x system count`: every system walks every entity, so that
//! product is the number of entity-visits one frame performs, and `elem/s` stays comparable across
//! both axes as well as across the two libraries.
//!
//! Passing order is deliberate: for each scenario the two libraries run back to back, so a machine
//! that heats up or drifts in clock speed over a long run drags both the same way instead of
//! penalising whichever one was registered last.

use std::hint::black_box;
use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use xynok_ecs_benches::config::warn_if_debug_build;
use xynok_ecs_benches::parallel::{PARALLEL_ENTITY_COUNTS, ParallelWorkload, SystemGroup, bevy, count_label, xynok};

/// A frame here is heavier than a single query pass, and there are only 12 benchmarks (2 group
/// sizes x 3 sizes x 2 libraries), so this can afford a longer window than `query.rs` takes. The
/// extra time is worth having: thread hand-off is noisier than a straight loop, and the confidence
/// intervals tighten up noticeably with more samples.
const WARM_UP: Duration = Duration::from_secs(1);
const MEASUREMENT: Duration = Duration::from_secs(5);

fn bench_one<W: ParallelWorkload>(group: &mut BenchmarkGroup<'_, WallTime>, entity_count: usize, systems: SystemGroup)
{
    let mut runner = W::setup(entity_count, systems);

    group.bench_function(BenchmarkId::new(W::NAME, count_label(entity_count)), |b| {
        b.iter(|| W::run_frame(black_box(&mut runner)));
    });
}

/// One criterion group per group size, holding both libraries at every entity count.
fn bench_group_size(c: &mut Criterion, systems: SystemGroup)
{
    let mut group = c.benchmark_group(format!("parallel/{}", systems.slug()));
    group.warm_up_time(WARM_UP);
    group.measurement_time(MEASUREMENT);

    for &entity_count in &PARALLEL_ENTITY_COUNTS
    {
        group.throughput(Throughput::Elements((entity_count * systems.count()) as u64));
        bench_one::<xynok::ParallelFrame>(&mut group, entity_count, systems);
        bench_one::<bevy::ParallelFrame>(&mut group, entity_count, systems);
    }

    group.finish();
}

fn parallel_benches(c: &mut Criterion)
{
    warn_if_debug_build();

    for systems in SystemGroup::ALL
    {
        bench_group_size(c, systems);
    }
}

criterion_group!(parallel, parallel_benches);
criterion_main!(parallel);
