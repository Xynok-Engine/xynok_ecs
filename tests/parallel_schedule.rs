//! Integration tests for E1: a session is a list of steps, and a step can be a group of systems
//! that run at the same time.
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use common::*;
use xynok_concurrency::pool::Config as LaneConfig;
use xynok_ecs::query::Query;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession as Session, DefaultScheduler, TScheduler};
use xynok_ecs::world::World;
use xynok_std::unsafe_ptr::HeapPtr;

/// A four-seat pool: three workers plus the calling thread. Enough for a group to really split up,
/// and small enough that the test does not depend on the core count of the machine running it.
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
// System fixtures. They have to be `fn` items, since a system is identified by its own type.
// ------------------------------------------------------------------------------------------------

fn double_hp(query: Query<&mut Hp>)
{
    for hp in query
    {
        hp.0 *= 2;
    }
}

fn triple_mana(query: Query<&mut Mana>)
{
    for mana in query
    {
        mana.0 *= 3;
    }
}

fn read_hp(query: Query<&Hp>)
{
    let _ = query.into_iter().count();
}

fn sum_hp_into_total(query: Query<&Hp>)
{
    TOTAL.fetch_add(query.into_iter().map(|hp| hp.0 as usize).sum::<usize>(), Ordering::SeqCst);
}

fn sum_mana_into_total(query: Query<&Mana>)
{
    TOTAL.fetch_add(query.into_iter().map(|mana| mana.0 as usize).sum::<usize>(), Ordering::SeqCst);
}

/// The accumulating systems all write here, because a `fn` system carries no state of its own.
/// That means they share one cell across tests, and cargo runs tests in parallel, so
/// [`TOTAL_LOCK`] queues up whichever tests touch it.
static TOTAL: AtomicUsize = AtomicUsize::new(0);
static TOTAL_LOCK: Mutex<()> = Mutex::new(());
static ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn note_a()
{
    ORDER.lock().unwrap().push("a");
}
fn note_b()
{
    ORDER.lock().unwrap().push("b");
}
fn note_c()
{
    ORDER.lock().unwrap().push("c");
}

// ------------------------------------------------------------------------------------------------

/// Runs twice on purpose: doubling twice is a factor of four, so a system running one time too many
/// or too few moves the number a long way rather than a little.
#[test]
fn t_parallel_group_runs_every_system_exactly_once_per_run()
{
    let mut w = World::default();
    for i in 1..=64u32
    {
        w.create((Hp(i), Mana(i)));
    }

    let _serialise = TOTAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut scheduler = scheduler_with(w);
    scheduler.add_system_parallel(Session::Update, (double_hp, triple_mana));
    scheduler.add_system(Session::LateUpdate, sum_hp_into_total);
    scheduler.add_system(Session::AppQuit, sum_mana_into_total);

    scheduler.run(Session::Update);
    scheduler.run(Session::Update);

    TOTAL.store(0, Ordering::SeqCst);
    scheduler.run(Session::LateUpdate);
    assert_eq!(TOTAL.load(Ordering::SeqCst), (1..=64usize).map(|i| i * 4).sum::<usize>(), "Hp must be doubled exactly twice");

    TOTAL.store(0, Ordering::SeqCst);
    scheduler.run(Session::AppQuit);
    assert_eq!(TOTAL.load(Ordering::SeqCst), (1..=64usize).map(|i| i * 9).sum::<usize>(), "Mana must be tripled exactly twice");
}

#[test]
fn t_parallel_group_writes_disjoint_columns_correctly()
{
    let mut w = World::default();
    for i in 1..=200u32
    {
        w.create((Hp(i), Mana(i)));
    }

    let _serialise = TOTAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut scheduler = scheduler_with(w);
    scheduler.add_system_parallel(Session::Update, (double_hp, triple_mana));
    scheduler.run(Session::Update);

    // Read back through a sequential system, so the world stays where it is inside the scheduler
    TOTAL.store(0, Ordering::SeqCst);
    scheduler.add_system(Session::LateUpdate, sum_hp_into_total);
    scheduler.run(Session::LateUpdate);

    let expected: usize = (1..=200usize).map(|i| i * 2).sum();
    assert_eq!(TOTAL.load(Ordering::SeqCst), expected, "the Hp column must be doubled exactly once");
}

/// The referee has to reject a wrong declaration at the call site, not at some later `run`
#[test]
#[should_panic(expected = "conflict")]
fn t_conflicting_group_is_rejected_at_add_system_parallel()
{
    let mut w = World::default();
    w.create((Hp(1), Mana(1)));

    let mut scheduler = scheduler_with(w);
    scheduler.add_system_parallel(Session::Update, (read_hp, double_hp));
}

/// Two systems only reading the same component do not conflict
#[test]
fn t_two_readers_of_one_component_may_share_a_group()
{
    fn read_hp_again(query: Query<&Hp>)
    {
        let _ = query.into_iter().count();
    }

    let mut w = World::default();
    w.create(Hp(7));

    let mut scheduler = scheduler_with(w);
    scheduler.add_system_parallel(Session::Update, (read_hp, read_hp_again));
    scheduler.run(Session::Update);
}

/// A writing system that lands in a group twice conflicts with itself
#[test]
#[should_panic(expected = "conflict")]
fn t_a_writer_cannot_share_a_group_with_itself()
{
    let mut w = World::default();
    w.create(Hp(1));

    let mut scheduler = scheduler_with(w);
    scheduler.add_system_parallel(Session::Update, (double_hp, double_hp));
}

/// Steps run in declaration order, even with a parallel group sitting in the middle
#[test]
fn t_steps_run_in_declaration_order()
{
    ORDER.lock().unwrap().clear();

    let mut w = World::default();
    w.create(Hp(1));

    let mut scheduler = scheduler_with(w);
    scheduler
        .add_system(Session::Update, note_a)
        .add_system_parallel(Session::Update, (double_hp, triple_mana))
        .add_system(Session::Update, note_b)
        .add_system(Session::Update, note_c);

    scheduler.run(Session::Update);

    assert_eq!(*ORDER.lock().unwrap(), vec!["a", "b", "c"], "a later step must not overtake an earlier one");
}

#[test]
fn t_steps_list_reflects_what_was_added()
{
    let mut w = World::default();
    w.create((Hp(1), Mana(1)));

    let mut scheduler = scheduler_with(w);
    scheduler
        .add_system(Session::Update, read_hp)
        .add_system_parallel(Session::Update, (double_hp, triple_mana));

    let steps = scheduler.steps(Session::Update);
    assert_eq!(steps.len(), 2, "one single system plus one group is two steps");
    assert_eq!(steps[0].len(), 1);
    assert_eq!(steps[1].len(), 2);
    assert!(scheduler.steps(Session::Start).is_empty(), "a session nobody added to must be empty");
}

/// `threads: 0` keeps the semantics and only removes the parallelism. It is how you answer "is this
/// bug about threading?" without editing a line of code.
#[test]
fn t_inline_lane_runs_the_same_group_sequentially()
{
    let mut w = World::default();
    for i in 1..=32u32
    {
        w.create((Hp(i), Mana(i)));
    }

    let _serialise = TOTAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut scheduler = DefaultScheduler::with_lane_config(HeapPtr::new(w), LaneConfig::inline());
    assert_eq!(scheduler.lane().worker_threads(), 0);

    scheduler.add_system_parallel(Session::Update, (double_hp, triple_mana));
    scheduler.run(Session::Update);

    TOTAL.store(0, Ordering::SeqCst);
    scheduler.add_system(Session::LateUpdate, sum_hp_into_total);
    scheduler.run(Session::LateUpdate);

    assert_eq!(TOTAL.load(Ordering::SeqCst), (1..=32usize).map(|i| i * 2).sum::<usize>());
}

/// A new archetype appearing between two steps makes every `QuerySpec` stale, and refreshing one is
/// a **write** into the world. The preparation pass before spawning exists for exactly this: were
/// that refresh left inside the jobs, the whole group would rebuild one archetype list at once.
#[test]
fn t_a_group_sees_archetypes_created_by_the_step_before_it()
{
    use xynok_ecs::cmd_buffer::Commands;

    static SEEN_HP: AtomicUsize = AtomicUsize::new(0);
    static SEEN_MANA: AtomicUsize = AtomicUsize::new(0);

    fn spawn_a_new_archetype(cmd: Commands)
    {
        for i in 1..=40u32
        {
            cmd.create((Hp(i), Mana(i)));
        }
    }
    fn count_hp(query: Query<&Hp>)
    {
        SEEN_HP.store(query.into_iter().count(), Ordering::SeqCst);
    }
    fn count_mana(query: Query<&Mana>)
    {
        SEEN_MANA.store(query.into_iter().count(), Ordering::SeqCst);
    }

    let mut scheduler = scheduler_with(World::default());
    scheduler
        .add_system(Session::Update, spawn_a_new_archetype)
        .add_system_parallel(Session::Update, (count_hp, count_mana));

    scheduler.run(Session::Update);

    assert_eq!(SEEN_HP.load(Ordering::SeqCst), 40, "the parallel group did not see the archetype the previous step created");
    assert_eq!(SEEN_MANA.load(Ordering::SeqCst), 40);
}
