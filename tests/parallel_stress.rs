//! Stress test for E1 + E2 + E3 together: a parallel system group, a query split by chunk, and
//! structural change going through the command buffer.
//!
//! Every loop here has a **hard ceiling**. An uncapped accumulating loop in a test like this is the
//! fastest way to eat a machine's RAM when something underneath is wrong, and at that point the
//! first thing to break is the machine rather than the test.
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use common::*;
use xynok_concurrency::pool::Config as LaneConfig;
use xynok_ecs::cmd_buffer::Commands;
use xynok_ecs::entity::Entity;
use xynok_ecs::query::Query;
use xynok_ecs::schedule::lane::Lane;
use xynok_ecs::schedule::scheduler::{DefaultScheduleSession as Session, DefaultScheduler, TScheduler};
use xynok_ecs::world::World;
use xynok_std::unsafe_ptr::HeapPtr;

/// Frames to run. Hard ceiling.
const FRAMES: usize = 120;
/// Entities to start with. Hard ceiling.
const START_ENTITIES: u32 = 900;
/// The world is never allowed past this.
const MAX_ENTITIES: usize = 4_000;

static TICKS: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static DOOMED: Mutex<Vec<Entity>> = Mutex::new(Vec::new());
static STRESS_LOCK: Mutex<()> = Mutex::new(());

/// Writes the `Hp` column, split by chunk.
fn tick_hp(query: Query<&mut Hp>, lane: Lane)
{
    query.par_for_each_chunk(lane.pool(), 2, |view| {
        for hp in view.columns
        {
            hp.0 = hp.0.wrapping_add(1);
        }
    });
}

/// Writes the `Mana` column, alongside [`tick_hp`]: a different column of the same chunks.
fn tick_mana(query: Query<&mut Mana>, lane: Lane)
{
    query.par_for_each_chunk(lane.pool(), 2, |view| {
        for mana in view.columns
        {
            mana.0 = mana.0.wrapping_add(2);
        }
    });
}

/// Counts the live entities and picks a few to destroy in the next frame.
fn survey(query: Query<&Hp>)
{
    let mut live = 0usize;
    let mut doomed = DOOMED.lock().unwrap_or_else(|e| e.into_inner());
    doomed.clear();

    query.for_each_chunk(|view| {
        live += view.len();
        // Hard ceiling: at most one per chunk, and at most 64 per frame
        if doomed.len() < 64 && !view.is_empty()
        {
            doomed.push(view.entities[0]);
        }
    });

    LIVE.store(live, Ordering::SeqCst);
    TICKS.fetch_add(1, Ordering::SeqCst);
}

/// The frame's structural change: destroy the ones picked, then replace them if there is room.
fn churn(cmd: Commands)
{
    let doomed = DOOMED.lock().unwrap_or_else(|e| e.into_inner());
    for e in doomed.iter()
    {
        cmd.destroy(*e);
    }

    let live = LIVE.load(Ordering::SeqCst);
    let room = MAX_ENTITIES.saturating_sub(live);
    // Replace exactly as many as were destroyed, and never go past the ceiling
    let spawn = doomed.len().min(room);
    for i in 0..spawn
    {
        cmd.create((Hp(i as u32), Mana(i as u32)));
    }
}

#[test]
fn t_parallel_frames_keep_the_world_consistent()
{
    let _serialise = STRESS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    TICKS.store(0, Ordering::SeqCst);

    let mut w = World::default();
    for i in 0..START_ENTITIES
    {
        w.create((Hp(i), Mana(i)));
    }
    // A second archetype, so the queries match more than one
    for i in 0..START_ENTITIES / 3
    {
        w.create(Hp(i));
    }

    let mut scheduler = DefaultScheduler::with_lane_config(
        HeapPtr::new(w),
        LaneConfig {
            threads: 3,
            ..LaneConfig::default()
        },
    );
    scheduler
        .add_system_parallel(Session::Update, (tick_hp, tick_mana))
        .add_system(Session::Update, survey)
        .add_system(Session::Update, churn);

    for _ in 0..FRAMES
    {
        scheduler.run(Session::Update);
        assert!(LIVE.load(Ordering::SeqCst) <= MAX_ENTITIES, "the world went past the hard ceiling of {MAX_ENTITIES} entities");
    }

    assert_eq!(TICKS.load(Ordering::SeqCst), FRAMES, "some frame did not run all of its steps");

    // The entity count must hold steady: each frame replaces exactly what it destroyed, and no
    // frame ever reached the ceiling
    let expected = START_ENTITIES as usize + (START_ENTITIES / 3) as usize;
    assert_eq!(LIVE.load(Ordering::SeqCst), expected, "destroy and create did not balance out");
}
