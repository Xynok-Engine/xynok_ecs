#![allow(unused)]
//! Three things this example shows, in the order they matter:
//!
//! 1. `add_system_parallel` declares a group of systems that run at the same time. Xynok never
//!    infers that; you say it, and the scheduler checks your claim against every pair's access
//!    scope at the call site.
//! 2. `Query::par_for_each_chunk` splits one system's own work by chunk across the same pool.
//! 3. `Commands` is the only way to create, destroy or move components while systems run in
//!    parallel. The commands land at the synchronisation point that ends the step.

use xynok_concurrency::pool::Config as LaneConfig;
use xynok_ecs::cmd_buffer::Commands;
use xynok_ecs::component;
use xynok_ecs::query::Query;
use xynok_ecs::schedule::lane::Lane;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_ecs::world::World;
use xynok_std::unsafe_ptr::HeapPtr;

#[component]
#[derive(Debug, Default)]
struct Position
{
    x: f32,
    y: f32,
}
#[component]
#[derive(Debug, Default)]
struct Velocity
{
    x: f32,
    y: f32,
}
#[component]
#[derive(Debug, Default)]
struct Hp(i32);
#[component]
#[derive(Debug, Default)]
struct Corpse;

/// Writes `Position`, reads `Velocity`.
///
/// `Lane` hands the system the pool it is already running on, and `par_for_each_chunk` splits the
/// query's chunks across it. `batch` counts chunks: two chunks per job here, because these chunks
/// are cheap. Aim for roughly 20 microseconds of work per job and derive `batch` from that.
fn integrate(query: Query<(&mut Position, &Velocity)>, lane: Lane)
{
    query.par_for_each_chunk(lane.pool(), 2, |view| {
        let (positions, velocities) = view.columns;
        for (position, velocity) in positions.iter_mut().zip(velocities.iter())
        {
            position.x += velocity.x;
            position.y += velocity.y;
        }
    });
}

/// Writes `Hp`. It touches no component `integrate` touches, which is what lets the two share a
/// group.
fn decay(query: Query<&mut Hp>)
{
    for hp in query
    {
        hp.0 -= 10;
    }
}

/// Reads `Hp` and queues structural change.
///
/// `World::add_component` would need `&mut World`, which no parallel job has. `Commands` records
/// the change into this worker's own buffer instead, and the scheduler applies every buffer, in
/// slot order, once the step is over.
///
/// The chunk view is also the only place an entity id is available: `Entity` is not a component
/// column, so it cannot be a query parameter, but `view.entities[i]` belongs to row `i` of every
/// column slice.
///
/// `merge_component` rather than `add_component`, because this runs every frame and an entity that
/// is already a corpse would otherwise be handed a second `Corpse`.
fn mark_the_dead(query: Query<&Hp>, cmd: Commands)
{
    query.for_each_chunk(|view| {
        for (e, hp) in view.entities.iter().zip(view.columns.iter())
        {
            if hp.0 <= 0
            {
                cmd.merge_component(*e, Corpse);
            }
        }
    });
}

fn report(positions: Query<&Position>, corpses: Query<&Corpse>)
{
    let moved = positions.into_iter().count();
    let dead = corpses.into_iter().count();
    println!("  {moved} entities moved, {dead} marked as corpses");
}

fn main()
{
    let mut world = HeapPtr::new(World::default());

    for i in 0..2_000
    {
        world.create((Position { x: 0.0, y: 0.0 }, Velocity { x: 1.0, y: (i % 7) as f32 }, Hp(100 - i % 100)));
    }

    // Three workers plus the calling thread. `LaneConfig::default()` sizes it as `cores - 1`, which
    // is the right choice in a real engine: the calling thread joins in as the last participant.
    // `LaneConfig::inline()` spawns nothing at all, which is the quickest way to check whether a
    // bug is about threading.
    let mut scheduler = DefaultScheduler::with_lane_config(
        world,
        LaneConfig {
            threads: 3,
            ..LaneConfig::default()
        },
    );

    scheduler
        // One step, two systems, running at the same time. Swap `decay` for anything that also
        // writes `Position` and this call panics right here rather than at the first `run`.
        .add_system_parallel(DefaultScheduleSession::Update, (integrate, decay))
        // A step of its own: it reads the `Hp` that `decay` just wrote
        .add_system(DefaultScheduleSession::Update, mark_the_dead)
        .add_system(DefaultScheduleSession::LateUpdate, report);

    for frame in 1..=3
    {
        println!("---------------- frame {frame}");
        scheduler.run(DefaultScheduleSession::Update);
        scheduler.run(DefaultScheduleSession::LateUpdate);
    }
}
