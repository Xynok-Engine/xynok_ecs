//! Shared workload definition for every competitor in this benchmark: each one stores the same
//! `Position`/`Velocity`/`Health` payload and runs the same "position += velocity; health -= decay"
//! pass over 1, 2, or 3 of those components, just through a different storage/query mechanism
//! (`xynok_ecs`, `bevy_ecs`, or a plain `Vec`).
use bevy_ecs::prelude::Component;
// aliased: bevy's `Component` derive also defines a derive-helper attribute literally named
// `component` (e.g. `#[component(storage = ...)]`), which is otherwise ambiguous with this one.
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

// Zero-sized tag components used only to fragment entities into distinct archetypes: every
// entity still carries `Position`/`Velocity`/`Health`, but which (if any) tag it also carries
// determines which of the 5 archetypes it lives in. A query over any subset of
// `Position`/`Velocity`/`Health` matches all 5, so it must fan out across every one of them.
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

pub fn seed_position(i: usize) -> Position
{
    Position {
        x: i as f32,
        y: i as f32 * 0.5,
    }
}
pub fn seed_velocity(_i: usize) -> Velocity
{
    Velocity { x: 1.0, y: -1.0 }
}
pub fn seed_health(i: usize) -> Health
{
    Health {
        value: 100.0 + (i % 50) as f32,
    }
}

/// Splits `total` into `groups` near-equal-sized buckets (the first `total % groups` buckets get
/// one extra element). Every `ENTITY_COUNTS` value divides evenly by `FRAGMENTED_ARCHETYPES`, so
/// in practice this always returns `groups` equal buckets, but it stays correct if that changes.
pub fn split_counts(total: usize, groups: usize) -> Vec<usize>
{
    let base = total / groups;
    let remainder = total % groups;
    (0..groups).map(|i| base + usize::from(i < remainder)).collect()
}

/// How entities carrying the benchmark's components are laid out across archetypes.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchetypeLayout
{
    /// Every entity shares one archetype: `(Position, Velocity, Health)`.
    Single,
    /// Entities are split across 5 archetypes that all carry `(Position, Velocity, Health)` but
    /// differ by an extra tag component (or no tag, for the base group). A query over the shared
    /// components must scan all 5.
    Fragmented5,
}

impl ArchetypeLayout
{
    pub const ALL: [ArchetypeLayout; 2] = [ArchetypeLayout::Single, ArchetypeLayout::Fragmented5];

    pub fn label(self) -> &'static str
    {
        match self
        {
            ArchetypeLayout::Single => "1 archetype",
            ArchetypeLayout::Fragmented5 => "5 archetypes",
        }
    }
}

pub const FRAGMENTED_ARCHETYPES: usize = 5;
pub const ENTITY_COUNTS: [usize; 3] = [1_000, 10_000, 100_000];

/// Every timed sample covers at least this many nanoseconds of `run_query_once` calls, batched
/// together and divided back down, so `Instant::now()`'s own overhead (tens of ns) can't dominate
/// the measurement of very fast queries (some samples are sub-microsecond).
pub const MIN_SAMPLE_NANOS: u128 = 20_000;
pub const WARMUP_ITERS: usize = 20;
pub const MEASURED_SAMPLES: usize = 200;

/// One competitor under test. `setup` and `prepare_query` are allowed to allocate (and are
/// measured separately); `run_query_once` is the part timed and checked for allocation-freedom.
pub trait EcsBenchmark
{
    type Storage;
    type PreparedQuery;

    const NAME: &'static str;
    /// Number of components read/written by `run_query_once`'s query (1, 2, or 3).
    const COMPONENT_COUNT: u8;

    fn setup(entity_count: usize, layout: ArchetypeLayout) -> Self::Storage;
    fn prepare_query(storage: &mut Self::Storage) -> Self::PreparedQuery;
    fn run_query_once(storage: &mut Self::Storage, query: &mut Self::PreparedQuery);
}
