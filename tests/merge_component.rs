//! Integration tests for `World::merge_component`.
mod common;

use common::*;
use xynok_ecs::{entity::Entity, world::testing, world::World};

#[test]
fn t_merge_component_adds_a_missing_component_like_add_component()
{
    let mut w = World::default();
    let e = w.create(Hp(7));
    let before = testing::entity_location(&w, e).arch_id;

    w.merge_component(e, Mana(3));

    assert_ne!(testing::entity_location(&w, e).arch_id, before, "gaining a new component must move the entity to a new archetype");
    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(7), "the pre-existing component must survive the move");
    assert_eq!(testing::read_component::<Mana>(&w, e), Mana(3), "the new component must be readable");
}

#[test]
fn t_merge_component_overwrites_an_existing_component_in_place()
{
    let mut w = World::default();
    let e = w.create(Hp(7));
    let before = testing::entity_location(&w, e).arch_id;

    w.merge_component(e, Hp(99));

    assert_eq!(
        testing::entity_location(&w, e).arch_id,
        before,
        "merging a component that's already fully covered must not move the entity to another archetype"
    );
    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(99), "merge_component must overwrite the existing value");
    assert_eq!(testing::entity_stored_at_row_of(&w, e), e);
}

#[test]
fn t_merge_component_overwrites_overlap_while_adding_new_components()
{
    let mut w = World::default();
    let e = w.create(Hp(7));
    w.add_component(e, Mana(3));

    w.merge_component(e, (Mana(50), Pos { x: 1.0, y: 2.0 }));

    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(7), "the unrelated component must survive the merge");
    assert_eq!(testing::read_component::<Mana>(&w, e), Mana(50), "merge_component must overwrite the overlapping component");
    assert_eq!(testing::read_component::<Pos>(&w, e), Pos { x: 1.0, y: 2.0 }, "the new component must be readable");
    assert_eq!(testing::entity_stored_at_row_of(&w, e), e);
}

#[test]
fn t_merge_component_repairs_the_mapping_of_the_swapped_entity()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..5u32).map(|i| w.create(Hp(i))).collect();
    let (e1, e4) = (entities[1], entities[4]);
    let hole = testing::entity_location(&w, e1).idx_in_chunk;

    w.merge_component(e1, Mana(100));

    assert_eq!(
        testing::entity_location(&w, e4).idx_in_chunk,
        hole,
        "the last row of the source chunk must be re-pointed to the hole"
    );
    assert_eq!(testing::entity_stored_at_row_of(&w, e4), e4);
    assert_eq!(testing::read_component::<Hp>(&w, e4), Hp(4));

    let untouched: Vec<Entity> = vec![entities[0], entities[2], entities[3], e4];
    assert_entity_mapping_is_consistent(&w, &untouched);
}

#[test]
fn t_merge_component_in_place_drops_the_overwritten_value_exactly_once()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let e = w.create(Tracked(1));
    assert_eq!(drop_count(), 0, "storing a value must not drop it");

    w.merge_component(e, Tracked(2));
    assert_eq!(drop_count(), 1, "overwriting an existing component in place must drop the old value exactly once");

    w.destroy(e);
    assert_eq!(drop_count(), 2, "the new value must still be dropped exactly once when the entity is destroyed");
}

#[test]
fn t_merge_component_moving_archetype_drops_the_overwritten_value_exactly_once()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let e = w.create((Tracked(1), Hp(1)));
    assert_eq!(drop_count(), 0, "storing a value must not drop it");

    w.merge_component(e, (Tracked(2), Mana(1)));
    assert_eq!(
        drop_count(),
        1,
        "the component shared with the entity's current archetype must be dropped, not migrated and leaked"
    );
    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(1), "the unrelated component must survive the archetype move");
    assert_eq!(testing::read_component::<Mana>(&w, e), Mana(1));

    w.destroy(e);
    assert_eq!(drop_count(), 2, "the new value must still be dropped exactly once when the entity is destroyed");
}
