//! Cmd buffer: `Cmd` picks the cheaper of two ways depending on where it is called from. Inside
//! a parallel group it records the write and only replays it at `World::sync_point()`. Running
//! single threaded there is nothing to wait for, so it writes to the world right away and the
//! later `sync_point()` has nothing left to do. These tests pin down both ways, and check that
//! the state they leave behind is the same one calling `World` directly would have produced.
//!
//! Systems in the scheduler are `Fn`, not `FnMut`, so they cannot capture locals. The tests
//! therefore hand data to their systems through statics. The test runner runs tests in parallel
//! inside one binary, so every test keeps its own statics and never shares them with another.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use xynok_ecs::cmd_buffer::cmd::Cmd;
use xynok_ecs::component;
use xynok_ecs::entity::Entity;
use xynok_ecs::query::Query;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_ecs::world::{World, testing};
use xynok_std::unsafe_ptr::HeapPtr;

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hp(u32);

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mana(u32);

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Armor(u32);

const UPDATE: DefaultScheduleSession = DefaultScheduleSession::Update;

/// Where a system and its test pass entities back and forth.
///
/// `lock` here ignores poisoning on purpose: one failing test should not drag the others down
/// with it.
struct Slot(Mutex<Vec<Entity>>);

impl Slot
{
    const fn new() -> Self
    {
        Self(Mutex::new(Vec::new()))
    }
    fn push(&self, e: Entity)
    {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).push(e);
    }
    fn set(&self, e: Entity)
    {
        let mut g = self.0.lock().unwrap_or_else(|p| p.into_inner());
        g.clear();
        g.push(e);
    }
    fn all(&self) -> Vec<Entity>
    {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    fn one(&self) -> Entity
    {
        self.all()[0]
    }
}

/// A fresh world, on the heap because the scheduler holds a pointer into it.
fn new_world() -> HeapPtr<World>
{
    HeapPtr::new(World::default())
}

// ------------------------------------------------------------------------------------------------
// create
// ------------------------------------------------------------------------------------------------

static CREATE_ONE: Slot = Slot::new();

fn sys_create_one(mut cmd: Cmd)
{
    CREATE_ONE.push(cmd.create(Hp(10)));
}

#[test]
fn t_create_applies_right_away_when_single_threaded()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_one);
    scheduler.run(UPDATE);

    let e = CREATE_ONE.one();

    // A single threaded run has nobody to synchronise with, so the entity is already there.
    assert!(world.exists(e), "{e} must exist as soon as the system returns");
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1, "a query must see it without waiting for a flush");

    world.sync_point();

    assert!(world.exists(e), "{e} must still be alive after sync_point");
    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(10));
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1);
}

// Large enough to drain the batch of 32 pre-allocated entities and ask for another one.
const MANY: u32 = 100;
static CREATE_MANY: Slot = Slot::new();

fn sys_create_many(mut cmd: Cmd)
{
    for i in 0..MANY
    {
        CREATE_MANY.push(cmd.create(Hp(i)));
    }
}

#[test]
fn t_create_past_one_pre_allocation_batch()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_many);
    scheduler.run(UPDATE);
    world.sync_point();

    let created = CREATE_MANY.all();
    assert_eq!(created.len(), MANY as usize);

    // No handle is ever handed out twice, not even at the seam between two batches.
    let mut slots: Vec<_> = created.iter().map(|e| e.idx()).collect();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(slots.len(), MANY as usize, "the cmd buffer handed out the same entity twice across pre-allocation batches");

    for (i, &e) in created.iter().enumerate()
    {
        assert!(world.exists(e), "{e} must be alive after sync_point");
        assert_eq!(testing::read_component::<Hp>(&world, e), Hp(i as u32));
    }

    // Only the entities the system asked for show up, nothing the pre-allocation left over.
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), MANY as usize);
}

// ------------------------------------------------------------------------------------------------
// add / merge / remove component
// ------------------------------------------------------------------------------------------------

static ADD_TARGET: Slot = Slot::new();

fn sys_add_component(mut cmd: Cmd)
{
    cmd.add_component(ADD_TARGET.one(), Mana(7));
}

#[test]
fn t_add_component_applies_right_away_when_single_threaded()
{
    let mut world = new_world();
    let e = world.create(Hp(1));
    ADD_TARGET.set(e);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_add_component);
    scheduler.run(UPDATE);

    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 1, "Mana is already there, no flush needed");

    world.sync_point();

    assert_eq!(testing::read_component::<Mana>(&world, e), Mana(7));
    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(1), "the old component must follow the entity into its new archetype");
}

static MERGE_TARGET: Slot = Slot::new();

fn sys_merge_component(mut cmd: Cmd)
{
    // merge both overwrites a component that is already there and adds one that is not.
    cmd.merge_component(MERGE_TARGET.one(), (Hp(99), Armor(5)));
}

#[test]
fn t_merge_component_applies_right_away_when_single_threaded()
{
    let mut world = new_world();
    let e = world.create(Hp(1));
    MERGE_TARGET.set(e);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_merge_component);
    scheduler.run(UPDATE);

    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(99), "merge has already overwritten the old value");

    world.sync_point();

    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(99));
    assert_eq!(testing::read_component::<Armor>(&world, e), Armor(5));
}

static REMOVE_TARGET: Slot = Slot::new();

fn sys_remove_component(mut cmd: Cmd)
{
    cmd.remove_component::<Mana>(REMOVE_TARGET.one());
}

#[test]
fn t_remove_component_applies_right_away_when_single_threaded()
{
    let mut world = new_world();
    let e = world.create((Hp(1), Mana(2)));
    REMOVE_TARGET.set(e);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_remove_component);
    scheduler.run(UPDATE);

    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 0, "Mana is already gone, no flush needed");

    world.sync_point();

    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 0);
    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(1));
    assert!(world.exists(e));
}

// ------------------------------------------------------------------------------------------------
// destroy
// ------------------------------------------------------------------------------------------------

static DESTROY_TARGET: Slot = Slot::new();

fn sys_destroy(mut cmd: Cmd)
{
    cmd.destroy(DESTROY_TARGET.one());
}

#[test]
fn t_destroy_applies_right_away_when_single_threaded()
{
    let mut world = new_world();
    let keep = world.create(Hp(1));
    let doomed = world.create(Hp(2));
    DESTROY_TARGET.set(doomed);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_destroy);
    scheduler.run(UPDATE);

    assert!(!world.exists(doomed), "the entity is already gone when the system returns");

    world.sync_point();

    assert!(!world.exists(doomed));
    assert!(world.exists(keep));
    assert_eq!(testing::read_component::<Hp>(&world, keep), Hp(1), "swap-remove must leave the surviving row intact");
    assert_eq!(testing::entity_stored_at_row_of(&world, keep), keep);
}

// ------------------------------------------------------------------------------------------------
// execution order
// ------------------------------------------------------------------------------------------------

static ORDERED: Slot = Slot::new();

fn sys_create_then_touch(mut cmd: Cmd)
{
    // create, add, merge, add, remove on one entity. Whichever way Cmd takes, the order has to
    // hold: run them out of order and add_component blows up, because the entity is not there yet.
    let e = cmd.create(Hp(1));
    cmd.add_component(e, Mana(2));
    cmd.merge_component(e, Hp(3));
    cmd.add_component(e, Armor(4));
    cmd.remove_component::<Mana>(e);
    ORDERED.push(e);
}

#[test]
fn t_commands_run_in_the_order_they_were_recorded()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_then_touch);
    scheduler.run(UPDATE);
    world.sync_point();

    let e = ORDERED.one();
    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(3), "merge must run after create");
    assert_eq!(testing::read_component::<Armor>(&world, e), Armor(4));
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 0, "remove must run after add");
}

static CREATE_DESTROY: Slot = Slot::new();

fn sys_create_then_destroy(mut cmd: Cmd)
{
    let keep = cmd.create(Hp(1));
    let doomed = cmd.create(Hp(2));
    cmd.destroy(doomed);
    CREATE_DESTROY.push(keep);
    CREATE_DESTROY.push(doomed);
}

#[test]
fn t_create_then_destroy_within_one_batch()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_then_destroy);
    scheduler.run(UPDATE);
    world.sync_point();

    let out = CREATE_DESTROY.all();
    assert!(world.exists(out[0]));
    assert!(!world.exists(out[1]), "an entity created and destroyed in the same batch must be gone after the flush");
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1);
}

// ------------------------------------------------------------------------------------------------
// buffer lifecycle
// ------------------------------------------------------------------------------------------------

static FLUSH_TWICE: Slot = Slot::new();

fn sys_create_for_double_flush(mut cmd: Cmd)
{
    FLUSH_TWICE.push(cmd.create(Hp(1)));
}

#[test]
fn t_a_second_sync_point_does_nothing_more()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_for_double_flush);
    scheduler.run(UPDATE);

    world.sync_point();
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1);

    world.sync_point();
    world.sync_point();
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1, "flushing an empty buffer again must not create anything");
}

static ACCUMULATE: Slot = Slot::new();

fn sys_create_for_accumulate(mut cmd: Cmd)
{
    ACCUMULATE.push(cmd.create(Hp(1)));
}

#[test]
fn t_several_runs_each_apply_right_away()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_for_accumulate);
    scheduler.run(UPDATE);
    scheduler.run(UPDATE);
    scheduler.run(UPDATE);

    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 3, "every run must land, none of them may get lost");

    world.sync_point();

    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 3, "sync_point has nothing left to replay");
}

static INTERLEAVED: Slot = Slot::new();

fn sys_create_for_interleaved(mut cmd: Cmd)
{
    INTERLEAVED.push(cmd.create(Hp(1)));
}

#[test]
fn t_flushing_between_runs()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_create_for_interleaved);

    for expected in 1..=3
    {
        scheduler.run(UPDATE);
        world.sync_point();
        assert_eq!(world.create_query::<&Hp>().into_iter().count(), expected);
    }
}

#[test]
fn t_sync_point_on_a_world_that_never_used_cmd()
{
    let mut world = World::default();
    world.create(Hp(1));
    // No worker has registered anything, so the flush has to be a no-op rather than a panic.
    world.sync_point();
    world.sync_point();
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 1);
}

// ------------------------------------------------------------------------------------------------
// several workers
// ------------------------------------------------------------------------------------------------

const PER_WORKER: u32 = 50;
static WORKER_A: Slot = Slot::new();
static WORKER_B: Slot = Slot::new();

fn sys_worker_a(mut cmd: Cmd)
{
    for i in 0..PER_WORKER
    {
        WORKER_A.push(cmd.create(Hp(i)));
    }
}

fn sys_worker_b(mut cmd: Cmd)
{
    for i in 0..PER_WORKER
    {
        WORKER_B.push(cmd.create(Mana(i)));
    }
}

#[test]
fn t_two_parallel_systems_get_a_buffer_each()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system_parallel(UPDATE, (sys_worker_a, sys_worker_b));
    scheduler.run(UPDATE);

    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 0);

    world.sync_point();

    let a = WORKER_A.all();
    let b = WORKER_B.all();
    assert_eq!(a.len(), PER_WORKER as usize);
    assert_eq!(b.len(), PER_WORKER as usize);

    // Both workers draw from the same allocator, so neither may end up with the other's slot.
    let mut slots: Vec<_> = a.iter().chain(b.iter()).map(|e| e.idx()).collect();
    slots.sort_unstable();
    slots.dedup();
    assert_eq!(slots.len(), 2 * PER_WORKER as usize, "the two workers were handed the same entity slot");

    assert_eq!(world.create_query::<&Hp>().into_iter().count(), PER_WORKER as usize);
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), PER_WORKER as usize);
    for &e in a.iter().chain(b.iter())
    {
        assert!(world.exists(e), "{e}, created by a worker, must be alive after sync_point");
    }
}

// ------------------------------------------------------------------------------------------------
// working alongside a query
// ------------------------------------------------------------------------------------------------

static SPAWNED_FROM_QUERY: AtomicUsize = AtomicUsize::new(0);

fn sys_spawn_per_row(mut cmd: Cmd, query: Query<&Hp>)
{
    for hp in query
    {
        cmd.create(Mana(hp.0));
        SPAWNED_FROM_QUERY.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn t_creating_entities_while_iterating_a_query()
{
    let mut world = new_world();
    for i in 0..10
    {
        world.create(Hp(i));
    }

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_spawn_per_row);
    scheduler.run(UPDATE);

    // The loop must see exactly the ten rows it started with, not the ones it spawns along the way.
    assert_eq!(SPAWNED_FROM_QUERY.load(Ordering::SeqCst), 10);

    world.sync_point();
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 10);
}

static DESPAWN_COUNT: AtomicU32 = AtomicU32::new(0);

fn sys_destroy_even(mut cmd: Cmd, query: Query<(&Entity, &Hp)>)
{
    for (e, hp) in query
    {
        if hp.0 % 2 == 0
        {
            cmd.destroy(*e);
            DESPAWN_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
fn t_bulk_destroy_leaves_the_survivors_intact()
{
    let mut world = new_world();
    let all: Vec<Entity> = (0..40).map(|i| world.create(Hp(i))).collect();

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system(UPDATE, sys_destroy_even);
    scheduler.run(UPDATE);
    world.sync_point();

    assert_eq!(DESPAWN_COUNT.load(Ordering::SeqCst), 20);
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 20);

    for (i, &e) in all.iter().enumerate()
    {
        let alive = i % 2 == 1;
        assert_eq!(world.exists(e), alive, "{e} (hp={i}) is alive or dead when it should be the other way round");
        if alive
        {
            assert_eq!(testing::read_component::<Hp>(&world, e), Hp(i as u32));
            assert_eq!(testing::entity_stored_at_row_of(&world, e), e, "swap-remove knocked the row mapping of {e} out of line");
        }
    }
}

// ------------------------------------------------------------------------------------------------
// the parallel way: inside a group, every command waits for the flush
// ------------------------------------------------------------------------------------------------

static PAR_ADD: Slot = Slot::new();
static PAR_REMOVE: Slot = Slot::new();

fn sys_par_add(mut cmd: Cmd)
{
    cmd.add_component(PAR_ADD.one(), Mana(7));
}

fn sys_par_remove(mut cmd: Cmd)
{
    cmd.remove_component::<Mana>(PAR_REMOVE.one());
}

#[test]
fn t_parallel_add_and_remove_wait_for_the_flush()
{
    let mut world = new_world();
    let added_to = world.create(Hp(1));
    let removed_from = world.create((Hp(2), Mana(2)));
    PAR_ADD.set(added_to);
    PAR_REMOVE.set(removed_from);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system_parallel(UPDATE, (sys_par_add, sys_par_remove));
    scheduler.run(UPDATE);

    // Only the Mana that was there before the run, neither command has touched the world yet.
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 1, "no command may land before sync_point");
    assert_eq!(testing::read_component::<Mana>(&world, removed_from), Mana(2));

    world.sync_point();

    assert_eq!(testing::read_component::<Mana>(&world, added_to), Mana(7));
    assert_eq!(testing::read_component::<Hp>(&world, added_to), Hp(1), "the old component must follow the entity into its new archetype");
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 1, "one entity gained Mana and the other lost it");
    assert_eq!(testing::read_component::<Hp>(&world, removed_from), Hp(2));
    assert!(world.exists(removed_from));
}

static PAR_MERGE: Slot = Slot::new();
static PAR_DESTROY: Slot = Slot::new();

fn sys_par_merge(mut cmd: Cmd)
{
    cmd.merge_component(PAR_MERGE.one(), (Hp(99), Armor(5)));
}

fn sys_par_destroy(mut cmd: Cmd)
{
    cmd.destroy(PAR_DESTROY.one());
}

#[test]
fn t_parallel_merge_and_destroy_wait_for_the_flush()
{
    let mut world = new_world();
    let merged = world.create(Hp(1));
    let doomed = world.create(Hp(2));
    PAR_MERGE.set(merged);
    PAR_DESTROY.set(doomed);

    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system_parallel(UPDATE, (sys_par_merge, sys_par_destroy));
    scheduler.run(UPDATE);

    assert_eq!(testing::read_component::<Hp>(&world, merged), Hp(1), "the old value must stay untouched before sync_point");
    assert!(world.exists(doomed), "the entity must stay alive until the flush");

    world.sync_point();

    assert_eq!(testing::read_component::<Hp>(&world, merged), Hp(99));
    assert_eq!(testing::read_component::<Armor>(&world, merged), Armor(5));
    assert!(!world.exists(doomed));
    assert_eq!(testing::entity_stored_at_row_of(&world, merged), merged, "swap-remove must leave the surviving row intact");
}

static PAR_ORDERED: Slot = Slot::new();
static PAR_SIDE: Slot = Slot::new();

fn sys_par_create_then_touch(mut cmd: Cmd)
{
    let e = cmd.create(Hp(1));
    cmd.add_component(e, Mana(2));
    cmd.merge_component(e, Hp(3));
    cmd.add_component(e, Armor(4));
    cmd.remove_component::<Mana>(e);
    PAR_ORDERED.push(e);
}

fn sys_par_create_then_destroy(mut cmd: Cmd)
{
    let keep = cmd.create(Armor(1));
    let doomed = cmd.create(Armor(2));
    cmd.destroy(doomed);
    PAR_SIDE.push(keep);
    PAR_SIDE.push(doomed);
}

#[test]
fn t_parallel_each_worker_replays_its_own_commands_in_order()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system_parallel(UPDATE, (sys_par_create_then_touch, sys_par_create_then_destroy));
    scheduler.run(UPDATE);

    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 0, "nothing may exist before the flush");

    world.sync_point();

    let e = PAR_ORDERED.one();
    assert_eq!(testing::read_component::<Hp>(&world, e), Hp(3), "merge must run after create");
    assert_eq!(testing::read_component::<Armor>(&world, e), Armor(4));
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 0, "remove must run after add");

    let side = PAR_SIDE.all();
    assert!(world.exists(side[0]));
    assert!(!world.exists(side[1]), "an entity created and destroyed in the same batch must be gone after the flush");
}

static MIXED_PARALLEL: Slot = Slot::new();
static MIXED_SINGLE: Slot = Slot::new();

fn sys_mixed_a(mut cmd: Cmd)
{
    MIXED_PARALLEL.push(cmd.create(Hp(1)));
}

fn sys_mixed_b(mut cmd: Cmd)
{
    MIXED_PARALLEL.push(cmd.create(Hp(2)));
}

fn sys_mixed_single(mut cmd: Cmd)
{
    MIXED_SINGLE.push(cmd.create(Mana(3)));
}

#[test]
fn t_a_single_step_after_a_parallel_one_writes_right_away_again()
{
    let mut world = new_world();
    let mut scheduler = DefaultScheduler::new(world.as_ref_mut());
    scheduler.add_system_parallel(UPDATE, (sys_mixed_a, sys_mixed_b));
    scheduler.add_system(UPDATE, sys_mixed_single);
    scheduler.run(UPDATE);

    // The parallel step must leave the world back in its single threaded mode, otherwise the
    // step behind it would be recording when it has no reason to.
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 1, "the single step must have written straight into the world");
    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 0, "the parallel step is still waiting for the flush");

    world.sync_point();

    assert_eq!(world.create_query::<&Hp>().into_iter().count(), 2);
    assert_eq!(world.create_query::<&Mana>().into_iter().count(), 1);
    for &e in MIXED_PARALLEL.all().iter().chain(MIXED_SINGLE.all().iter())
    {
        assert!(world.exists(e), "{e} must be alive after sync_point");
    }
}
