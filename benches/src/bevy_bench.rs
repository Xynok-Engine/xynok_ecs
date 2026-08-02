use bevy_ecs::prelude::World;
use bevy_ecs::query::QueryState;

use crate::common::{
    ArchetypeLayout, EcsBenchmark, FRAGMENTED_ARCHETYPES, Health, MarkerA, MarkerB, MarkerC, MarkerD, Position, Velocity, seed_health, seed_position,
    seed_velocity, split_counts,
};

fn build_world(entity_count: usize, layout: ArchetypeLayout) -> World
{
    let mut world = World::new();
    match layout
    {
        ArchetypeLayout::Single =>
        {
            for i in 0..entity_count
            {
                world.spawn((seed_position(i), seed_velocity(i), seed_health(i)));
            }
        }
        ArchetypeLayout::Fragmented5 =>
        {
            let mut i = 0usize;
            for (group, count) in split_counts(entity_count, FRAGMENTED_ARCHETYPES).into_iter().enumerate()
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
        }
    }
    world
}

pub struct BevyBenchmark1;

impl EcsBenchmark for BevyBenchmark1
{
    type Storage = World;
    type PreparedQuery = QueryState<&'static mut Position>;

    const NAME: &'static str = "bevy_ecs";
    const COMPONENT_COUNT: u8 = 1;

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
        for mut pos in query.iter_mut(storage)
        {
            pos.x += 1.0;
            pos.y += 1.0;
        }
    }
}

pub struct BevyBenchmark2;

impl EcsBenchmark for BevyBenchmark2
{
    type Storage = World;
    type PreparedQuery = QueryState<(&'static mut Position, &'static Velocity)>;

    const NAME: &'static str = "bevy_ecs";
    const COMPONENT_COUNT: u8 = 2;

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
        for (mut pos, vel) in query.iter_mut(storage)
        {
            pos.x += vel.x;
            pos.y += vel.y;
        }
    }
}

pub struct BevyBenchmark3;

impl EcsBenchmark for BevyBenchmark3
{
    type Storage = World;
    type PreparedQuery = QueryState<(&'static mut Position, &'static Velocity, &'static mut Health)>;

    const NAME: &'static str = "bevy_ecs";
    const COMPONENT_COUNT: u8 = 3;

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
        for (mut pos, vel, mut health) in query.iter_mut(storage)
        {
            pos.x += vel.x;
            pos.y += vel.y;
            health.value -= 0.1;
        }
    }
}
