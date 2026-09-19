//! `Singleton<&T>` / `Singleton<&mut T>`: a system parameter for an archetype that holds exactly
//! one entity, like the frame clock or a global config. No iteration, no `Option`, just the value.
//!
//! The archetype has to be a singleton one (`World::register_singleton` or
//! `World::create_singleton`). A regular archetype, an archetype the world has never seen, or a
//! singleton whose entity was never spawned are all errors, which the scheduler reports instead
//! of silently handing out nothing.

use xynok_ecs::component;
use xynok_ecs::query::Query;
use xynok_ecs::query::singleton::Singleton;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_ecs::world::World;
use xynok_std::unsafe_ptr::HeapPtr;

/// The frame clock, one per world
#[component(ChangeAble)]
#[derive(Debug, Default)]
struct Time
{
    delta:  f32,
    scale:  f32,
    passed: f32,
}

#[component]
#[derive(Debug, Default)]
struct Position(f32);

#[component]
#[derive(Debug, Default)]
struct Velocity(f32);

/// `&mut` on the singleton: it reads like a plain `&mut Time` thanks to `DerefMut`. `Time` is
/// `ChangeAble`, so the write below stamps the row's changed tick, exactly like a `Query<&mut T>`
/// would.
fn advance_time(mut time: Singleton<&mut Time>)
{
    time.passed += time.delta * time.scale;
}

/// `&` on the same singleton, next to a regular query. The scheduler sees the read of `Time` and
/// the write in `advance_time` as a conflict, so the two never run side by side.
fn integrate(time: Singleton<&Time>, positions: Query<(&mut Position, &Velocity)>)
{
    let step = time.delta * time.scale;
    for (pos, vel) in positions
    {
        pos.0 += vel.0 * step;
    }
}

fn report(time: Singleton<&Time>, positions: Query<&Position>)
{
    print!("passed {:.2}s ->", time.passed);
    for pos in positions
    {
        print!(" {:.2}", pos.0);
    }
    println!();
}

fn main()
{
    let mut world = HeapPtr::new(World::default());

    // One entity, its own archetype, and the world refuses a second one
    world.create_singleton(Time {
        delta:  1.0 / 60.0,
        scale:  1.0,
        passed: 0.0,
    });

    world.create((Position(0.0), Velocity(2.0)));
    world.create((Position(5.0), Velocity(-1.0)));

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler
        .add_system(DefaultScheduleSession::Update, advance_time)
        .add_system(DefaultScheduleSession::Update, integrate)
        .add_system(DefaultScheduleSession::LateUpdate, report);

    for _ in 0..3
    {
        scheduler.run(DefaultScheduleSession::Update);
        scheduler.run(DefaultScheduleSession::LateUpdate);
    }
}
