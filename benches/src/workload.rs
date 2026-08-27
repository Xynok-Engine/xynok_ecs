//! The one workload every competitor in this crate runs.
//!
//! Each library stores the same `Position`/`Velocity`/`Health` payload and performs the same
//! "position += velocity; health -= decay" pass over 1, 2 or 3 of those components. The only thing
//! that differs is the storage and the query mechanism underneath, which is the whole point: if the
//! payload or the arithmetic differed, the numbers would be comparing two different programs.

use bevy_ecs::prelude::Component;
// Aliased because bevy's `Component` derive also defines a derive-helper attribute literally named
// `component` (as in `#[component(storage = ...)]`), which would otherwise be ambiguous with this
// crate's own attribute of the same name.
use serde::Serialize;
use xynok_ecs::component as xynok_component;

#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Position
{
    pub x: f32,
    pub y: f32,
}

#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Velocity
{
    pub x: f32,
    pub y: f32,
}

#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Health
{
    pub value: f32,
}

// Zero-sized tags used only to fragment entities into distinct archetypes. Every entity still
// carries `Position`/`Velocity`/`Health`; which tag it also carries (or none, for the base group)
// decides which of the 5 archetypes it lives in. A query over any subset of the three matches all
// 5, so it has to fan out across every one of them.
#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerA;
#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerB;
#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerC;
#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerD;

#[inline]
pub fn seed_position(i: usize) -> Position
{
    Position {
        x: i as f32,
        y: i as f32 * 0.5,
    }
}

#[inline]
pub fn seed_velocity(_i: usize) -> Velocity
{
    Velocity { x: 1.0, y: -1.0 }
}

#[inline]
pub fn seed_health(i: usize) -> Health
{
    Health {
        value: 100.0 + (i % 50) as f32,
    }
}

/// Splits `total` into `groups` near-equal buckets, the first `total % groups` of them getting one
/// extra element. Every count in [`ENTITY_COUNTS`] and [`PARALLEL_ENTITY_COUNTS`] divides evenly by
/// [`FRAGMENTED_ARCHETYPES`], so in practice the buckets always come out equal, but this stays
/// correct if that ever changes.
pub fn split_counts(total: usize, groups: usize) -> Vec<usize>
{
    let base = total / groups;
    let remainder = total % groups;
    (0..groups).map(|i| base + usize::from(i < remainder)).collect()
}

/// How the entities carrying the workload's components are spread across archetypes.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchetypeLayout
{
    /// Every entity shares one archetype: `(Position, Velocity, Health)`.
    Single,
    /// Entities are split across 5 archetypes that all carry `(Position, Velocity, Health)` but
    /// differ by an extra tag component. A query over the shared components has to scan all 5.
    Fragmented5,
}

impl ArchetypeLayout
{
    pub const ALL: [ArchetypeLayout; 2] = [ArchetypeLayout::Single, ArchetypeLayout::Fragmented5];

    /// Short form, safe to use inside a criterion benchmark id (no spaces, no path separators).
    pub fn slug(self) -> &'static str
    {
        match self
        {
            ArchetypeLayout::Single => "1_archetype",
            ArchetypeLayout::Fragmented5 => "5_archetypes",
        }
    }

    /// How many groups [`split_counts`] should be asked for under this layout.
    pub fn group_count(self) -> usize
    {
        match self
        {
            ArchetypeLayout::Single => 1,
            ArchetypeLayout::Fragmented5 => FRAGMENTED_ARCHETYPES,
        }
    }
}

pub const FRAGMENTED_ARCHETYPES: usize = 5;

/// Entity counts for the single-threaded query benchmark. 1k fits in L2 on most machines, 100k
/// does not, so the pair says something about cache behaviour and not only about instruction
/// count.
pub const ENTITY_COUNTS: [usize; 3] = [1_000, 10_000, 100_000];

/// Entity counts for the parallel benchmark. Deliberately larger: with 16 KiB chunks and a ~32 byte
/// row, 100k entities is only a couple of hundred chunks, and below that the fork-join bookkeeping
/// is most of what gets measured.
pub const PARALLEL_ENTITY_COUNTS: [usize; 2] = [100_000, 1_000_000];

/// Renders an entity count the way it appears in a benchmark id: `1k`, `100k`, `1M`.
pub fn count_label(count: usize) -> String
{
    if count >= 1_000_000 && count.is_multiple_of(1_000_000)
    {
        format!("{}M", count / 1_000_000)
    }
    else if count >= 1_000 && count.is_multiple_of(1_000)
    {
        format!("{}k", count / 1_000)
    }
    else
    {
        count.to_string()
    }
}

/// One competitor in the single-threaded query benchmark.
///
/// `setup` and `prepare_query` are allowed to allocate and are never timed. `run_query_once` is the
/// only part that is: it must not allocate, and `memory_report` checks exactly that.
pub trait QueryWorkload
{
    type Storage;
    type PreparedQuery;

    /// Library name, as it appears in benchmark ids. Keep it free of spaces and `/`.
    const NAME: &'static str;
    /// How many components `run_query_once` reads or writes (1, 2 or 3).
    const COMPONENT_COUNT: u8;

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> Self::Storage;
    fn prepare_query(storage: &mut Self::Storage) -> Self::PreparedQuery;
    fn run_query_once(storage: &mut Self::Storage, query: &mut Self::PreparedQuery);
}

/// One competitor in the parallel benchmark.
///
/// Only `xynok_ecs` and `bevy_ecs` implement this: a `Vec` has no parallel query to compare against,
/// and hand-rolling one would be benchmarking whatever splitting scheme this file happened to pick.
///
/// The workload is fixed at 2 components (`position += velocity`). The question a parallel
/// benchmark answers is how a pass gets spread across threads, and arity does not change that;
/// `benches/query.rs` already covers what arity costs.
pub trait ParallelWorkload
{
    type Storage;

    const NAME: &'static str;

    /// Builds the storage and the pool it will run on. `threads` is the number of worker threads to
    /// spawn, not counting the calling thread, which participates in both libraries.
    fn setup(entity_count: usize, layout: ArchetypeLayout, threads: usize) -> Self::Storage;

    /// One pass over every matching entity, spread across the pool.
    fn run_parallel(storage: &mut Self::Storage);

    /// The same pass on the calling thread alone. This is the baseline the parallel number is a
    /// speedup over, measured through the same query machinery so the comparison isolates the
    /// spreading and nothing else.
    fn run_sequential(storage: &mut Self::Storage);
}
