//! Integration tests for `World::create_query` / `Query<T>` iteration.
mod common;

use std::collections::HashSet;

use common::*;
use xynok_ecs::world::World;

#[test]
fn t_query_returns_every_entity_with_the_component()
{
    let mut w = World::default();
    let expected: HashSet<u32> = (0..10u32).collect();
    for &i in &expected
    {
        w.create(Hp(i));
    }

    let query = w.create_query::<&Hp>();
    let seen: HashSet<u32> = query.into_iter().map(|hp| hp.0).collect();

    assert_eq!(seen, expected);
}

#[test]
fn t_query_ignores_entities_without_the_component()
{
    let mut w = World::default();
    w.create(Hp(1));
    w.create(Mana(2)); // different archetype, no Hp

    let query = w.create_query::<&Hp>();
    let count = query.into_iter().count();

    assert_eq!(count, 1, "an entity without Hp must not show up in a Hp query");
}

#[test]
fn t_query_with_no_matching_entities_is_empty()
{
    let mut w = World::default();
    w.create(Mana(1));

    let query = w.create_query::<&Hp>();
    assert_eq!(query.into_iter().count(), 0);
}

#[test]
fn t_query_tuple_requires_every_component_present()
{
    let mut w = World::default();
    let both = w.create(Hp(1));
    w.add_component(both, Mana(2));
    w.create(Hp(3)); // Hp only, must be excluded from a (&Hp, &Mana) query

    let query = w.create_query::<(&Hp, &Mana)>();
    let results: Vec<(u32, u32)> = query.into_iter().map(|(hp, mana)| (hp.0, mana.0)).collect();

    assert_eq!(results, vec![(1, 2)]);
}

#[test]
fn t_query_spans_multiple_archetypes_sharing_the_component()
{
    let mut w = World::default();
    w.create(Hp(1));
    let e = w.create(Hp(2));
    w.add_component(e, Mana(20)); // Hp now lives in a second archetype too

    let query = w.create_query::<&Hp>();
    let seen: HashSet<u32> = query.into_iter().map(|hp| hp.0).collect();

    assert_eq!(seen, HashSet::from([1, 2]));
}

#[test]
fn t_query_mut_allows_writing_through_the_iterator()
{
    let mut w = World::default();
    w.create(Hp(1));
    w.create(Hp(2));

    let query = w.create_query::<&mut Hp>();
    for hp in query
    {
        hp.0 *= 10;
    }

    let read_back = w.create_query::<&Hp>();
    let seen: HashSet<u32> = read_back.into_iter().map(|hp| hp.0).collect();
    assert_eq!(seen, HashSet::from([10, 20]));
}

#[test]
fn t_query_mut_and_read_can_combine_in_one_tuple()
{
    let mut w = World::default();
    let e = w.create(Hp(1));
    w.add_component(e, Mana(5));

    let query = w.create_query::<(&mut Hp, &Mana)>();
    for (hp, mana) in query
    {
        hp.0 += mana.0;
    }

    let read_back = w.create_query::<&Hp>();
    assert_eq!(read_back.into_iter().map(|hp| hp.0).collect::<Vec<_>>(), vec![6]);
}

/// A query that both reads and writes the same component in one call must be rejected: nothing
/// in `Query<T>` synchronizes the two accesses.
#[test]
#[should_panic(expected = "duplicated components")]
fn t_query_conflicting_access_on_the_same_component_is_rejected()
{
    let mut w = World::default();
    w.create(Hp(1));
    let _ = w.create_query::<(&Hp, &mut Hp)>();
}
