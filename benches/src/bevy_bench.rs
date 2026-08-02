use bevy_ecs::prelude::World;
use bevy_ecs::query::QueryState;

use crate::common::{seed_position, seed_velocity, EcsBenchmark, Position, Velocity};

pub struct BevyBenchmark;

impl EcsBenchmark for BevyBenchmark
{
    type Storage = World;
    type PreparedQuery = QueryState<(&'static mut Position, &'static Velocity)>;

    const NAME: &'static str = "bevy_ecs";

    fn setup(entity_count: usize) -> World
    {
        let mut world = World::new();
        for i in 0..entity_count
        {
            world.spawn((seed_position(i), seed_velocity(i)));
        }
        world
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
