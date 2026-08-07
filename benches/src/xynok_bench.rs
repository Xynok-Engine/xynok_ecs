use xynok_ecs::query::Query;
use xynok_ecs::world::World;

use crate::common::{
    seed_health, seed_position, seed_velocity, split_counts, ArchetypeLayout, EcsBenchmark, Health, MarkerA, MarkerB, MarkerC, MarkerD, Position, Velocity,
    FRAGMENTED_ARCHETYPES,
};

/// Every entity always carries `(Position, Velocity, Health)`; `layout` only decides whether they
/// all land in one archetype or get fanned out across `FRAGMENTED_ARCHETYPES` of them via an extra
/// tag component. Which subset of those 3 components gets queried is decided per-benchmark-struct
/// below, independently of how the storage was built.
fn build_world(entity_count: usize, layout: ArchetypeLayout) -> World
{
    let mut world = World::default();
    match layout
    {
        ArchetypeLayout::Single =>
        {
            for i in 0..entity_count
            {
                world.create((seed_position(i), seed_velocity(i), seed_health(i)));
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
                        0 => world.create((seed_position(i), seed_velocity(i), seed_health(i))),
                        1 => world.create((seed_position(i), seed_velocity(i), seed_health(i), MarkerA)),
                        2 => world.create((seed_position(i), seed_velocity(i), seed_health(i), MarkerB)),
                        3 => world.create((seed_position(i), seed_velocity(i), seed_health(i), MarkerC)),
                        _ => world.create((seed_position(i), seed_velocity(i), seed_health(i), MarkerD)),
                    };
                    i += 1;
                }
            }
        }
    }
    world
}

pub struct XynokBenchmark1;

impl EcsBenchmark for XynokBenchmark1
{
    type Storage = World;
    type PreparedQuery = Query<'static, &'static mut Position>;

    const NAME: &'static str = "xynok_ecs";
    const COMPONENT_COUNT: u8 = 1;

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
        for pos in *query
        {
            pos.x += 1.0;
            pos.y += 1.0;
        }
    }
}

pub struct XynokBenchmark2;

impl EcsBenchmark for XynokBenchmark2
{
    type Storage = World;
    type PreparedQuery = Query<'static, (&'static mut Position, &'static Velocity)>;

    const NAME: &'static str = "xynok_ecs";
    const COMPONENT_COUNT: u8 = 2;

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
        for (pos, vel) in *query
        {
            pos.x += vel.x;
            pos.y += vel.y;
        }
    }
}

pub struct XynokBenchmark3;

impl EcsBenchmark for XynokBenchmark3
{
    type Storage = World;
    type PreparedQuery = Query<'static, (&'static mut Position, &'static Velocity, &'static mut Health)>;

    const NAME: &'static str = "xynok_ecs";
    const COMPONENT_COUNT: u8 = 3;

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
        for (pos, vel, health) in *query
        {
            pos.x += vel.x;
            pos.y += vel.y;
            health.value -= 0.1;
        }
    }
}
