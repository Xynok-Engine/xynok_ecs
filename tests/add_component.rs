//! Integration tests for `World::add_component`.
mod common;

use common::*;
use xynok_ecs::{entity::Entity, world::testing, world::World};

#[test]
fn t_add_component_moves_the_entity_to_another_archetype()
{
    let mut w = World::default();
    let e = w.create(Hp(7));
    let before = testing::entity_location(&w, e).arch_id;

    w.add_component(e, Mana(3));

    assert_ne!(testing::entity_location(&w, e).arch_id, before, "gaining a component must move the entity to a new archetype");
}

#[test]
fn t_add_component_preserves_the_existing_values()
{
    let mut w = World::default();
    let e = w.create(Hp(7));

    w.add_component(e, Mana(3));

    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(7), "the pre-existing component must survive the move");
    assert_eq!(testing::read_component::<Mana>(&w, e), Mana(3), "the new component must be readable");
}

#[test]
fn t_add_component_keeps_the_entity_handle_in_its_new_row()
{
    let mut w = World::default();
    let e = w.create(Hp(7));

    w.add_component(e, Mana(3));

    assert_eq!(testing::entity_stored_at_row_of(&w, e), e, "the destination row must carry the entity handle");
}

/// Adding a component to a middle row must move the last row of the source chunk into the hole
/// and re-point the moved entity at its new index.
#[test]
fn t_add_component_repairs_the_mapping_of_the_swapped_entity()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..5u32).map(|i| w.create(Hp(i))).collect();
    let (e1, e4) = (entities[1], entities[4]);
    let hole = testing::entity_location(&w, e1).idx_in_chunk;

    w.add_component(e1, Mana(100));

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
fn t_add_component_leaves_the_other_entities_intact()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..8u32).map(|i| w.create(Hp(i))).collect();

    w.add_component(entities[3], Mana(42));

    for (i, &e) in entities.iter().enumerate()
    {
        assert_eq!(testing::read_component::<Hp>(&w, e), Hp(i as u32), "{e} lost its Hp when an unrelated entity gained a component");
    }
    assert_eq!(testing::read_component::<Mana>(&w, entities[3]), Mana(42));
}

#[test]
fn t_adding_two_components_in_sequence()
{
    let mut w = World::default();
    let e = w.create(Hp(1));

    w.add_component(e, Mana(2));
    w.add_component(e, Pos { x: 3.0, y: 4.0 });

    assert_eq!(testing::read_component::<Hp>(&w, e), Hp(1));
    assert_eq!(testing::read_component::<Mana>(&w, e), Mana(2));
    assert_eq!(testing::read_component::<Pos>(&w, e), Pos { x: 3.0, y: 4.0 });
    assert_eq!(testing::entity_stored_at_row_of(&w, e), e);
}

#[test]
#[should_panic(expected = "already exists")]
fn t_adding_a_component_twice_is_rejected()
{
    let mut w = World::default();
    let e = w.create(Hp(1));
    w.add_component(e, Mana(2));
    w.add_component(e, Mana(3));
}

#[test]
fn t_entities_with_the_same_component_set_share_one_archetype()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    let b = w.create(Hp(2));
    w.add_component(a, Mana(1));
    w.add_component(b, Mana(2));

    assert_eq!(
        testing::entity_location(&w, a).arch_id,
        testing::entity_location(&w, b).arch_id,
        "two entities holding {{Hp, Mana}} must end up in the same archetype"
    );
    assert_eq!(testing::read_component::<Hp>(&w, a), Hp(1));
    assert_eq!(testing::read_component::<Hp>(&w, b), Hp(2));
}
