use xynok_ecs::query::Query;
use xynok_ecs::world::World;

use crate::common::{seed_position, seed_velocity, EcsBenchmark, Position, Velocity};

pub struct XynokBenchmark;

impl EcsBenchmark for XynokBenchmark
{
    type Storage = World;
    type PreparedQuery = Query<(&'static mut Position, &'static Velocity)>;

    const NAME: &'static str = "xynok_ecs";

    fn setup(entity_count: usize) -> World
    {
        let mut world = World::default();
        for i in 0..entity_count
        {
            world.create((seed_position(i), seed_velocity(i)));
        }
        world
    }

    fn prepare_query(storage: &mut World) -> Self::PreparedQuery
    {
        storage.create_query::<(&mut Position, &Velocity)>()
    }

    fn run_query_once(_storage: &mut World, query: &mut Self::PreparedQuery)
    {
        for (pos, vel) in &*query
        {
            pos.x += vel.x;
            pos.y += vel.y;
        }
    }
}
