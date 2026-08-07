//! TEMPORARY - scratch verification, delete after reading.
mod common;

use common::*;
use xynok_ecs::world::{testing, World};

/// Registers enough distinct archetypes to push `World::archetypes` past several of its growth
/// boundaries (hashbrown reallocates at 3, 7, 14, 28 entries for a map built empty).
fn register_many_archetypes(w: &mut World)
{
    w.register_archetype::<Mana>();
    w.register_archetype::<Pos>();
    w.register_archetype::<Marker>();
    w.register_archetype::<Aligned32>();
    w.register_archetype::<(Hp, Mana)>();
    w.register_archetype::<(Hp, Pos)>();
    w.register_archetype::<(Hp, Marker)>();
    w.register_archetype::<(Hp, Aligned32)>();
    w.register_archetype::<(Mana, Pos)>();
    w.register_archetype::<(Mana, Marker)>();
    w.register_archetype::<(Mana, Aligned32)>();
    w.register_archetype::<(Pos, Marker)>();
    w.register_archetype::<(Pos, Aligned32)>();
    w.register_archetype::<(Marker, Aligned32)>();
    w.register_archetype::<(Hp, Mana, Pos)>();
    w.register_archetype::<(Hp, Mana, Marker)>();
}

/// entities exist BEFORE the query is built, so `QuerySpec.archetypes` is non-empty from the
/// start. Only the *address* of that Vec is under test here, not its freshness.
#[test]
fn t_vec_addr_survives_query_counter_rehash()
{
    let mut w = World::default();
    let expected: u32 = (0..10u32)
        .inspect(|&i| {
            w.create(Hp(i));
        })
        .sum();

    let query = w.create_query::<&Hp>();
    assert_eq!(query.into_iter().map(|hp| hp.0).sum::<u32>(), expected, "sanity");

    // force the query_counter map well past several growth boundaries
    let _ = w.create_query::<&Mana>();
    let _ = w.create_query::<&Pos>();
    let _ = w.create_query::<&Marker>();
    let _ = w.create_query::<&Aligned32>();
    let _ = w.create_query::<(&Hp, &Mana)>();
    let _ = w.create_query::<(&Hp, &Pos)>();
    let _ = w.create_query::<(&Mana, &Pos)>();
    let _ = w.create_query::<(&Mana, &Marker)>();
    let _ = w.create_query::<(&Pos, &Marker)>();
    let _ = w.create_query::<(&Hp, &Marker)>();
    let _ = w.create_query::<(&Hp, &Mana, &Pos)>();
    let _ = w.create_query::<(&Hp, &Mana, &Marker)>();

    w.create((Hp(1), Mana(2)));
    w.create((Pos { x: 1f32, y: 1f32 }, Mana(2)));
    assert_eq!(
        query.into_iter().map(|hp| hp.0).sum::<u32>(),
        expected,
        "the Vec behind the accessor moved during rehash"
    );
}

/// same thing, but the whole `World` is moved after the query is handed out.
#[test]
fn t_accessor_survives_world_move()
{
    let mut w = World::default();
    let expected: u32 = (0..10u32)
        .inspect(|&i| {
            w.create(Hp(i));
        })
        .sum();
    let query = w.create_query::<&Hp>();

    let boxed = Box::new(w); // World body relocates to the heap
    let mut moved = *boxed; // ... and back onto the stack, at a different address
    let _ = moved.create_query::<&Mana>();

    assert_eq!(query.into_iter().map(|hp| hp.0).sum::<u32>(), expected, "accessor did not survive the move");
}

// ------------------------------------------------------------------------------------------------
// tier 3a - the archetype indices cached inside `QuerySpec.archetypes`
// ------------------------------------------------------------------------------------------------

/// The direct statement of the invariant, with no query involved.
///
/// `QuerySpec` used to cache `*mut ArchetypeSpec`, which made "the specs never relocate" a hard
/// requirement and forced every entry to be boxed. It now caches indices instead, so the specs
/// are free to move within the registry's dense storage - what must not change is the index a
/// given archetype id maps to.
#[test]
fn t_archetype_index_survives_new_archetypes()
{
    let mut w = World::default();
    let anchor = w.create(Hp(1));

    let before = testing::archetype_index(&w, anchor);
    let count_before = testing::archetype_count(&w);

    register_many_archetypes(&mut w);

    // guards the test itself: if the registry never actually grew, the check proves nothing
    assert!(
        testing::archetype_count(&w) > count_before + 8,
        "the archetype registry did not grow enough to be a real test"
    );

    let after = testing::archetype_index(&w, anchor);
    assert_eq!(
        before, after,
        "archetype index moved: every index cached in a QuerySpec now points at the wrong archetype"
    );
}

/// The same invariant seen through the public API: whatever the registry did to its storage,
/// the query must still read the right rows back.
#[test]
fn t_query_reads_correctly_after_new_archetypes()
{
    let mut w = World::default();
    let expected: u32 = (0..10u32)
        .inspect(|&i| {
            w.create(Hp(i));
        })
        .sum();

    let query = w.create_query::<&Hp>();
    assert_eq!(query.into_iter().map(|hp| hp.0).sum::<u32>(), expected, "sanity");

    register_many_archetypes(&mut w);

    assert_eq!(
        query.into_iter().map(|hp| hp.0).sum::<u32>(),
        expected,
        "query read through a relocated ArchetypeSpec"
    );
}
