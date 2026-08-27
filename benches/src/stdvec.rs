//! The "no ECS" baseline.
//!
//! A plain `Vec` of rows, or several of them scanned back to back for the fragmented layout,
//! iterated with a straight `iter_mut()`. There is no query to plan and no archetype to match, so
//! this is close to the cost of the loop itself plus the memory traffic the payload forces. It is
//! not a competitor so much as a floor: an ECS pays archetype lookup on top of this, and the gap is
//! what that machinery costs.

use crate::workload::{ArchetypeLayout, Health, Position, QueryWorkload, Velocity, seed_health, seed_position, seed_velocity, split_counts};

pub struct Row
{
    pub position: Position,
    pub velocity: Velocity,
    pub health:   Health,
}

/// One `Vec` per archetype-equivalent group, each holding an equal share of the entities.
pub struct ChunkedStorage
{
    pub groups: Vec<Vec<Row>>,
}

pub fn build_storage(entity_count: usize, layout: ArchetypeLayout) -> ChunkedStorage
{
    let mut i = 0usize;
    let groups = split_counts(entity_count, layout.group_count())
        .into_iter()
        .map(|count| {
            (0..count)
                .map(|_| {
                    let row = Row {
                        position: seed_position(i),
                        velocity: seed_velocity(i),
                        health:   seed_health(i),
                    };
                    i += 1;
                    row
                })
                .collect()
        })
        .collect();
    ChunkedStorage { groups: groups }
}

pub struct Query1;

impl QueryWorkload for Query1
{
    type PreparedQuery = ();
    type Storage = ChunkedStorage;

    const COMPONENT_COUNT: u8 = 1;
    const NAME: &'static str = "std_vec";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> Self::Storage
    {
        build_storage(entity_count, layout)
    }

    fn prepare_query(_storage: &mut Self::Storage) -> Self::PreparedQuery {}

    fn run_query_once(storage: &mut Self::Storage, _query: &mut Self::PreparedQuery)
    {
        for row in storage.groups.iter_mut().flatten()
        {
            row.position.x += 1.0;
            row.position.y += 1.0;
        }
    }
}

pub struct Query2;

impl QueryWorkload for Query2
{
    type PreparedQuery = ();
    type Storage = ChunkedStorage;

    const COMPONENT_COUNT: u8 = 2;
    const NAME: &'static str = "std_vec";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> Self::Storage
    {
        build_storage(entity_count, layout)
    }

    fn prepare_query(_storage: &mut Self::Storage) -> Self::PreparedQuery {}

    fn run_query_once(storage: &mut Self::Storage, _query: &mut Self::PreparedQuery)
    {
        for row in storage.groups.iter_mut().flatten()
        {
            row.position.x += row.velocity.x;
            row.position.y += row.velocity.y;
        }
    }
}

pub struct Query3;

impl QueryWorkload for Query3
{
    type PreparedQuery = ();
    type Storage = ChunkedStorage;

    const COMPONENT_COUNT: u8 = 3;
    const NAME: &'static str = "std_vec";

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> Self::Storage
    {
        build_storage(entity_count, layout)
    }

    fn prepare_query(_storage: &mut Self::Storage) -> Self::PreparedQuery {}

    fn run_query_once(storage: &mut Self::Storage, _query: &mut Self::PreparedQuery)
    {
        for row in storage.groups.iter_mut().flatten()
        {
            row.position.x += row.velocity.x;
            row.position.y += row.velocity.y;
            row.health.value -= 0.1;
        }
    }
}
