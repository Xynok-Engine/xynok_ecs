#![allow(unused)]
//! Demonstrates the query state filters in `xynok_ecs::query::filter`: `Added<Q>`, `Changed<Q>`,
//! `Enabled<Q>`, `Disabled<Q>`. Each one only yields rows whose state changed since *that
//! system's own* previous run - not since some global instant - so a system running less often
//! than another still sees every event exactly once.

use xynok_ecs::component;
use xynok_ecs::query::filter::{Added, Changed, Disabled, Enabled};
use xynok_ecs::query::Query;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_ecs::world::World;
use xynok_ecs::wrapper::Disable;
use xynok_std::unsafe_ptr::HeapPtr;

#[component(EnableAble, ChangeAble)]
#[derive(Debug, Default)]
struct Hp(i32);

/// Only rows whose `Hp` was inserted (spawned, or `merge_component`d in) since this system's
/// own previous run. Entities that already existed the last time this system ran are skipped,
/// even on this same run - `Added` never fires twice for the same insert.
fn report_added(query: Query<Added<&Hp>>)
{
    for hp in query
    {
        println!("  added:    {hp:?}");
    }
}

/// Only rows whose `Hp` value was written since this system's own previous run: an insert
/// (`world.create`), an overwrite (`world.merge_component`), or a write through a
/// `Query<&mut Hp>`. Merely *reading* a row out of a `&mut` query does not count, see
/// `poison_everyone` below.
fn report_changed(query: Query<Changed<&Hp>>)
{
    for hp in query
    {
        println!("  changed:  {hp:?}");
    }
}

/// Only rows whose `Hp` enable bit is currently `true` (the default unless spawned via
/// `wrapper::Disable`)
fn report_enabled(query: Query<Enabled<&Hp>>)
{
    for hp in query
    {
        println!("  enabled:  {hp:?}");
    }
}

/// Only rows whose `Hp` enable bit is currently `false`
fn report_disabled(query: Query<Disabled<&Hp>>)
{
    for hp in query
    {
        println!("  disabled: {hp:?}");
    }
}

/// Writes to half the rows it walks. Only those halves show up in `report_changed` next run:
/// `Query<&mut Hp>` hands out a `Mut<Hp>`, which stamps the changed tick on `DerefMut` and not
/// on the way out of the iterator.
fn poison_everyone_below_50(query: Query<&mut Hp>)
{
    for mut hp in query
    {
        if hp.0 < 50
        {
            hp.0 += 1;
        }
    }
}

fn main()
{
    let mut world = HeapPtr::new(World::default());

    let a = world.create(Hp(10)); // enabled by default (`TComponent::ENABLE_VALUE = true`)
    let _b = world.create(Disable::new(Hp(20))); // spawned already disabled

    // A detached `&mut World`, kept around so `merge_component` can still be called below, after
    // `world` itself is moved into the scheduler.
    let mut world_mut = world.as_ref_mut();

    let mut scheduler = DefaultScheduler::new(world);
    scheduler
        .add_system(DefaultScheduleSession::Update, report_added)
        .add_system(DefaultScheduleSession::Update, report_changed)
        .add_system(DefaultScheduleSession::Update, report_enabled)
        .add_system(DefaultScheduleSession::Update, report_disabled)
        .add_system(DefaultScheduleSession::Update, poison_everyone_below_50);

    println!("---------------- run 1: both entities were just spawned, so Added and Changed fire for both");
    scheduler.run(DefaultScheduleSession::Update);

    println!("\n---------------- run 2: only what `poison_everyone_below_50` wrote in run 1 is Changed; Added reports nothing");
    scheduler.run(DefaultScheduleSession::Update);

    println!("\n---------------- merge_component overwrites `a`'s Hp in place: a real 'changed' write");
    world_mut.merge_component(a, Hp(99));

    println!("\n---------------- run 3: Changed fires for `a` (merge_component) and for whatever run 2 poisoned; Added still reports nothing");
    scheduler.run(DefaultScheduleSession::Update);
}
