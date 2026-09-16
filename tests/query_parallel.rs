//! `par_for_each_chunk` and `par_for_each_batch` have to touch every row exactly once, whether
//! the world has an executor or not, and even when they are called from a system that is itself
//! running inside a parallel group.

use std::sync::atomic::{AtomicUsize, Ordering};

use xynok_concurrency::thread_pool::cfg::CfgThreadPool;
use xynok_concurrency::thread_pool::ThreadPool;
use xynok_ecs::component;
use xynok_ecs::entity::Entity;
use xynok_ecs::query::filter::Changed;
use xynok_ecs::query::Query;
use xynok_ecs::schedule::executor::{JobArgs, TJobExecutor};
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
use xynok_ecs::world::World;
use xynok_std::unsafe_ptr::HeapPtr;

#[component(ChangeAble)]
struct Hp(u32);

#[component]
struct Mana(u32);

// A chunk is 16 KiB, so this many rows fill several chunks in each archetype
const PER_ARCHETYPE: u32 = 5_000;

fn fill(w: &mut World)
{
    for i in 0..PER_ARCHETYPE
    {
        w.create(Hp(i));
    }
    for i in PER_ARCHETYPE..2 * PER_ARCHETYPE
    {
        let e = w.create(Hp(i));
        w.add_component(e, Mana(i));
    }
}

fn with_pool() -> World
{
    let mut w = World::default();
    w.set_executor(Some(Box::new(ThreadPool::new(CfgThreadPool::new("test pool", 4)))));
    fill(&mut w);
    w
}

/// Every Hp went from `i` to `i + added`
fn assert_all_added(w: &mut World, added: u32)
{
    let mut seen: Vec<u32> = w.create_query::<&Hp>().into_iter().map(|hp| hp.0).collect();
    seen.sort_unstable();
    let expected: Vec<u32> = (0..2 * PER_ARCHETYPE).map(|i| i + added).collect();
    assert_eq!(seen, expected);
}

#[test]
fn t_par_chunk_writes_every_row_once_with_a_pool()
{
    let mut w = with_pool();
    w.create_query::<&mut Hp>().par_for_each_chunk(|mut hp| hp.0 += 7);
    assert_all_added(&mut w, 7);
}

#[test]
fn t_par_batch_writes_every_row_once_with_a_pool()
{
    let mut w = with_pool();
    let value = 3;
    w.create_query::<&mut Hp>().par_for_each_batch(100, move |mut hp| hp.0 += value);
    assert_all_added(&mut w, 3);
}

#[test]
fn t_par_falls_back_to_the_caller_without_an_executor()
{
    let mut w = World::default();
    fill(&mut w);
    assert!(w.executor().is_none());

    let mut query = w.create_query::<&mut Hp>();
    query.par_for_each_chunk(|mut hp| hp.0 += 1);
    query.par_for_each_batch(64, |mut hp| hp.0 += 1);
    assert_all_added(&mut w, 2);
}

#[test]
fn t_par_keeps_filters_and_change_ticks()
{
    let mut w = with_pool();
    let seen_up_to = w.capture_current_tick();

    // only every 10th row is written, so only those must show up as changed
    w.create_query::<&mut Hp>().par_for_each_batch(64, |mut hp| {
        if hp.0 % 10 == 0
        {
            hp.0 += 1;
        }
    });

    let count = AtomicUsize::new(0);
    w.create_query_since::<Changed<&Hp>>(seen_up_to).par_for_each_chunk(|_| {
        count.fetch_add(1, Ordering::Relaxed);
    });
    assert_eq!(count.into_inner(), (2 * PER_ARCHETYPE / 10) as usize);
}

#[test]
fn t_par_inside_a_parallel_system_group()
{
    fn add_hp(mut query: Query<&mut Hp>)
    {
        query.par_for_each_chunk(|mut hp| hp.0 += 1);
    }
    fn add_mana(mut query: Query<&mut Mana>)
    {
        query.par_for_each_batch(128, |mana| mana.0 += 1);
    }
    fn check(query: Query<(&Hp, &Mana)>)
    {
        let mut count = 0;
        for (hp, mana) in query
        {
            // both started equal, and each got +1 from its own job group
            assert_eq!(hp.0, mana.0);
            count += 1;
        }
        assert_eq!(count, PER_ARCHETYPE);
    }

    let mut world = HeapPtr::new(World::default());
    fill(&mut world);

    let mut scheduler = DefaultScheduler::new(world);
    scheduler
        .add_system_parallel(DefaultScheduleSession::Update, (add_hp, add_mana))
        .add_system(DefaultScheduleSession::Update, check);
    scheduler.run(DefaultScheduleSession::Update);
    scheduler.run(DefaultScheduleSession::Update);
}

/// Runs every job right on the caller, one after another, and counts them
#[derive(Default)]
struct CountingExecutor
{
    jobs: AtomicUsize,
}

impl TJobExecutor for CountingExecutor
{
    fn run_jobs(&self, args: &mut dyn Iterator<Item = JobArgs>, job: &(dyn Fn(JobArgs) + Sync))
    {
        for args in args
        {
            self.jobs.fetch_add(1, Ordering::Relaxed);
            job(args);
        }
    }
}

#[test]
fn t_par_jobs_match_iter_chunk_and_iter_batch_on_uneven_chunks()
{
    let mut w = World::default();
    fill(&mut w);
    // punch holes of different sizes, so chunks end up with uneven lengths
    let doomed: Vec<_> = w.create_query::<(&Entity, &Hp)>().into_iter().filter(|(_, hp)| hp.0 % 7 == 0 || hp.0 % 13 < 3).map(|(e, _)| *e).collect();
    for e in doomed
    {
        w.destroy(e);
    }
    let expected: Vec<u32> = w.create_query::<&Hp>().into_iter().map(|hp| hp.0 + 1).collect();

    let executor = Box::new(CountingExecutor::default());
    let jobs: *const AtomicUsize = &executor.jobs;
    w.set_executor(Some(executor));
    // SAFETY: the executor lives in the world for the rest of this test
    let jobs = unsafe { &*jobs };

    for batch_amount in [8, 9, 100, 1_000, 100_000]
    {
        let wanted = w.create_query::<&mut Hp>().iter_batch(batch_amount).count();
        jobs.store(0, Ordering::Relaxed);
        w.create_query::<&mut Hp>().par_for_each_batch(batch_amount, |_| {});
        assert_eq!(jobs.load(Ordering::Relaxed), wanted, "batch_amount {batch_amount}");
    }

    let wanted = w.create_query::<&mut Hp>().iter_chunk().count();
    jobs.store(0, Ordering::Relaxed);
    w.create_query::<&mut Hp>().par_for_each_chunk(|mut hp| hp.0 += 1);
    assert_eq!(jobs.load(Ordering::Relaxed), wanted);

    let mut seen: Vec<u32> = w.create_query::<&Hp>().into_iter().map(|hp| hp.0).collect();
    let mut expected = expected;
    seen.sort_unstable();
    expected.sort_unstable();
    assert_eq!(seen, expected);
}
