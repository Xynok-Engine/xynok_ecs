//! `bevy_ecs` under test, the reference this crate measures against.

use bevy_ecs::prelude::World;
use bevy_ecs::query::QueryState;

use crate::workload::{
    ArchetypeLayout, Health, MarkerA, MarkerB, MarkerC, MarkerD, Position, QueryWorkload, Velocity, seed_health, seed_position, seed_velocity, split_counts,
};

/// The same storage shape as `xynok::build_world`, built through bevy's own API.
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
    const DISPLAY_NAME: &'static str = "bevy_ecs";
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
    const DISPLAY_NAME: &'static str = "bevy_ecs";
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
    const DISPLAY_NAME: &'static str = "bevy_ecs";
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
