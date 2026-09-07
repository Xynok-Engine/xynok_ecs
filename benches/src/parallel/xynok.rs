//! `xynok_ecs` under test: one `add_system_parallel` group per frame.
//!
//! The group is checked at the `add_system_parallel` call site, so if two of these systems ever
//! started writing the same component the setup would panic here rather than quietly serialise
//! itself. That is worth knowing while reading the numbers: what is being timed is a group the
//! scheduler has already proven safe to fan out.

use xynok_ecs::query::Query;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_ecs::world::World;
use xynok_std::unsafe_ptr::HeapPtr;

use crate::parallel::{
    Drain, Health, Mana, ParallelWorkload, Poison, Position, Regen, Stamina, SystemGroup, Velocity, seed_drain, seed_health, seed_mana, seed_poison,
    seed_position, seed_regen, seed_stamina, seed_velocity,
};

/// Every entity carries all eight components, so every system in the group walks every entity.
pub fn build_world(entity_count: usize) -> World
{
    let mut world = World::default();
    for i in 0..entity_count
    {
        world.create((
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
    for (position, velocity) in query
    {
        position.x += velocity.x;
        position.y += velocity.y;
    }
}

fn tick_poison(query: Query<(&mut Health, &Poison)>)
{
    for (health, poison) in query
    {
        health.value -= poison.value;
    }
}

fn regen_mana(query: Query<(&mut Mana, &Regen)>)
{
    for (mana, regen) in query
    {
        mana.value = (mana.value + regen.value).min(50.0);
    }
}

fn drain_stamina(query: Query<(&mut Stamina, &Drain)>)
{
    for (stamina, drain) in query
    {
        stamina.value -= drain.value;
    }
}

/// The session the benchmarked frame runs. Which one it is does not matter, only that the whole
/// group sits in the same one: a session holds its steps in order, and one step is one group.
const SESSION: DefaultScheduleSession = DefaultScheduleSession::Update;

pub struct ParallelFrame;

impl ParallelWorkload for ParallelFrame
{
    type Runner = DefaultScheduler;
    type World = World;

    const DISPLAY_NAME: &'static str = "xynok_ecs";
    const NAME: &'static str = "xynok_ecs";

    fn setup(entity_count: usize, group: SystemGroup) -> Self::Runner
    {
        let world = HeapPtr::new(build_world(entity_count));
        let mut scheduler = DefaultScheduler::new(world);

        match group
        {
            SystemGroup::Two => scheduler.add_system_parallel(SESSION, (integrate, tick_poison)),
            SystemGroup::Four => scheduler.add_system_parallel(SESSION, (integrate, tick_poison, regen_mana, drain_stamina)),
        };

        // One untimed frame, so the query registration the first run does lands outside the
        // measurement. bevy's first `Schedule::run` builds its graph for the same reason, and both
        // sides get the same courtesy.
        scheduler.run(SESSION);
        scheduler
    }

    fn run_frame(runner: &mut Self::Runner)
    {
        runner.run(SESSION);
    }

    fn build_world_only(entity_count: usize) -> World
    {
        build_world(entity_count)
    }
}
