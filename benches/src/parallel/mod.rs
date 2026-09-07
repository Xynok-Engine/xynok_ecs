//! The workload for the multi-threaded benchmark: one frame of a schedule that runs several
//! systems side by side.
//!
//! The single-threaded benchmark in `benches/query.rs` asks how fast one query walks its
//! entities. This one asks a different question: given a group of systems that provably never
//! touch the same component in an incompatible way, how much does a library's scheduler get out
//! of running them at once, and how much does it charge for the attempt.
//!
//! Both schedulers are handed exactly the same group. Every system reads one component and writes
//! another, and no two of them name the same component, which is what lets `xynok_ecs`
//! `add_system_parallel` accept the group and what lets bevy's multi-threaded executor spread it
//! over its task pool without any explicit ordering.
//!
//! Every entity carries all eight components, so each system in the group walks the same number of
//! rows no matter how many systems the group has. Growing the group grows the work, and the shape
//! of that growth against the number of threads is the thing being measured.

pub mod bevy;
pub mod xynok;

use bevy_ecs::prelude::Component;
use serde::Serialize;
use xynok_ecs::component as xynok_component;

pub use crate::workload::{Health, Position, Velocity, count_label, seed_health, seed_position, seed_velocity};

/// Poison damage applied to `Health` by `tick_poison`.
#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Poison
{
    pub value: f32,
}

/// Written by `regen_mana`, read by nobody else.
#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Mana
{
    pub value: f32,
}

#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Regen
{
    pub value: f32,
}

/// Written by `drain_stamina`, read by nobody else.
#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Stamina
{
    pub value: f32,
}

#[xynok_component]
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Drain
{
    pub value: f32,
}

#[inline]
pub fn seed_poison(i: usize) -> Poison
{
    Poison { value: (i % 7) as f32 * 0.25 }
}

#[inline]
pub fn seed_mana(i: usize) -> Mana
{
    Mana { value: (i % 40) as f32 }
}

#[inline]
pub fn seed_regen(_i: usize) -> Regen
{
    Regen { value: 0.5 }
}

#[inline]
pub fn seed_stamina(i: usize) -> Stamina
{
    Stamina { value: 50.0 + (i % 20) as f32 }
}

#[inline]
pub fn seed_drain(_i: usize) -> Drain
{
    Drain { value: 0.2 }
}

/// How many worker threads each scheduler gets.
///
/// `xynok_ecs`'s `DefaultScheduler` builds its pool with four workers and no way to ask for
/// another number, so bevy's task pool is pinned to the same four. Both pools also run work on the
/// thread that started the group, so the two sides are matched: same worker count, same caller
/// participation. If `DefaultScheduler` ever stops hard-coding four, this has to follow.
pub const WORKER_THREADS: usize = 4;

/// Group sizes under test.
///
/// Two is the smallest group where parallelism can pay for itself at all. Four matches the worker
/// count, so it is the largest group that still gets a thread each. The pair says whether the
/// scheduler's overhead is per group or per system.
pub const SYSTEM_COUNTS: [usize; 2] = [2, 4];

/// Entity counts, the same ones the single-threaded benchmark uses so the two can be read together.
pub const PARALLEL_ENTITY_COUNTS: [usize; 3] = [1_000, 10_000, 100_000];

/// Which systems make up the group, in the order they are added.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SystemGroup
{
    /// `integrate` and `tick_poison`.
    Two,
    /// The above plus `regen_mana` and `drain_stamina`.
    Four,
}

impl SystemGroup
{
    pub const ALL: [SystemGroup; 2] = [SystemGroup::Two, SystemGroup::Four];

    pub fn from_count(count: usize) -> Self
    {
        match count
        {
            2 => SystemGroup::Two,
            4 => SystemGroup::Four,
            other => panic!("no system group of size {other}, see SYSTEM_COUNTS"),
        }
    }

    pub fn count(self) -> usize
    {
        match self
        {
            SystemGroup::Two => 2,
            SystemGroup::Four => 4,
        }
    }

    /// Short form, safe inside a criterion benchmark id.
    pub fn slug(self) -> String
    {
        format!("{}_systems", self.count())
    }

    pub fn label(self) -> String
    {
        format!("{} systems", self.count())
    }
}

/// One competitor in the multi-threaded benchmark.
///
/// `setup` builds the world and wires up the schedule, and is never timed. `run_frame` runs the
/// group once, and is the only part that is.
pub trait ParallelWorkload
{
    /// Whatever the library needs to run a frame: a scheduler, or a world plus a schedule.
    type Runner;
    /// The world on its own, with nothing scheduled against it.
    type World;

    /// Library name, as it appears in benchmark ids. No spaces, no `/`.
    const NAME: &'static str;
    /// The same name written for a human reading a table.
    const DISPLAY_NAME: &'static str;

    fn setup(entity_count: usize, group: SystemGroup) -> Self::Runner;
    fn run_frame(runner: &mut Self::Runner);

    /// The world `setup` would build, handed back on its own.
    ///
    /// The report weighs this rather than a whole runner: a runner also holds a scheduler, and on
    /// one side that scheduler owns a thread pool, whose stacks and queues have nothing to do with
    /// what storing those entities costs.
    fn build_world_only(entity_count: usize) -> Self::World;
}
