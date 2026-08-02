//! The "no ECS" baseline: a plain `Vec<(Position, Velocity)>`, iterated with a straight
//! `iter_mut()`. There's no query to prepare, so this is roughly the speed of the loop itself
//! with none of the archetype/indirection overhead either real ECS pays for.
use crate::common::{seed_position, seed_velocity, EcsBenchmark, Position, Velocity};

pub struct VecBenchmark;

impl EcsBenchmark for VecBenchmark
{
    type Storage = Vec<(Position, Velocity)>;
    type PreparedQuery = ();

    const NAME: &'static str = "std::Vec";

    fn setup(entity_count: usize) -> Self::Storage
    {
        (0..entity_count).map(|i| (seed_position(i), seed_velocity(i))).collect()
    }

    fn prepare_query(_storage: &mut Self::Storage) -> Self::PreparedQuery
    {
    }

    fn run_query_once(storage: &mut Self::Storage, _query: &mut Self::PreparedQuery)
    {
        for (pos, vel) in storage.iter_mut()
        {
            pos.x += vel.x;
            pos.y += vel.y;
        }
    }
}
