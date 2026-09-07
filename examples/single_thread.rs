#![allow(unused)]
//! `DefaultScheduler` runs every system of a session sequentially on the calling thread, in
//! the order they were added. No system ever overlaps another, so a `&mut` query in one
//! system can be read back by the next one in the same session.

use xynok_ecs::component;
use xynok_ecs::query::Query;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_ecs::world::World;
use xynok_std::unsafe_ptr::HeapPtr;

#[component]
#[derive(Debug, Default)]
struct Name(&'static str);
#[component]
#[derive(Debug, Default)]
struct Hp(i32);
#[component]
#[derive(Debug, Default)]
struct Mana(i32);
#[component]
#[derive(Debug, Default)]
struct Poison(i32);

/// A system takes no parameter at all: it never touches component storage, so it declares an
/// empty access scope
fn announce_start()
{
    println!("---------------- Start: world is ready");
}

/// One `&mut` parameter: the only writer of `Mana` in this session
fn regen_mana(query: Query<&mut Mana>)
{
    for mana in query
    {
        mana.0 = (mana.0 + 5).min(50);
    }
}

/// Two parameters in one system. They are checked against each other at `add_system` time:
/// `&mut Hp` plus `&Poison` is fine because no component is both written and read here.
/// A second `Query<&Hp>` beside `&mut Hp` would panic at the `add_system` call site.
fn tick_poison(damaged: Query<(&mut Hp, &Poison)>, healthy: Query<&Name>)
{
    for (hp, poison) in damaged
    {
        hp.0 -= poison.0;
    }
    // `healthy` matches every entity carrying `Name`, poisoned or not
    let count = healthy.into_iter().count();
    println!("  {count} named entities alive");
}

/// Runs after the two systems above, so it observes their writes within the same frame
fn report(query: Query<(&Name, &Hp, &Mana)>)
{
    for (name, hp, mana) in query
    {
        println!("  {} -> hp {}, mana {}", name.0, hp.0, mana.0);
    }
}

fn announce_quit()
{
    println!("---------------- AppQuit: shutting down");
}

fn main()
{
    // The scheduler takes ownership of the world, so spawn everything before handing it over
    let mut world = HeapPtr::new(World::default());

    world.create((Name("hero"), Hp(100), Mana(10)));
    world.create((Name("mage"), Hp(70), Mana(40)));
    // Same components plus `Poison`: a different archetype, still matched by `Query<&Hp>`
    world.create((Name("goblin"), Hp(30), Mana(0), Poison(3)));

    let mut scheduler = DefaultScheduler::new(world);

    scheduler
        .add_system(DefaultScheduleSession::Start, announce_start)
        // Order inside a session is the order of these calls
        .add_system(DefaultScheduleSession::Update, regen_mana)
        .add_system(DefaultScheduleSession::Update, tick_poison)
        .add_system(DefaultScheduleSession::LateUpdate, report)
        .add_system(DefaultScheduleSession::AppQuit, announce_quit);

    scheduler.run(DefaultScheduleSession::Start);

    // A session with no system registered is simply skipped, hence `PreUpdate` costs nothing
    for frame in 1..=3
    {
        println!("---------------- frame {frame}");
        scheduler.run(DefaultScheduleSession::PreUpdate);
        scheduler.run(DefaultScheduleSession::Update);
        scheduler.run(DefaultScheduleSession::LateUpdate);
    }

    scheduler.run(DefaultScheduleSession::AppQuit);
}
