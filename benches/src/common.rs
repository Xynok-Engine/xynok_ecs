//! Shared workload definition for every competitor in this benchmark: each one stores the same
//! `Position`/`Velocity` payload and runs the same "position += velocity" pass, just through a
//! different storage/query mechanism (`xynok_ecs`, `bevy_ecs`, or a plain `Vec`).
use bevy_ecs::prelude::Component;
// aliased: bevy's `Component` derive also defines a derive-helper attribute literally named
// `component` (e.g. `#[component(storage = ...)]`), which is otherwise ambiguous with this one.
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

pub const ENTITY_COUNTS: [usize; 3] = [1_000, 10_000, 100_000];
pub const WARMUP_ITERS: usize = 10;
pub const MEASURED_ITERS: usize = 100;

/// One competitor under test. `setup` and `prepare_query` are allowed to allocate (and are
/// measured separately); `run_query_once` is the part timed and checked for allocation-freedom.
pub trait EcsBenchmark
{
    type Storage;
    type PreparedQuery;

    const NAME: &'static str;

    fn setup(entity_count: usize) -> Self::Storage;
    fn prepare_query(storage: &mut Self::Storage) -> Self::PreparedQuery;
    fn run_query_once(storage: &mut Self::Storage, query: &mut Self::PreparedQuery);
}
