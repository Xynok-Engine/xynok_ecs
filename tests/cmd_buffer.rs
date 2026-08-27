//! Integration tests for E3: structural change written into a command buffer and applied at the
//! synchronisation point.
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use common::*;
use xynok_concurrency::pool::Config as LaneConfig;
use xynok_ecs::cmd_buffer::Commands;
use xynok_ecs::query::Query;
use xynok_ecs::schedule::lane::Lane;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession as Session, DefaultScheduler, TScheduler};
use xynok_ecs::world::World;
use xynok_std::unsafe_ptr::HeapPtr;

fn scheduler_with(world: World) -> DefaultScheduler
{
    DefaultScheduler::with_lane_config(
        HeapPtr::new(world),
        LaneConfig {
            threads: 3,
            ..LaneConfig::default()
        },
    )
}

// ------------------------------------------------------------------------------------------------
// The bare buffer, without a scheduler
// ------------------------------------------------------------------------------------------------

#[test]
fn t_nothing_happens_until_apply()
{
    let mut w = World::default();
    w.command_buffers().with(0, |buffer| {
        buffer.create(Hp(1));
        buffer.create(Hp(2));
    });

    assert_eq!(w.create_query::<&Hp>().into_iter().count(), 0, "the world changed before the commands were applied");
    assert_eq!(w.command_buffers_mut().pending(), 2);

    w.apply_commands();

    let mut seen: Vec<u32> = w.create_query::<&Hp>().into_iter().map(|hp| hp.0).collect();
    seen.sort_unstable();
    assert_eq!(seen, vec![1, 2]);
    assert_eq!(w.command_buffers_mut().pending(), 0, "the buffer must be empty once applied");
}

#[test]
fn t_commands_run_in_the_order_they_were_written()
{
    let mut w = World::default();
    let e = w.create(Hp(1));

    w.command_buffers().with(0, |buffer| {
        buffer.add_component(e, Mana(10));
        buffer.merge_component(e, Mana(20));
        buffer.remove_component::<Mana>(e);
        buffer.add_component(e, Mana(30));
    });
    w.apply_commands();

    let manas: Vec<u32> = w.create_query::<&Mana>().into_iter().map(|mana| mana.0).collect();
    assert_eq!(manas, vec![30], "commands must run in the order they were written, not in any other");
}

#[test]
fn t_destroy_of_an_already_dead_entity_is_ignored()
{
    let mut w = World::default();
    let e = w.create(Hp(1));

    w.command_buffers().with(0, |buffer| {
        buffer.destroy(e);
        // Another job may perfectly well decide to destroy the same entity, and neither is wrong
        buffer.destroy(e);
    });
    w.apply_commands();

    assert!(!w.exists(e));
    assert_eq!(w.create_query::<&Hp>().into_iter().count(), 0);
}

#[test]
fn t_push_runs_an_arbitrary_change()
{
    let mut w = World::default();
    w.command_buffers().with(0, |buffer| {
        buffer.push(|world| {
            world.create(Hp(99));
        });
    });
    w.apply_commands();

    assert_eq!(w.create_query::<&Hp>().into_iter().map(|hp| hp.0).collect::<Vec<_>>(), vec![99]);
}

#[test]
fn t_dropped_component_of_a_removed_row_runs_its_destructor()
{
    let _serialise = DROP_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let e = w.create((Hp(1), Tracked(7)));

    w.command_buffers().with(0, |buffer| {
        buffer.remove_component::<Tracked>(e);
    });
    assert_eq!(drop_count(), 0, "nothing is dropped before the command is applied");

    w.apply_commands();
    assert_eq!(drop_count(), 1, "the removed value must be dropped once, on the applying thread");
}

// ------------------------------------------------------------------------------------------------
// Through the scheduler
// ------------------------------------------------------------------------------------------------

static SPAWNED: AtomicUsize = AtomicUsize::new(0);
static SPAWN_LOCK: Mutex<()> = Mutex::new(());

/// Every entity carrying `Hp` spawns a new `Mana` entity. The structural change goes through
/// `Commands` rather than calling `World::create` directly, because this system runs alongside
/// another one.
fn spawn_one_per_hp(query: Query<&Hp>, cmd: Commands)
{
    for hp in query
    {
        cmd.create(Mana(hp.0));
        SPAWNED.fetch_add(1, Ordering::SeqCst);
    }
}

fn double_mana(query: Query<&mut Mana>)
{
    for mana in query
    {
        mana.0 *= 2;
    }
}

#[test]
fn t_commands_from_a_system_land_at_the_end_of_the_step()
{
    let _serialise = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    SPAWNED.store(0, Ordering::SeqCst);

    let mut w = World::default();
    for i in 1..=50u32
    {
        w.create(Hp(i));
    }

    let mut scheduler = scheduler_with(w);
    // Two steps: the first spawns entities, the second reads them back. If the commands were not
    // applied at the end of step one, step two would see nothing
    scheduler
        .add_system(Session::Update, spawn_one_per_hp)
        .add_system(Session::Update, double_mana);
    scheduler.run(Session::Update);

    assert_eq!(SPAWNED.load(Ordering::SeqCst), 50);

    static TOTAL: AtomicUsize = AtomicUsize::new(0);
    fn sum_mana(query: Query<&Mana>)
    {
        TOTAL.store(query.into_iter().map(|mana| mana.0 as usize).sum(), Ordering::SeqCst);
    }
    scheduler.add_system(Session::LateUpdate, sum_mana);
    scheduler.run(Session::LateUpdate);

    assert_eq!(TOTAL.load(Ordering::SeqCst), (1..=50usize).map(|i| i * 2).sum::<usize>());
}

/// Several workers writing commands within one step: one slot each, and every slot has to be
/// drained
#[test]
fn t_commands_written_from_several_workers_are_all_applied()
{
    let mut w = World::default();
    for i in 0..400u32
    {
        w.create(Hp(i));
    }

    fn spawn_from_every_chunk(query: Query<&Hp>, cmd: Commands, lane: Lane)
    {
        query.par_for_each_chunk(lane.pool(), 1, |view| {
            for hp in view.columns
            {
                cmd.create(Mana(hp.0));
            }
        });
    }

    let mut scheduler = scheduler_with(w);
    scheduler.add_system(Session::Update, spawn_from_every_chunk);
    scheduler.run(Session::Update);

    static COUNT: AtomicUsize = AtomicUsize::new(0);
    fn count_mana(query: Query<&Mana>)
    {
        COUNT.store(query.into_iter().count(), Ordering::SeqCst);
    }
    scheduler.add_system(Session::LateUpdate, count_mana);
    scheduler.run(Session::LateUpdate);

    assert_eq!(COUNT.load(Ordering::SeqCst), 400, "a command buffer slot was left undrained");
}

/// `Commands` touches no component storage, so it does not stop two systems sharing a group
#[test]
fn t_commands_does_not_block_a_parallel_group()
{
    let _serialise = SPAWN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    SPAWNED.store(0, Ordering::SeqCst);

    fn touch_pos(query: Query<&mut Pos>)
    {
        for pos in query
        {
            pos.x += 1.0;
        }
    }

    let mut w = World::default();
    for i in 1..=30u32
    {
        w.create((Hp(i), Pos { x: 0.0, y: 0.0 }));
    }

    let mut scheduler = scheduler_with(w);
    scheduler.add_system_parallel(Session::Update, (spawn_one_per_hp, touch_pos));
    scheduler.run(Session::Update);

    assert_eq!(SPAWNED.load(Ordering::SeqCst), 30);
}
