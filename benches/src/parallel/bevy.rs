//! `bevy_ecs` under test: the same group of systems, run by bevy's multi-threaded executor.
//!
//! Two things have to be set up before the comparison means anything. The executor is chosen
//! explicitly rather than left to `default_executor`, so the benchmark cannot silently fall back
//! to the single-threaded one if a feature flag goes missing. And the task pool is built with the
//! same worker count `xynok_ecs`'s scheduler hard-codes, because a pool sized to the machine would
//! be measuring the machine rather than the two schedulers.
//!
//! No ordering is declared between the systems. That is the point: bevy's executor looks at what
//! each system accesses, finds no conflict, and is free to run all of them at once.

use bevy_ecs::prelude::{Query, Schedule, World};
use bevy_ecs::schedule::MultiThreadedExecutor;
use bevy_tasks::{ComputeTaskPool, TaskPoolBuilder};

use crate::parallel::{
    Drain, Health, Mana, ParallelWorkload, Poison, Position, Regen, Stamina, SystemGroup, Velocity, WORKER_THREADS, seed_drain, seed_health, seed_mana,
    seed_poison, seed_position, seed_regen, seed_stamina, seed_velocity,
};

/// Builds the pool bevy's executor will look for, sized to match `xynok_ecs`'s.
///
/// `ComputeTaskPool` is a process-wide singleton, so this only takes effect the first time it runs
/// and every later call is a no-op. Calling it from every `setup` is deliberate: it means no
/// benchmark can accidentally be the one that runs before the pool exists, in which case bevy
/// would build a default pool sized to the machine and quietly change what is being compared.
pub fn init_task_pool()
{
    ComputeTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(WORKER_THREADS).build());
}

/// The same storage shape as `parallel::xynok::build_world`, built through bevy's own API.
pub fn build_world(entity_count: usize) -> World
{
    let mut world = World::new();
    for i in 0..entity_count
    {
        world.spawn((
            seed_position(i),
            seed_velocity(i),
            seed_health(i),
            seed_poison(i),
            seed_mana(i),
            seed_regen(i),
            seed_stamina(i),
            seed_drain(i),
        ));
    }
    world
}

fn integrate(query: Query<(&mut Position, &Velocity)>)
{
    for (mut position, velocity) in query
    {
        position.x += velocity.x;
        position.y += velocity.y;
    }
}

fn tick_poison(query: Query<(&mut Health, &Poison)>)
{
    for (mut health, poison) in query
    {
        health.value -= poison.value;
    }
}

fn regen_mana(query: Query<(&mut Mana, &Regen)>)
{
    for (mut mana, regen) in query
    {
        mana.value = (mana.value + regen.value).min(50.0);
    }
}

fn drain_stamina(query: Query<(&mut Stamina, &Drain)>)
{
    for (mut stamina, drain) in query
    {
        stamina.value -= drain.value;
    }
}

pub struct ParallelFrame;

impl ParallelWorkload for ParallelFrame
{
    type Runner = (World, Schedule);
    type World = World;

    const DISPLAY_NAME: &'static str = "bevy_ecs";
    const NAME: &'static str = "bevy_ecs";

    fn setup(entity_count: usize, group: SystemGroup) -> Self::Runner
    {
        init_task_pool();

        let mut world = build_world(entity_count);
        let mut schedule = Schedule::default();
        schedule.set_executor(MultiThreadedExecutor::new());

        match group
        {
            SystemGroup::Two => schedule.add_systems((integrate, tick_poison)),
            SystemGroup::Four => schedule.add_systems((integrate, tick_poison, regen_mana, drain_stamina)),
        };

        // Untimed, for the same reason as on the `xynok_ecs` side: the first run is where the
        // schedule graph gets built and the executor initialised.
        schedule.run(&mut world);
        (world, schedule)
    }

    fn run_frame(runner: &mut Self::Runner)
    {
        let (world, schedule) = runner;
        schedule.run(world);
    }

    fn build_world_only(entity_count: usize) -> World
    {
        build_world(entity_count)
    }
}
