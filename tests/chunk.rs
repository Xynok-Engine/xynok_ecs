//! Integration tests for chunk packing: how rows are laid out into and across chunks as an
//! archetype fills up, overflows, and frees rows.
mod common;

use common::*;
use xynok_ecs::{world::testing, world::World};

/// A chunk sized for `max_len` rows must actually accept `max_len` rows before a second
/// chunk is allocated.
#[test]
fn t_entities_pack_into_a_single_chunk_up_to_max_len()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = testing::max_len(&w, probe);
    assert!(max_len > 1, "this test needs an archetype holding more than one row per chunk");

    let mut entities = vec![probe];
    for i in 1..max_len as u32
    {
        entities.push(w.create(Hp(i)));
    }

    assert_eq!(testing::chunk_count(&w, probe), 1, "{max_len} rows must fit in a single chunk");
    for (i, &e) in entities.iter().enumerate()
    {
        assert_eq!(testing::entity_location(&w, e).chunk_idx, 0, "{e} should live in the first chunk");
        assert_eq!(testing::read_component::<Hp>(&w, e), Hp(i as u32));
    }
}

#[test]
fn t_overflowing_a_chunk_allocates_exactly_one_more()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = testing::max_len(&w, probe);

    for i in 1..=max_len as u32
    {
        w.create(Hp(i));
    }

    assert_eq!(testing::chunk_count(&w, probe), 2, "row max_len + 1 must open a second chunk, not more");
}

#[test]
fn t_a_full_chunk_never_receives_more_rows_than_it_can_hold()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = testing::max_len(&w, probe);

    for i in 1..(max_len * 2) as u32
    {
        w.create(Hp(i));
    }

    let chunk_count = testing::chunk_count(&w, probe);
    for chunk_idx in 0..chunk_count
    {
        let len = testing::chunk_len(&w, probe, chunk_idx);
        assert!(len <= max_len, "chunk {chunk_idx} holds {len} rows but only fits {max_len}");
    }
}

/// A chunk that just dropped below capacity must be reusable instead of leaking a new
/// allocation.
#[test]
fn t_a_chunk_is_reused_after_a_row_is_freed()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = testing::max_len(&w, probe);

    let mut entities = vec![probe];
    for i in 1..max_len as u32
    {
        entities.push(w.create(Hp(i)));
    }
    assert_eq!(testing::chunk_count(&w, probe), 1);

    w.destroy(entities[0]);
    let recycled = w.create(Hp(999));

    assert_eq!(
        testing::chunk_count(&w, recycled),
        1,
        "the freed row should be reused instead of allocating a new chunk"
    );
}

/// Freeing three rows of the same chunk must not enqueue that chunk into the free list more than
/// once — otherwise a later refill hands out an already-full chunk and writes past `max_len`.
#[test]
fn t_freeing_rows_must_not_enqueue_the_same_chunk_twice()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = testing::max_len(&w, probe);

    let mut entities = vec![probe];
    for i in 1..max_len as u32
    {
        entities.push(w.create(Hp(i)));
    }
    assert_eq!(testing::free_chunk_count(&w, probe), 0, "a chunk filled to max_len must not stay in the free list");

    for &e in entities.iter().take(3)
    {
        w.destroy(e);
    }
    assert_eq!(
        testing::free_chunk_count(&w, probe),
        1,
        "freeing three rows of one chunk must leave that chunk in the free list exactly once"
    );

    for i in 0..4u32
    {
        let e = w.create(Hp(900 + i));
        let loc = testing::entity_location(&w, e);
        assert!(
            loc.idx_in_chunk < max_len,
            "row {} of chunk {} is outside the chunk (max_len = {max_len})",
            loc.idx_in_chunk,
            loc.chunk_idx
        );
    }
}
