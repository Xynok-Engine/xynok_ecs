//! Integration tests for E2: splitting a query by chunk and running it across the pool.
mod common;

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use common::*;
use xynok_concurrency::pool::{Config as LaneConfig, ThreadPool};
use xynok_ecs::entity::Entity;
use xynok_ecs::world::{testing, World};

fn lane(threads: usize) -> ThreadPool
{
    ThreadPool::new(LaneConfig {
        threads: threads,
        ..LaneConfig::default()
    })
}

/// Creates enough entities for the archetype to span more than one chunk, and reports how many
/// chunks it ended up with.
fn fill_two_chunks(w: &mut World) -> (Vec<Entity>, usize)
{
    let probe = w.create(Hp(0));
    let max_len = testing::max_len(w, probe);

    let mut live = vec![probe];
    for i in 1..(max_len * 2 + 3)
    {
        live.push(w.create(Hp(i as u32)));
    }

    let chunks = testing::chunk_count(w, probe);
    assert!(chunks >= 3, "this test needs an archetype spanning several chunks, it has {chunks}");
    (live, chunks)
}

#[test]
fn t_for_each_chunk_visits_every_row_once()
{
    let mut w = World::default();
    let (live, _) = fill_two_chunks(&mut w);

    let mut seen = Vec::new();
    w.create_query::<&Hp>().for_each_chunk(|view| {
        assert_eq!(view.len(), view.columns.len(), "the entity count and the column length must agree");
        assert!(!view.is_empty(), "an empty chunk must not reach the closure");
        seen.extend(view.columns.iter().map(|hp| hp.0));
    });

    seen.sort_unstable();
    let expected: Vec<u32> = (0..live.len() as u32).collect();
    assert_eq!(seen, expected);
}

#[test]
fn t_chunk_view_entities_line_up_with_columns()
{
    let mut w = World::default();
    let (live, _) = fill_two_chunks(&mut w);
    let live: HashSet<Entity> = live.into_iter().collect();

    let mut pairs = Vec::new();
    w.create_query::<&Hp>().for_each_chunk(|view| {
        for (e, hp) in view.entities.iter().zip(view.columns.iter())
        {
            pairs.push((*e, hp.0));
        }
    });

    assert_eq!(pairs.len(), live.len());
    for (e, _) in pairs.iter()
    {
        assert!(live.contains(e), "{e} is not an entity of this world");
    }
    // Entity `i` was created with `Hp(i as u32)`, so every row must still hold that pair
    for (e, hp) in pairs
    {
        assert_eq!(hp, e.idx() as u32, "{e} sits next to another row's Hp value");
    }
}

#[test]
fn t_par_for_each_chunk_writes_every_row_exactly_once()
{
    let mut w = World::default();
    let (live, chunks) = fill_two_chunks(&mut w);
    let pool = lane(3);

    // `batch = 1` is one chunk per batch: maximum splitting, so the test really walks the
    // parallel path
    w.create_query::<&mut Hp>().par_for_each_chunk(&pool, 1, |view| {
        for hp in view.columns
        {
            hp.0 += 1000;
        }
    });

    let seen: Vec<u32> = w.create_query::<&Hp>().into_iter().map(|hp| hp.0).collect();
    assert_eq!(seen.len(), live.len());
    let mut sorted = seen;
    sorted.sort_unstable();
    let expected: Vec<u32> = (0..live.len() as u32).map(|i| i + 1000).collect();
    assert_eq!(sorted, expected, "across {chunks} chunks a row was added to twice or skipped");
}

#[test]
fn t_par_for_each_chunk_covers_every_chunk_once()
{
    let mut w = World::default();
    let (_, chunks) = fill_two_chunks(&mut w);
    let pool = lane(3);

    static VISITS: AtomicUsize = AtomicUsize::new(0);
    VISITS.store(0, Ordering::SeqCst);

    w.create_query::<&Hp>().par_for_each_chunk(&pool, 1, |_| {
        VISITS.fetch_add(1, Ordering::SeqCst);
    });

    assert_eq!(VISITS.load(Ordering::SeqCst), chunks, "every chunk must reach exactly one job");
}

/// A `batch` larger than the total chunk count means running sequentially, and the result must
/// not change
#[test]
fn t_batch_above_the_chunk_count_runs_inline()
{
    let mut w = World::default();
    let (live, _) = fill_two_chunks(&mut w);
    let pool = lane(3);

    w.create_query::<&mut Hp>().par_for_each_chunk(&pool, usize::MAX, |view| {
        for hp in view.columns
        {
            hp.0 += 1;
        }
    });

    let total: u64 = w.create_query::<&Hp>().into_iter().map(|hp| hp.0 as u64).sum();
    let expected: u64 = (0..live.len() as u64).map(|i| i + 1).sum();
    assert_eq!(total, expected);
}

#[test]
fn t_par_for_each_chunk_on_an_empty_query_does_nothing()
{
    let mut w = World::default();
    w.create(Mana(1));
    let pool = lane(3);

    static VISITS: AtomicUsize = AtomicUsize::new(0);
    VISITS.store(0, Ordering::SeqCst);

    w.create_query::<&Hp>().par_for_each_chunk(&pool, 1, |_| {
        VISITS.fetch_add(1, Ordering::SeqCst);
    });

    assert_eq!(VISITS.load(Ordering::SeqCst), 0);
}

/// A tuple query hands back a tuple of slices, and those slices must belong to the same chunk
#[test]
fn t_tuple_query_hands_out_one_slice_per_column()
{
    let mut w = World::default();
    for i in 1..=300u32
    {
        w.create((Hp(i), Mana(i * 2)));
    }
    let pool = lane(3);

    w.create_query::<(&mut Hp, &Mana)>().par_for_each_chunk(&pool, 1, |view| {
        let (hps, manas) = view.columns;
        assert_eq!(hps.len(), manas.len(), "two columns of one chunk must have the same length");
        assert_eq!(hps.len(), view.entities.len());

        for (hp, mana) in hps.iter_mut().zip(manas.iter())
        {
            assert_eq!(mana.0, hp.0 * 2, "the two slices are off by a row");
            hp.0 = mana.0;
        }
    });

    let total: u64 = w.create_query::<&Hp>().into_iter().map(|hp| hp.0 as u64).sum();
    assert_eq!(total, (1..=300u64).map(|i| i * 2).sum::<u64>());
}

/// Several archetypes matching one query: their chunks form a single flat sequence, and no
/// archetype may be skipped
#[test]
fn t_chunks_of_several_archetypes_are_all_visited()
{
    let mut w = World::default();
    for i in 0..500u32
    {
        w.create(Hp(i));
    }
    for i in 500..900u32
    {
        w.create((Hp(i), Mana(i)));
    }
    for i in 900..1200u32
    {
        w.create((Hp(i), Mana(i), Pos { x: 0.0, y: 0.0 }));
    }
    let pool = lane(3);

    let seen: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    w.create_query::<&Hp>().par_for_each_chunk(&pool, 1, |view| {
        let mut seen = seen.lock().unwrap_or_else(|e| e.into_inner());
        seen.extend(view.columns.iter().map(|hp| hp.0));
    });

    let mut seen = seen.into_inner().unwrap_or_else(|e| e.into_inner());
    seen.sort_unstable();
    assert_eq!(seen, (0..1200u32).collect::<Vec<_>>(), "some archetype was skipped");
}

/// A chunk left empty after all of its entities were destroyed must not reach the closure
#[test]
fn t_empty_chunks_are_skipped()
{
    let mut w = World::default();
    let (live, _) = fill_two_chunks(&mut w);
    for e in live.iter()
    {
        w.destroy(*e);
    }
    let pool = lane(3);

    static VISITS: AtomicUsize = AtomicUsize::new(0);
    VISITS.store(0, Ordering::SeqCst);

    w.create_query::<&Hp>().par_for_each_chunk(&pool, 1, |_| {
        VISITS.fetch_add(1, Ordering::SeqCst);
    });
    w.create_query::<&Hp>().for_each_chunk(|_| {
        VISITS.fetch_add(1, Ordering::SeqCst);
    });

    assert_eq!(VISITS.load(Ordering::SeqCst), 0, "the world is empty yet a chunk reached the closure");
}
