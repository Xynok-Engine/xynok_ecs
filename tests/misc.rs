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
    w.register_archetype::<Hp>();
    let after_first = testing::archetype_count(&w);
    w.register_archetype::<Hp>();

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
