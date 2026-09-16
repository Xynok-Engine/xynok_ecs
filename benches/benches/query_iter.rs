//! The three ways to walk a query: `iter`, `iter_chunk` and `iter_batch`, each against the closest
//! thing `bevy_ecs` has.
//!
//! ```bash
//! cargo bench -p xynok_ecs_benches --bench query_iter
//! ```
//!
//! Every benchmark does the same pass as the 3 component case in `benches/query.rs`
//! (`position += velocity; health -= 0.1`), over the world `xynok::build_world` and
//! `bevy::build_world` build, split across 5 archetypes. Five archetypes is the layout where how an
//! iterator crosses from one archetype or chunk to the next actually shows up in the numbers.
//!
//! Benchmark ids read `query_iter/<mode>/<library>/<entity count>`:
//!
//! - **iter**: `Query::iter` against bevy's `QueryState::iter_mut`. Both walk one entity at a time
//!   on one thread.
//! - **iter_chunk**: a `Query::iter_chunk` loop with an inner loop per chunk. bevy has no public
//!   per chunk iteration, so its side is `iter_mut` again. What this asks is whether the nested
//!   loop costs anything next to a flat walk.
//! - **iter_batch**: `Query::iter_batch` with every batch spawned on the task pool, against bevy's
//!   `par_iter_mut` with the same fixed batch size. Both run on the one `ComputeTaskPool`, pinned
//!   to `WORKER_THREADS` workers, so the only difference is how each library cuts the rows and
//!   hands them out.
//!
//! The layout and batch size live in `iter_modes`, so the report reads the same numbers.
//!
//! As in `benches/query.rs`, the world is built once outside the timed region, and the two
//! libraries run back to back for every scenario.

use std::hint::black_box;
use std::time::Duration;

use bevy_ecs::batching::BatchingStrategy;
use bevy_ecs::query::QueryState;
use bevy_tasks::ComputeTaskPool;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use xynok_ecs_benches::config::warn_if_debug_build;
use xynok_ecs_benches::iter_modes::{ITER_BATCH_SIZE, ITER_MODE_LAYOUT};
use xynok_ecs_benches::parallel::bevy::init_task_pool;
use xynok_ecs_benches::workload::{ArchetypeLayout, ENTITY_COUNTS, Health, Position, Velocity, count_label};
use xynok_ecs_benches::{bevy, xynok};

const WARM_UP: Duration = Duration::from_secs(1);
const MEASUREMENT: Duration = Duration::from_secs(3);

const LAYOUT: ArchetypeLayout = ITER_MODE_LAYOUT;
const BATCH_SIZE: usize = ITER_BATCH_SIZE;

type BevyQuery = QueryState<(&'static mut Position, &'static Velocity, &'static mut Health)>;

fn bench_iter(c: &mut Criterion)
{
    let mut group = c.benchmark_group("query_iter/iter");
    group.warm_up_time(WARM_UP);
    group.measurement_time(MEASUREMENT);

    for &entity_count in &ENTITY_COUNTS
    {
        group.throughput(Throughput::Elements(entity_count as u64));

        let mut world = xynok::build_world(entity_count, LAYOUT);
        group.bench_function(BenchmarkId::new("xynok_ecs", count_label(entity_count)), |b| {
            b.iter(|| {
                let mut query = black_box(&mut world).create_query::<(&mut Position, &Velocity, &mut Health)>();
                for (position, velocity, health) in query.iter()
                {
                    position.x += velocity.x;
                    position.y += velocity.y;
                    health.value -= 0.1;
                }
            });
        });

        bench_bevy_iter_mut(&mut group, entity_count);
    }

    group.finish();
}

fn bench_iter_chunk(c: &mut Criterion)
{
    let mut group = c.benchmark_group("query_iter/iter_chunk");
    group.warm_up_time(WARM_UP);
    group.measurement_time(MEASUREMENT);

    for &entity_count in &ENTITY_COUNTS
    {
        group.throughput(Throughput::Elements(entity_count as u64));

        let mut world = xynok::build_world(entity_count, LAYOUT);
        group.bench_function(BenchmarkId::new("xynok_ecs", count_label(entity_count)), |b| {
            b.iter(|| {
                let mut query = black_box(&mut world).create_query::<(&mut Position, &Velocity, &mut Health)>();
                for chunk in query.iter_chunk()
                {
                    for (position, velocity, health) in chunk
                    {
                        position.x += velocity.x;
                        position.y += velocity.y;
                        health.value -= 0.1;
                    }
                }
            });
        });

        bench_bevy_iter_mut(&mut group, entity_count);
    }

    group.finish();
}

fn bench_iter_batch(c: &mut Criterion)
{
    init_task_pool();

    let mut group = c.benchmark_group("query_iter/iter_batch");
    group.warm_up_time(WARM_UP);
    group.measurement_time(MEASUREMENT);

    for &entity_count in &ENTITY_COUNTS
    {
        group.throughput(Throughput::Elements(entity_count as u64));

        let mut world = xynok::build_world(entity_count, LAYOUT);
        group.bench_function(BenchmarkId::new("xynok_ecs", count_label(entity_count)), |b| {
            b.iter(|| {
                let mut query = black_box(&mut world).create_query::<(&mut Position, &Velocity, &mut Health)>();
                ComputeTaskPool::get().scope(|scope| {
                    for batch in query.iter_batch(BATCH_SIZE)
                    {
                        scope.spawn(async move {
                            for (position, velocity, health) in batch
                            {
                                position.x += velocity.x;
                                position.y += velocity.y;
                                health.value -= 0.1;
                            }
                        });
                    }
                });
            });
        });

        let mut world = bevy::build_world(entity_count, LAYOUT);
        let mut query: BevyQuery = world.query();
        group.bench_function(BenchmarkId::new("bevy_ecs", count_label(entity_count)), |b| {
            b.iter(|| {
                query
                    .par_iter_mut(black_box(&mut world))
                    .batching_strategy(BatchingStrategy::fixed(BATCH_SIZE))
                    .for_each(|(mut position, velocity, mut health)| {
                        position.x += velocity.x;
                        position.y += velocity.y;
                        health.value -= 0.1;
                    });
            });
        });
    }

    group.finish();
}

/// bevy's side of both single threaded modes: it has one way to walk a query, so it is the same
/// loop in both groups.
fn bench_bevy_iter_mut(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>, entity_count: usize)
{
    let mut world = bevy::build_world(entity_count, LAYOUT);
    let mut query: BevyQuery = world.query();
    group.bench_function(BenchmarkId::new("bevy_ecs", count_label(entity_count)), |b| {
        b.iter(|| {
            for (mut position, velocity, mut health) in query.iter_mut(black_box(&mut world))
            {
                position.x += velocity.x;
                position.y += velocity.y;
                health.value -= 0.1;
            }
        });
    });
}

fn query_iter_benches(c: &mut Criterion)
{
    warn_if_debug_build();

    bench_iter(c);
    bench_iter_chunk(c);
    bench_iter_batch(c);
}

criterion_group!(query_iter, query_iter_benches);
criterion_main!(query_iter);
