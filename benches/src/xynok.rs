//! `xynok_ecs` under test. Preparation warms the world-owned query cache; each timed
//! traversal reacquires a borrowing view without retaining a detached mutable query.

use xynok_ecs::world::World;

use crate::workload::{
    ArchetypeLayout, Health, MarkerA, MarkerB, MarkerC, MarkerD, Position, QueryWorkload, Velocity, seed_health, seed_position, seed_velocity, split_counts,
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
    type PreparedQuery = ();
    type Storage = World;

    const COMPONENT_COUNT: u8 = 1;
    const DISPLAY_NAME: &'static str = "xynok_ecs";
    const NAME: &'static str = "xynok_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> World
    {
        build_world(entity_count, layout)
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        let _ = storage.create_query::<&mut Position>();
    }

    fn run_query_once(storage: &mut World, _query: &mut Self::PreparedQuery)
    {
        for position in storage.create_query::<&mut Position>()
        {
            position.x += 1.0;
            position.y += 1.0;
        }
    }
}

pub struct Query2;

impl QueryWorkload for Query2
{
    type PreparedQuery = ();
    type Storage = World;

    const COMPONENT_COUNT: u8 = 2;
    const DISPLAY_NAME: &'static str = "xynok_ecs";
    const NAME: &'static str = "xynok_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> World
    {
        build_world(entity_count, layout)
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        let _ = storage.create_query::<(&mut Position, &Velocity)>();
    }

    fn run_query_once(storage: &mut World, _query: &mut Self::PreparedQuery)
    {
        for (position, velocity) in storage.create_query::<(&mut Position, &Velocity)>()
        {
            position.x += velocity.x;
            position.y += velocity.y;
        }
    }
}

pub struct Query3;

impl QueryWorkload for Query3
{
    type PreparedQuery = ();
    type Storage = World;

    const COMPONENT_COUNT: u8 = 3;
    const DISPLAY_NAME: &'static str = "xynok_ecs";
    const NAME: &'static str = "xynok_ecs";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> World
    {
        build_world(entity_count, layout)
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        let _ = storage.create_query::<(&mut Position, &Velocity, &mut Health)>();
    }

    fn run_query_once(storage: &mut World, _query: &mut Self::PreparedQuery)
    {
        for (position, velocity, health) in storage.create_query::<(&mut Position, &Velocity, &mut Health)>()
        {
            position.x += velocity.x;
            position.y += velocity.y;
            health.value -= 0.1;
        }
    }
}
