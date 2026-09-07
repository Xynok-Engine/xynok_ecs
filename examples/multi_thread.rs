#![allow(unused)]
//! `add_system_parallel` hands a whole group of systems to the scheduler's thread pool: every
//! system of the group starts at the same time, and the group is done when the slowest one is.
//!
//! The scheduler only accepts a group whose systems provably never touch the same data in an
//! incompatible way. That check happens at the `add_system_parallel` call site, so a data race
//! turns into a panic while you are wiring the schedule up, not into a wrong number three hours
//! into a play session.

use std::thread;
use std::time::{Duration, Instant};

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
struct Position(f32);
#[component]
#[derive(Debug, Default)]
struct Velocity(f32);
#[component]
#[derive(Debug, Default)]
struct Hp(i32);
#[component]
#[derive(Debug, Default)]
struct Poison(i32);
#[component]
#[derive(Debug, Default)]
struct Mana(i32);
#[component]
#[derive(Debug, Default)]
struct Regen(i32);

/// Stands in for a system heavy enough that running it beside its neighbours is worth anything.
/// Without it every system here would finish long before the pool even hands the next one out,
/// and the timings printed below would say nothing.
const WORK: Duration = Duration::from_millis(60);

fn busy()
{
    thread::sleep(WORK);
}

/// The pool runs jobs on its worker threads and on the thread that opened the scope, so one line
/// of the group usually carries the caller's thread id
fn tag(system: &str) -> String
{
    format!("  [{:?}] {system}", thread::current().id())
}

// ---------------------------------------------------------------------------------------------
// A read-only group. Both systems read `Position`, which is fine: shared reads never conflict,
// no matter how many of them run at once.
// ---------------------------------------------------------------------------------------------

fn survey_speed(query: Query<(&Position, &Velocity)>)
{
    busy();
    let fastest = query.into_iter().map(|(_, v)| v.0).fold(0.0f32, f32::max);
    println!("{}", tag(&format!("survey_speed -> fastest {fastest}")));
}

fn survey_spread(query: Query<&Position>)
{
    busy();
    let sum: f32 = query.into_iter().map(|p| p.0).sum();
    println!("{}", tag(&format!("survey_spread -> position sum {sum}")));
}

// ---------------------------------------------------------------------------------------------
// A writing group. Each system writes a component no other system in the group touches, so the
// three of them can never meet on the same bytes.
// ---------------------------------------------------------------------------------------------

fn integrate(query: Query<(&mut Position, &Velocity)>)
{
    busy();
    for (position, velocity) in query
    {
        position.0 += velocity.0;
    }
    println!("{}", tag("integrate -> wrote Position"));
}

fn tick_poison(query: Query<(&mut Hp, &Poison)>)
{
    busy();
    for (hp, poison) in query
    {
        hp.0 -= poison.0;
    }
    println!("{}", tag("tick_poison -> wrote Hp"));
}

fn regen_mana(query: Query<(&mut Mana, &Regen)>)
{
    busy();
    for (mana, regen) in query
    {
        mana.0 = (mana.0 + regen.0).min(50);
    }
    println!("{}", tag("regen_mana -> wrote Mana"));
}

/// A second writer of `Mana`. Putting it in the same group as `regen_mana` is what the checker
/// exists for, see the commented call in `main`.
fn drain_mana(query: Query<&mut Mana>)
{
    for mana in query
    {
        mana.0 -= 1;
    }
}

/// Single system, so it starts only after the whole `Update` group has finished. That is the
/// point of a group boundary: inside a group there is no ordering at all, between two steps
/// there is.
fn report(query: Query<(&Name, &Position, &Hp, &Mana)>)
{
    for (name, position, hp, mana) in query
    {
        println!("  {} -> pos {:.1}, hp {}, mana {}", name.0, position.0, hp.0, mana.0);
    }
}

fn main()
{
    let mut world = HeapPtr::new(World::default());

    world.create((Name("hero"), Position(0.0), Velocity(1.5), Hp(100), Mana(10), Regen(5), Poison(0)));
    world.create((Name("mage"), Position(4.0), Velocity(0.5), Hp(70), Mana(40), Regen(8), Poison(0)));
    world.create((Name("goblin"), Position(9.0), Velocity(2.0), Hp(30), Mana(0), Regen(1), Poison(3)));

    let mut scheduler = DefaultScheduler::new(world);

    scheduler
        .add_system_parallel(DefaultScheduleSession::PreUpdate, (survey_speed, survey_spread))
        .add_system_parallel(DefaultScheduleSession::Update, (integrate, tick_poison, regen_mana))
        .add_system(DefaultScheduleSession::LateUpdate, report);

    // Rejected right here, at the call site, with `ParallelGroupConflict`: `regen_mana` and
    // `drain_mana` both write `Mana`, and the same pair of entities feeds both queries. Uncomment
    // to see the panic.
    // scheduler.add_system_parallel(DefaultScheduleSession::Update, (regen_mana, drain_mana));

    // A group of one is not worth a thread pool round trip, so the scheduler runs it inline on
    // this thread. Watch the thread id below: it stays the caller's.
    scheduler.add_system_parallel(DefaultScheduleSession::LateUpdate, (survey_spread,));

    for frame in 1..=2
    {
        println!("---------------- frame {frame}");

        let started = Instant::now();
        scheduler.run(DefaultScheduleSession::PreUpdate);
        println!("  PreUpdate: 2 systems x {WORK:?} took {:?}", started.elapsed());

        let started = Instant::now();
        scheduler.run(DefaultScheduleSession::Update);
        println!("  Update: 3 systems x {WORK:?} took {:?}", started.elapsed());

        scheduler.run(DefaultScheduleSession::LateUpdate);
    }
}
