//! `xynok_ecs` under test.

use xynok_concurrency::pool::{Config as PoolConfig, ThreadPool};
use xynok_ecs::query::Query;
use xynok_ecs::world::World;

use crate::config::batches_per_participant;
use crate::workload::{
    ArchetypeLayout, Health, MarkerA, MarkerB, MarkerC, MarkerD, ParallelWorkload, Position, QueryWorkload, Velocity, seed_health, seed_position,
    seed_velocity, split_counts,
};

/// Every entity always carries `(Position, Velocity, Health)`. `layout` only decides whether they
/// all land in one archetype or get fanned out across several via an extra tag component. Which
/// subset of those three actually gets queried is decided per benchmark type below, independently
/// of how the storage was built.
pub fn build_world(entity_count: usize, layout: ArchetypeLayout) -> World
{
    let mut world = World::default();
    let mut i = 0usize;
    for (group, count) in split_counts(entity_count, layout.group_count()).into_iter().enumerate()
    {
        for _ in 0..count
        {
            match group
            {
                0 => world.create((seed_position(i), seed_velocity(i), seed_health(i))),
                1 => world.create((seed_position(i), seed_velocity(i), seed_health(i), MarkerA)),
                2 => world.create((seed_position(i), seed_velocity(i), seed_health(i), MarkerB)),
                3 => world.create((seed_position(i), seed_velocity(i), seed_health(i), MarkerC)),
                _ => world.create((seed_position(i), seed_velocity(i), seed_health(i), MarkerD)),
            };
            i += 1;
        }
    }
    world
}

pub struct Query1;

impl QueryWorkload for Query1
{
    type PreparedQuery = Query<'static, &'static mut Position>;
    type Storage = World;

    const COMPONENT_COUNT: u8 = 1;
    const NAME: &'static str = "xynok_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> World
    {
        build_world(entity_count, layout)
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        storage.create_query::<&mut Position>()
    }

    fn run_query_once(_storage: &mut World, query: &mut Self::PreparedQuery)
    {
        for position in *query
        {
            position.x += 1.0;
            position.y += 1.0;
        }
    }
}

pub struct Query2;

impl QueryWorkload for Query2
{
    type PreparedQuery = Query<'static, (&'static mut Position, &'static Velocity)>;
    type Storage = World;

    const COMPONENT_COUNT: u8 = 2;
    const NAME: &'static str = "xynok_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> World
    {
        build_world(entity_count, layout)
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        storage.create_query::<(&mut Position, &Velocity)>()
    }

    fn run_query_once(_storage: &mut World, query: &mut Self::PreparedQuery)
    {
        for (position, velocity) in *query
        {
            position.x += velocity.x;
            position.y += velocity.y;
        }
    }
}

pub struct Query3;

impl QueryWorkload for Query3
{
    type PreparedQuery = Query<'static, (&'static mut Position, &'static Velocity, &'static mut Health)>;
    type Storage = World;

    const COMPONENT_COUNT: u8 = 3;
    const NAME: &'static str = "xynok_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> World
    {
        build_world(entity_count, layout)
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        storage.create_query::<(&mut Position, &Velocity, &mut Health)>()
    }

    fn run_query_once(_storage: &mut World, query: &mut Self::PreparedQuery)
    {
        for (position, velocity, health) in *query
        {
            position.x += velocity.x;
            position.y += velocity.y;
            health.value -= 0.1;
        }
    }
}

type ParQuery = Query<'static, (&'static mut Position, &'static Velocity)>;

/// World, pool and query kept together so a benchmark iteration is one call and nothing gets
/// rebuilt between samples.
pub struct ParallelStorage
{
    /// Boxed on purpose. [`Query`] holds raw pointers into the world it was created from, and this
    /// struct gets moved into the benchmark closure after the query exists. Moving a `Box` moves
    /// the pointer, not the `World`, so those pointers stay valid.
    _world: Box<World>,
    query:  ParQuery,
    pool:   ThreadPool,
    /// Chunks per job. See [`crate::config::batches_per_participant`] for where the number comes
    /// from.
    batch:  usize,
}

pub struct Parallel;

impl ParallelWorkload for Parallel
{
    type Storage = ParallelStorage;

    const NAME: &'static str = "xynok_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout, threads: usize) -> Self::Storage
    {
        let mut world = Box::new(build_world(entity_count, layout));
        let query: ParQuery = world.create_query::<(&mut Position, &Velocity)>();

        let pool = ThreadPool::new(PoolConfig {
            threads: threads,
            thread_name: "xynok-bench".to_string(),
            ..PoolConfig::default()
        });

        // The lot size is derived from what the query actually touches, so it tracks the entity
        // count and the layout instead of being a constant that happens to suit one of them.
        let mut chunk_count = 0usize;
        query.for_each_chunk(|_| chunk_count += 1);

        // Divided by the chunks in the **largest** archetype, not by the total, because that is
        // what bevy does: `QueryParIter::get_batch_size` takes `.max()` over the matched tables and
        // divides that by the thread count. So a 5 archetype layout gives bevy 5 times as many
        // batches as a single archetype one, and dividing the total here instead would hand bevy
        // five lots for every one of ours and then call the difference a result about the two
        // libraries.
        //
        // Every archetype in this workload holds the same number of entities by construction, so
        // the largest one is the total split evenly. See `workload::split_counts`.
        let largest_archetype = chunk_count.div_ceil(layout.group_count());
        let jobs = pool.worker_count() * batches_per_participant();
        let batch = largest_archetype.div_ceil(jobs.max(1)).max(1);

        ParallelStorage {
            _world: world,
            query:  query,
            pool:   pool,
            batch:  batch,
        }
    }

    fn run_parallel(storage: &mut Self::Storage)
    {
        storage.query.par_for_each_chunk(&storage.pool, storage.batch, |view| {
            let (positions, velocities) = view.columns;
            for (position, velocity) in positions.iter_mut().zip(velocities.iter())
            {
                position.x += velocity.x;
                position.y += velocity.y;
            }
        });
    }

    fn run_sequential(storage: &mut Self::Storage)
    {
        storage.query.for_each_chunk(|view| {
            let (positions, velocities) = view.columns;
            for (position, velocity) in positions.iter_mut().zip(velocities.iter())
            {
                position.x += velocity.x;
                position.y += velocity.y;
            }
        });
    }
}
