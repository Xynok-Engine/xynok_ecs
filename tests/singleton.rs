//! Singleton archetypes: one entity at most, chunk sized to fit that one row.
mod common;

use common::*;
use xynok_ecs::apis::identifies::XynokEcsError;
use xynok_ecs::world::{testing, World};

#[test]
fn t_create_singleton_stores_the_values()
{
    let mut w = World::default();
    let e = w.create_singleton((Hp(10), Mana(20)));

    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(10));
    assert_eq!(testing::read_component::<Mana>(&w, e), Mana(20));
}

#[test]
fn t_second_singleton_is_rejected()
{
    let mut w = World::default();
    w.create_singleton((Hp(1), Mana(1)));

    let err = w.try_create_singleton((Hp(2), Mana(2))).unwrap_err();
    assert!(matches!(err, XynokEcsError::SingletonAlreadyExists(_)));
}

#[test]
#[should_panic(expected = "already has its entity")]
fn t_create_singleton_panics_on_duplicate()
{
    let mut w = World::default();
    w.create_singleton(Hp(1));
    w.create_singleton(Hp(2));
}

#[test]
#[should_panic(expected = "already has its entity")]
fn t_plain_create_on_a_singleton_layout_is_rejected()
{
    let mut w = World::default();
    w.create_singleton(Hp(1));
    w.create(Hp(2));
}

#[test]
fn t_same_components_in_another_order_hit_the_same_singleton()
{
    let mut w = World::default();
    w.create_singleton((Hp(1), Mana(1)));

    let err = w.try_create_singleton((Mana(2), Hp(2))).unwrap_err();
    assert!(matches!(err, XynokEcsError::SingletonAlreadyExists(_)));
}

#[test]
fn t_singleton_can_be_spawned_again_after_destroy()
{
    let mut w = World::default();
    let first = w.create_singleton(Hp(1));
    w.destroy(first);

    let second = w.create_singleton(Hp(2));
    assert_eq!(testing::read_component::<Hp>(&w, second), Hp(2));
}

#[test]
fn t_add_component_on_a_singleton_is_rejected_and_changes_nothing()
{
    let mut w = World::default();
    let e = w.create_singleton((Hp(1), Mana(1)));

    let err = w.try_add_component(e, Pos { x: 0.0, y: 0.0 }).unwrap_err();
    assert!(matches!(err, XynokEcsError::StructureChangeNotAllowed(..)));

    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(1));
    assert_eq!(testing::entity_stored_at_row_of(&w, e), e);
    // still occupied, the slot was not handed back
    assert!(w.try_create_singleton((Hp(2), Mana(2))).is_err());
}

#[test]
fn t_remove_component_on_a_singleton_is_rejected()
{
    let mut w = World::default();
    let e = w.create_singleton((Hp(1), Mana(1)));

    let err = w.try_remove_component::<Mana>(e).unwrap_err();
    assert!(matches!(err, XynokEcsError::StructureChangeNotAllowed(..)));
    assert_eq!(testing::read_component::<Mana>(&w, e), Mana(1));
}

#[test]
fn t_merge_that_would_move_a_singleton_is_rejected()
{
    let mut w = World::default();
    let e = w.create_singleton(Hp(1));

    let err = w.try_merge_component(e, (Hp(2), Mana(2))).unwrap_err();
    assert!(matches!(err, XynokEcsError::StructureChangeNotAllowed(..)));
    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(1));
}

#[test]
fn t_merge_that_only_overwrites_a_singleton_is_allowed()
{
    let mut w = World::default();
    let e = w.create_singleton((Hp(1), Mana(1)));

    w.merge_component(e, Hp(9));
    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(9));
}

#[test]
fn t_entity_cannot_move_into_a_singleton_through_add_component()
{
    let mut w = World::default();
    w.register_singleton::<(Hp, Mana)>();

    let e = w.create(Hp(5));
    let err = w.try_add_component(e, Mana(6)).unwrap_err();
    assert!(matches!(err, XynokEcsError::StructureChangeNotAllowed(..)));
    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(5));
}

#[test]
fn t_locked_regular_archetype_rejects_structure_change()
{
    use xynok_ecs::apis::ArchetypeCfg;

    let mut w = World::default();
    w.register_archetype::<Hp>(ArchetypeCfg { allow_structure_change: false, ..CFG });

    let a = w.create(Hp(1));
    let b = w.create(Hp(2));
    assert!(matches!(w.try_add_component(a, Mana(1)).unwrap_err(), XynokEcsError::StructureChangeNotAllowed(..)));

    // a locked archetype is not a singleton, many entities are fine
    assert_eq!(testing::read_component::<Hp>(&w, b), Hp(2));
}

#[test]
fn t_registering_again_with_a_different_structure_change_flag_is_rejected()
{
    use xynok_ecs::apis::ArchetypeCfg;

    let mut w = World::default();
    w.register_archetype::<Hp>(CFG);

    let err = w.try_register_archetype::<Hp>(ArchetypeCfg { allow_structure_change: false, ..CFG }).unwrap_err();
    assert!(matches!(err, XynokEcsError::ArchetypeAlreadyCreatedWithDifferentStructureChange(..)));
}

#[test]
fn t_register_singleton_is_idempotent()
{
    let mut w = World::default();
    w.register_singleton::<Hp>();
    let after_first = testing::archetype_count(&w);
    w.register_singleton::<Hp>();

    assert_eq!(testing::archetype_count(&w), after_first);
}

#[test]
fn t_regular_archetype_cannot_become_a_singleton()
{
    let mut w = World::default();
    w.create(Hp(1));

    let err = w.try_register_singleton::<Hp>().unwrap_err();
    assert!(matches!(err, XynokEcsError::ArchetypeIsNotSingleton(_)));
}

#[test]
fn t_singleton_cannot_be_registered_as_regular_archetype()
{
    let mut w = World::default();
    w.register_singleton::<Hp>();

    let err = w.try_register_archetype::<Hp>(CFG).unwrap_err();
    assert!(matches!(err, XynokEcsError::ArchetypeIsSingleton(_)));
}

#[test]
fn t_singleton_chunk_fits_one_row()
{
    let mut w = World::default();
    let e = w.create_singleton((Hp(1), Aligned32(2)));

    assert_eq!(testing::max_len(&w, e), 1);
    let size = testing::chunk_size_in_byte(&w, e);
    assert!(size < xynok_ecs::apis::constants::DEFAULT_CHUNK_SIZE_IN_BYTE);
    assert_eq!(size % align_of::<Aligned32>(), 0);
}

#[test]
fn t_singleton_is_visible_to_queries()
{
    let mut w = World::default();
    w.create_singleton(Hp(7));
    w.create((Hp(8), Mana(8))); // a regular archetype that also has Hp

    let q = w.create_query::<&Hp>();
    let mut seen: Vec<u32> = q.into_iter().map(|hp| hp.0).collect();
    seen.sort();
    assert_eq!(seen, vec![7, 8]);
}

#[test]
fn t_derived_archetype_name_follows_type_name_format()
{
    let mut w = World::default();
    let e = w.create(Hp(1));
    w.add_component(e, Mana(1));
    assert_eq!(testing::archetype_name(&w, e), std::any::type_name::<(Hp, Mana)>());

    w.remove_component::<Hp>(e);
    assert_eq!(testing::archetype_name(&w, e), std::any::type_name::<Mana>());
}

/// Hand written component that shares `Hp`'s storage but asks for change tracking, which the
/// layout built from `Hp` does not have. Writing it through that layout makes `push` fail.
struct TrackedHp(#[allow(unused)] u32);
impl xynok_ecs::apis::traits::TComponent for TrackedHp
{
    type QueryType = Hp;
    type StorageType = Hp;
    type MutPolicy = xynok_ecs::query::mut_ref::NoTracking;

    const STORAGE_LOCATION: xynok_ecs::apis::identifies::StorageLocation = xynok_ecs::apis::identifies::StorageLocation::Chunk;
    const STATE_DETECTION: xynok_ecs::apis::identifies::StateDetection = xynok_ecs::apis::identifies::StateDetection::ChangeAble;
}

#[test]
fn t_failed_singleton_push_gives_the_entity_slot_back()
{
    let mut w = World::default();
    w.register_singleton::<Hp>();

    let err = w.try_create_singleton(TrackedHp(1)).unwrap_err();
    assert!(matches!(err, XynokEcsError::ComponentStateNotAvailable(_, _)));

    // the slot taken for the failed singleton is back in the free list, so it gets reused
    let e = w.create(Mana(1));
    assert_eq!(e.idx(), 0);
    assert!(w.exists(e));
}
