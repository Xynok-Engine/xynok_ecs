//! `Query::iter`, `Query::iter_chunk` and `Query::iter_batch` all walk the same rows, just cut
//! into different pieces. These tests check that every way of walking sees each matching entity
//! exactly once, and that a batch really can be walked on another thread.

use std::collections::HashSet;

use xynok_ecs::component;
use xynok_ecs::query::filter::{Changed, Without};
use xynok_ecs::world::World;

#[component(ChangeAble)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hp(u32);

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mana(u32);

// A chunk is 16 KiB, so this many rows fill several chunks in each archetype
const PER_ARCHETYPE: u32 = 5_000;

/// Hp `0..PER_ARCHETYPE` alone, and Hp `PER_ARCHETYPE..2 * PER_ARCHETYPE` next to a Mana
fn two_archetypes() -> World
{
    let mut w = World::default();
    for i in 0..PER_ARCHETYPE
    {
        w.create(Hp(i));
    }
    for i in PER_ARCHETYPE..2 * PER_ARCHETYPE
    {
        let e = w.create(Hp(i));
        w.add_component(e, Mana(i));
    }
    w
}

fn all_hp() -> HashSet<u32>
{
    (0..2 * PER_ARCHETYPE).collect()
}

#[test]
fn t_iter_sees_every_entity_once()
{
    let mut w = two_archetypes();
    let mut query = w.create_query::<&Hp>();

    let seen: Vec<u32> = query.iter().map(|hp| hp.0).collect();
    assert_eq!(seen.len(), all_hp().len());
    assert_eq!(seen.into_iter().collect::<HashSet<_>>(), all_hp());

    // a second walk starts over from the first row
    assert_eq!(query.iter().count(), all_hp().len());
}

#[test]
fn t_iter_chunk_covers_every_entity_across_several_chunks()
{
    let mut w = two_archetypes();
    let mut query = w.create_query::<&Hp>();

    let mut chunk_count = 0;
    let mut seen = Vec::new();
    for mut chunk in query.iter_chunk()
    {
        chunk_count += 1;
        assert!(!chunk.is_empty(), "`iter_chunk` skips empty chunks");
        let before = seen.len();
        seen.extend(chunk.iter().map(|hp| hp.0));
        assert_eq!(seen.len() - before, chunk.len());
    }

    assert!(chunk_count > 2, "sanity: the rows really spread over more than one chunk per archetype");
    assert_eq!(seen.len(), all_hp().len());
    assert_eq!(seen.into_iter().collect::<HashSet<_>>(), all_hp());
}

#[test]
fn t_a_chunk_can_be_walked_by_value()
{
    let mut w = two_archetypes();
    let mut query = w.create_query::<(&Hp, &Mana)>();

    let mut total = 0;
    for chunk in query.iter_chunk()
    {
        for (hp, mana) in chunk
        {
            assert_eq!(hp.0, mana.0);
            total += 1;
        }
    }
    assert_eq!(total, PER_ARCHETYPE as usize);
}

#[test]
fn t_iter_batch_cuts_rows_into_even_batches_with_a_short_last_one()
{
    let mut w = two_archetypes();
    let mut query = w.create_query::<&Hp>();

    // an odd size, so batches have to cut through chunks and across the archetype border
    let batch_amount = 777;
    let total = all_hp().len();

    let sizes: Vec<usize> = query.iter_batch(batch_amount).map(|batch| batch.len()).collect();
    assert_eq!(sizes.len(), total.div_ceil(batch_amount));
    assert!(sizes[..sizes.len() - 1].iter().all(|&size| size == batch_amount));
    assert_eq!(*sizes.last().unwrap(), total % batch_amount);

    let mut seen = Vec::new();
    for mut batch in query.iter_batch(batch_amount)
    {
        seen.extend(batch.iter().map(|hp| hp.0));
    }
    assert_eq!(seen.len(), total);
    assert_eq!(seen.into_iter().collect::<HashSet<_>>(), all_hp());
}

#[test]
fn t_iter_batch_bigger_than_the_query_gives_one_batch()
{
    let mut w = two_archetypes();
    let mut query = w.create_query::<&Hp>();

    let batches: Vec<_> = query.iter_batch(1_000_000).collect();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].len(), all_hp().len());
}

#[test]
#[should_panic(expected = "batch_amount above 0")]
fn t_iter_batch_of_zero_panics()
{
    let mut w = two_archetypes();
    let mut query = w.create_query::<&Hp>();
    let _ = query.iter_batch(0);
}

#[test]
fn t_iterators_over_an_empty_query_are_empty()
{
    let mut w = World::default();
    w.create(Mana(1));

    let mut query = w.create_query::<&Hp>();
    assert_eq!(query.iter().count(), 0);
    assert_eq!(query.iter_chunk().count(), 0);
    assert_eq!(query.iter_batch(8).count(), 0);

    let mut without = w.create_query::<Without<Mana>>();
    assert_eq!(without.iter().count(), 0, "`Without` alone names no column to walk");
    assert_eq!(without.iter_chunk().count(), 0);
    assert_eq!(without.iter_batch(8).count(), 0);
}

#[test]
fn t_batches_count_rows_before_filters()
{
    let mut w = two_archetypes();
    let seen_up_to = w.capture_current_tick();

    for mut hp in w.create_query::<&mut Hp>()
    {
        if hp.0 % 10 == 0
        {
            hp.0 += 1;
        }
    }

    let mut query = w.create_query_since::<Changed<&Hp>>(seen_up_to);
    let batches: Vec<_> = query.iter_batch(100).collect();

    // the batch sizes ignore the filter...
    assert_eq!(batches.iter().map(|batch| batch.len()).sum::<usize>(), all_hp().len());
    // ...but walking them does not
    let changed: HashSet<u32> = batches.into_iter().flatten().map(|hp| hp.0).collect();
    let expected: HashSet<u32> = (0..2 * PER_ARCHETYPE).filter(|i| i % 10 == 0).map(|i| i + 1).collect();
    assert_eq!(changed, expected);
}

#[test]
fn t_batches_write_on_their_own_threads()
{
    let mut w = two_archetypes();
    let seen_up_to = w.capture_current_tick();

    {
        let mut query = w.create_query::<&mut Hp>();
        std::thread::scope(|scope| {
            for batch in query.iter_batch(1_000)
            {
                scope.spawn(move || {
                    for mut hp in batch
                    {
                        hp.0 += 100_000;
                    }
                });
            }
        });
    }

    let expected: HashSet<u32> = all_hp().into_iter().map(|i| i + 100_000).collect();
    let values: HashSet<u32> = w.create_query::<&Hp>().into_iter().map(|hp| hp.0).collect();
    assert_eq!(values, expected);

    // every write went through `Mut`, so every row reports as changed
    let changed = w.create_query_since::<Changed<&Hp>>(seen_up_to).into_iter().count();
    assert_eq!(changed, all_hp().len());
}
