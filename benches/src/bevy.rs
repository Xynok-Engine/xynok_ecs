//! `bevy_ecs` under test.

use bevy_ecs::prelude::World;
use bevy_ecs::query::QueryState;

use crate::workload::{
    ArchetypeLayout, Health, MarkerA, MarkerB, MarkerC, MarkerD, ParallelWorkload, Position, QueryWorkload, Velocity, seed_health, seed_position,
    seed_velocity, split_counts,
};

pub fn build_world(entity_count: usize, layout: ArchetypeLayout) -> World
{
    let mut world = World::new();
    let mut i = 0usize;
    for (group, count) in split_counts(entity_count, layout.group_count()).into_iter().enumerate()
    {
        for _ in 0..count
        {
            match group
            {
                0 => world.spawn((seed_position(i), seed_velocity(i), seed_health(i))),
                1 => world.spawn((seed_position(i), seed_velocity(i), seed_health(i), MarkerA)),
                2 => world.spawn((seed_position(i), seed_velocity(i), seed_health(i), MarkerB)),
                3 => world.spawn((seed_position(i), seed_velocity(i), seed_health(i), MarkerC)),
                _ => world.spawn((seed_position(i), seed_velocity(i), seed_health(i), MarkerD)),
            };
            i += 1;
        }
    }
    world
}

pub struct Query1;

impl QueryWorkload for Query1
{
    type PreparedQuery = QueryState<&'static mut Position>;
    type Storage = World;

    const COMPONENT_COUNT: u8 = 1;
    const NAME: &'static str = "bevy_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> World
    {
        build_world(entity_count, layout)
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        storage.query::<&mut Position>()
    }

    fn run_query_once(storage: &mut World, query: &mut Self::PreparedQuery)
    {
        for mut position in query.iter_mut(storage)
        {
            position.x += 1.0;
            position.y += 1.0;
        }
    }
}

pub struct Query2;

impl QueryWorkload for Query2
{
    type PreparedQuery = QueryState<(&'static mut Position, &'static Velocity)>;
    type Storage = World;

    const COMPONENT_COUNT: u8 = 2;
    const NAME: &'static str = "bevy_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> World
    {
        build_world(entity_count, layout)
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        storage.query::<(&mut Position, &Velocity)>()
    }

    fn run_query_once(storage: &mut World, query: &mut Self::PreparedQuery)
    {
        for (mut position, velocity) in query.iter_mut(storage)
        {
            position.x += velocity.x;
            position.y += velocity.y;
        }
    }
}

pub struct Query3;

impl QueryWorkload for Query3
{
    type PreparedQuery = QueryState<(&'static mut Position, &'static Velocity, &'static mut Health)>;
    type Storage = World;

    const COMPONENT_COUNT: u8 = 3;
    const NAME: &'static str = "bevy_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> World
    {
        build_world(entity_count, layout)
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        storage.query::<(&mut Position, &Velocity, &mut Health)>()
    }

    fn run_query_once(storage: &mut World, query: &mut Self::PreparedQuery)
    {
        for (mut position, velocity, mut health) in query.iter_mut(storage)
        {
            position.x += velocity.x;
            position.y += velocity.y;
            health.value -= 0.1;
        }
    }
}

pub struct ParallelStorage
{
    world: World,
    query: QueryState<(&'static mut Position, &'static Velocity)>,
}

pub struct Parallel;

impl ParallelWorkload for Parallel
{
    type Storage = ParallelStorage;

    const NAME: &'static str = "bevy_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout, threads: usize) -> Self::Storage
    {
        // `ComputeTaskPool` is process-global and can only be built once, which is why the thread
        // count for a whole benchmark process comes from the environment. See
        // `crate::config::init_bevy_task_pool`.
        crate::config::init_bevy_task_pool(threads);

        let mut world = build_world(entity_count, layout);
        let query = world.query::<(&mut Position, &Velocity)>();
        ParallelStorage { world: world, query: query }
    }

    fn run_parallel(storage: &mut Self::Storage)
    {
        let ParallelStorage { world, query } = storage;
        // The default batching strategy on purpose: it is what a bevy user gets without tuning, and
        // it is the policy `xynok::Parallel` is set up to mirror.
        query.par_iter_mut(world).for_each(|(mut position, velocity)| {
            position.x += velocity.x;
            position.y += velocity.y;
        });
    }

    fn run_sequential(storage: &mut Self::Storage)
    {
        let ParallelStorage { world, query } = storage;
        for (mut position, velocity) in query.iter_mut(world)
        {
            position.x += velocity.x;
            position.y += velocity.y;
        }
    }
}
