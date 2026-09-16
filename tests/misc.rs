//! Regression guards that don't fit neatly under create/destroy/add/remove/merge: zero-sized
//! components, over-aligned components, archetype registration, and archetype isolation.
mod common;

use common::*;
use xynok_ecs::{entity::Entity, world::testing, world::World};

#[test]
fn t_zero_sized_components_can_be_stored_and_destroyed()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..8).map(|_| w.create(Marker)).collect();

    for &e in &entities
    {
        assert_eq!(testing::read_component::<Marker>(&w, e), Marker);
        assert_eq!(testing::entity_stored_at_row_of(&w, e), e);
    }
    for &e in &entities
    {
        w.destroy(e);
    }
}

#[test]
fn t_over_aligned_components_land_on_an_aligned_address()
{
    let mut w = World::default();
    let e = w.create(Aligned32(0xDEAD_BEEF));

    let value: Aligned32 = testing::read_component::<Aligned32>(&w, e);
    assert_eq!(value.0, 0xDEAD_BEEF);
    assert_eq!(
        (&value as *const Aligned32).addr() % align_of::<Aligned32>(),
        0,
        "an over-aligned component must be stored at a correctly aligned address"
    );
}

#[test]
fn t_register_archetype_is_idempotent()
{
    let mut w = World::default();
    w.register_archetype::<Hp>(CFG);
    let after_first = testing::archetype_count(&w);
    w.register_archetype::<Hp>(CFG);

    assert_eq!(testing::archetype_count(&w), after_first, "registering the same archetype twice must not create a second one");
}

#[test]
fn t_unrelated_archetypes_do_not_share_rows()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    let b = w.create(Mana(2));

    assert_ne!(testing::entity_location(&w, a).arch_id, testing::entity_location(&w, b).arch_id, "{{Hp}} and {{Mana}} are different archetypes");
    assert_eq!(testing::read_component::<Hp>(&w, a), Hp(1));
    assert_eq!(testing::read_component::<Mana>(&w, b), Mana(2));
}


/// `world.create((Hp(1), Hp(2)))` used to die on an `unwrap()` deep inside `ChunkLayout`, with a
/// message that named neither the archetype nor the component. A chunk keeps one column per
/// component, so the second value would land on top of the first without dropping it.
#[test]
#[should_panic(expected = "more than once")]
fn an_archetype_naming_one_component_twice_is_rejected()
{
    let mut w = World::default();
    w.create((Hp(1), Hp(2)));
}

/// The same archetype through `register_archetype`, which never builds a row: the check has to
/// sit where the archetype is registered, not where an entity is written.
#[test]
#[should_panic(expected = "more than once")]
fn registering_a_duplicated_archetype_is_rejected()
{
    let mut w = World::default();
    w.register_archetype::<(Hp, Mana, Hp)>(CFG);
}

/// The duplicate collapses onto the component set of a plain `Hp` archetype, so it can reuse
/// that archetype and never reach `ChunkLayout` at all. This is the case the layout-level check
/// on its own cannot see.
#[test]
#[should_panic(expected = "more than once")]
fn a_duplicate_that_reuses_an_existing_archetype_is_still_rejected()
{
    let mut w = World::default();
    w.create(Hp(1));
    w.create((Hp(2), Hp(3)));
}

/// Issue #33: a registered archetype uses the chunk size it was registered with.
#[test]
fn t_registered_chunk_size_is_used()
{
    use xynok_ecs::apis::ArchetypeCfg;

    let mut w = World::default();
    w.register_archetype::<Hp>(ArchetypeCfg { chunk_size_in_byte: 4 * 1024 });
    let small = w.create(Hp(1));
    let big = w.create(Mana(2));

    let small_rows = testing::max_len(&w, small);
    let big_rows = testing::max_len(&w, big);
    assert!(big_rows > small_rows * 3, "the default 16 KB chunk should hold about 4x the rows of a 4 KB one, got {big_rows} vs {small_rows}");
    assert_eq!(testing::read_component::<Hp>(&w, small), Hp(1));
}

/// `(Hp, Mana)` and `(Mana, Hp)` are the same archetype, so the second registration only has to
/// agree with the first.
#[test]
fn t_registering_an_existing_archetype_with_another_chunk_size_fails()
{
    use xynok_ecs::apis::{identifies::XynokEcsError, ArchetypeCfg};

    let mut w = World::default();
    w.register_archetype::<(Hp, Mana)>(ArchetypeCfg { chunk_size_in_byte: 8 * 1024 });

    w.try_register_archetype::<(Mana, Hp)>(ArchetypeCfg { chunk_size_in_byte: 8 * 1024 })
        .expect("same size, nothing to change");
    assert!(matches!(
        w.try_register_archetype::<(Mana, Hp)>(ArchetypeCfg { chunk_size_in_byte: 4 * 1024 }),
        Err(XynokEcsError::ArchetypeAlreadyCreatedWithDifferentChunkSize(_, 8192, 4096))
    ));
}

/// An archetype first reached through `create` already has the default size, registering it later
/// with another size is too late.
#[test]
fn t_registering_after_spawn_with_another_chunk_size_fails()
{
    use xynok_ecs::apis::{identifies::XynokEcsError, ArchetypeCfg};

    let mut w = World::default();
    w.create(Hp(1));
    assert!(matches!(
        w.try_register_archetype::<Hp>(ArchetypeCfg { chunk_size_in_byte: 4 * 1024 }),
        Err(XynokEcsError::ArchetypeAlreadyCreatedWithDifferentChunkSize(_, _, _))
    ));
}

#[test]
fn t_invalid_chunk_size_is_rejected()
{
    use xynok_ecs::apis::{identifies::XynokEcsError, ArchetypeCfg};

    let mut w = World::default();
    for size in [0, 1023]
    {
        assert!(matches!(
            w.try_register_archetype::<Hp>(ArchetypeCfg { chunk_size_in_byte: size }),
            Err(XynokEcsError::InvalidChunkSize(_, _))
        ));
    }
    assert_eq!(testing::archetype_count(&w), 0, "a rejected cfg must not create the archetype");
}
