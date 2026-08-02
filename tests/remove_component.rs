//! Integration tests for `World::remove_component`.
mod common;

use common::*;
use xynok_ecs::{world::testing, world::World};

#[test]
fn t_remove_component_returns_the_stored_value()
{
    let mut w = World::default();
    let e = w.create(Hp(9));
    w.add_component(e, Mana(77));

    let taken: Mana = w.remove_component::<Mana>(e);

    assert_eq!(taken, Mana(77), "remove_component must hand back the value that was stored");
}

#[test]
fn t_remove_component_keeps_the_remaining_values()
{
    let mut w = World::default();
    let e = w.create(Hp(9));
    w.add_component(e, Mana(77));

    let _ = w.remove_component::<Mana>(e);

    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(9), "the surviving component must keep its value");
    assert_eq!(testing::entity_stored_at_row_of(&w, e), e);
    assert!(w.exists(e));
}

#[test]
fn t_add_then_remove_returns_to_the_original_archetype()
{
    let mut w = World::default();
    let e = w.create(Hp(5));
    let original = testing::entity_location(&w, e).arch_id;

    w.add_component(e, Mana(6));
    let _ = w.remove_component::<Mana>(e);

    assert_eq!(
        testing::entity_location(&w, e).arch_id,
        original,
        "{{Hp, Mana}} minus Mana must be the original {{Hp}} archetype"
    );
    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(5));
}

#[test]
fn t_remove_component_from_the_middle_of_a_chunk()
{
    let mut w = World::default();
    let mut entities = Vec::new();
    for i in 0..5u32
    {
        let e = w.create(Hp(i));
        w.add_component(e, Mana(100 + i));
        entities.push(e);
    }

    let taken = w.remove_component::<Mana>(entities[2]);

    assert_eq!(taken, Mana(102));
    assert_eq!(testing::read_component::<Hp>(&w, entities[2]), Hp(2));
}

/// The entity swapped into the hole left by a removed component must keep its own remaining
/// values, not inherit stale data from the row that left.
#[test]
fn t_remove_component_compacts_the_dropped_column_in_the_source_chunk()
{
    let mut w = World::default();
    let mut entities = Vec::new();
    for i in 0..5u32
    {
        let e = w.create(Hp(i));
        w.add_component(e, Mana(100 + i));
        entities.push(e);
    }
    let last = entities[4];

    let _ = w.remove_component::<Mana>(entities[2]);

    assert_eq!(
        testing::read_component::<Mana>(&w, last),
        Mana(104),
        "the entity swapped into the hole must keep its own Mana, not inherit the removed row's"
    );
    assert_eq!(testing::read_component::<Hp>(&w, last), Hp(4));
}

#[test]
fn t_remove_component_repairs_the_mapping_of_the_swapped_entity()
{
    let mut w = World::default();
    let mut entities = Vec::new();
    for i in 0..5u32
    {
        let e = w.create(Hp(i));
        w.add_component(e, Mana(100 + i));
        entities.push(e);
    }
    let hole = testing::entity_location(&w, entities[1]).idx_in_chunk;

    let _ = w.remove_component::<Mana>(entities[1]);

    let moved = entities[4];
    assert_eq!(testing::entity_location(&w, moved).idx_in_chunk, hole);
    assert_eq!(testing::entity_stored_at_row_of(&w, moved), moved);
    assert_entity_mapping_is_consistent(&w, &[entities[0], entities[2], entities[3], moved]);
}

#[test]
fn t_removing_the_last_component_is_unsupported()
{
    let mut w = World::default();
    let e = w.create(Hp(1));
    let _ = w.remove_component::<Hp>(e);
}
