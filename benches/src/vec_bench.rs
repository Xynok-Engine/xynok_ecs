//! The "no ECS" baseline: a plain `Vec` of rows (or, for the fragmented layout, several
//! `Vec`s scanned back to back), iterated with a straight `iter_mut()`. There's no query or
//! archetype matching to do, so this is roughly the speed of the loop itself, plus — for the
//! fragmented layout — the cost of looping over several separate contiguous buffers instead of
//! one, with none of the real ECS's archetype-lookup overhead.
use crate::common::{seed_health, seed_position, seed_velocity, split_counts, ArchetypeLayout, EcsBenchmark, Health, Position, Velocity, FRAGMENTED_ARCHETYPES};

pub struct Row
{
    pub pos:    Position,
    pub vel:    Velocity,
    pub health: Health,
}

/// One `Vec` per archetype-equivalent chunk: 1 chunk for `Single`, `FRAGMENTED_ARCHETYPES` chunks
/// for `Fragmented5`, each holding an equal share of the entities.
pub struct ChunkedStorage
{
    pub chunks: Vec<Vec<Row>>,
}

fn build_storage(entity_count: usize, layout: ArchetypeLayout) -> ChunkedStorage
{
    let group_count = match layout
    {
        ArchetypeLayout::Single => 1,
        ArchetypeLayout::Fragmented5 => FRAGMENTED_ARCHETYPES,
    };
    let mut i = 0usize;
    let chunks = split_counts(entity_count, group_count)
        .into_iter()
        .map(|count| {
            (0..count)
                .map(|_| {
                    let row = Row {
                        pos:    seed_position(i),
                        vel:    seed_velocity(i),
                        health: seed_health(i),
                    };
                    i += 1;
                    row
                })
                .collect()
        })
        .collect();
    ChunkedStorage { chunks }
}

pub struct VecBenchmark1;

impl EcsBenchmark for VecBenchmark1
{
    type Storage = ChunkedStorage;
    type PreparedQuery = ();

    const NAME: &'static str = "std::Vec";
    const COMPONENT_COUNT: u8 = 1;

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> Self::Storage
    {
        build_storage(entity_count, layout)
    }

    fn prepare_query(_storage: &mut Self::Storage) -> Self::PreparedQuery {}

    fn run_query_once(storage: &mut Self::Storage, _query: &mut Self::PreparedQuery)
    {
        for row in storage.chunks.iter_mut().flatten()
        {
            row.pos.x += 1.0;
            row.pos.y += 1.0;
        }
    }
}

pub struct VecBenchmark2;

impl EcsBenchmark for VecBenchmark2
{
    type Storage = ChunkedStorage;
    type PreparedQuery = ();

    const NAME: &'static str = "std::Vec";
    const COMPONENT_COUNT: u8 = 2;

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> Self::Storage
    {
        build_storage(entity_count, layout)
    }

    fn prepare_query(_storage: &mut Self::Storage) -> Self::PreparedQuery {}

    fn run_query_once(storage: &mut Self::Storage, _query: &mut Self::PreparedQuery)
    {
        for row in storage.chunks.iter_mut().flatten()
        {
            row.pos.x += row.vel.x;
            row.pos.y += row.vel.y;
        }
    }
}

pub struct VecBenchmark3;

impl EcsBenchmark for VecBenchmark3
{
    type Storage = ChunkedStorage;
    type PreparedQuery = ();

    const NAME: &'static str = "std::Vec";
    const COMPONENT_COUNT: u8 = 3;

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> Self::Storage
    {
        build_storage(entity_count, layout)
    }

    fn prepare_query(_storage: &mut Self::Storage) -> Self::PreparedQuery {}

    fn run_query_once(storage: &mut Self::Storage, _query: &mut Self::PreparedQuery)
    {
        for row in storage.chunks.iter_mut().flatten()
        {
            row.pos.x += row.vel.x;
            row.pos.y += row.vel.y;
            row.health.value -= 0.1;
        }
    }
}
