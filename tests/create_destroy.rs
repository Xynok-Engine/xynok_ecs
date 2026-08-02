//! Integration tests for `World::create` / `exists` / `destroy`.
mod common;

use common::*;
use xynok_ecs::{entity::Entity, world::testing, world::World};

#[test]
fn t_create_returns_distinct_handles()
{
    let mut w = World::default();
    let handles: Vec<Entity> = (0..64).map(|i| w.create(Hp(i))).collect();

    let unique: std::collections::HashSet<Entity> = handles.iter().copied().collect();
    assert_eq!(unique.len(), handles.len(), "create() must never hand out the same handle twice");
}

#[test]
fn t_create_stores_the_component_value()
{
    let mut w = World::default();
    let e = w.create(Pos { x: 1.5, y: -2.5 });
    assert_eq!(testing::read_component::<Pos>(&w, e), Pos { x: 1.5, y: -2.5 });
}

/// The chunk row must carry the owning entity handle, otherwise swap-remove cannot tell
/// the world which entity moved.
#[test]
fn t_create_writes_the_entity_handle_into_its_row()
{
    let mut w = World::default();
    let e0 = w.create(Hp(10));
    let e1 = w.create(Hp(20));

    assert_eq!(testing::entity_stored_at_row_of(&w, e0), e0);
    assert_eq!(testing::entity_stored_at_row_of(&w, e1), e1);
}

#[test]
fn t_exists_tracks_the_entity_lifecycle()
{
    let mut w = World::default();
    let e = w.create(Hp(1));
    assert!(w.exists(e), "a freshly created entity must exist");

    w.destroy(e);
    assert!(!w.exists(e), "a destroyed entity must not exist");
}

#[test]
fn t_exists_rejects_an_unknown_handle()
{
    let mut w = World::default();
    assert!(!w.exists(Entity::new(999, 1).unwrap()), "an index past the entity table must not exist");
    assert!(!w.exists(Entity::NULL), "the null handle must never exist");
}

#[test]
fn t_recycled_slot_gets_a_fresh_version_and_invalidates_the_stale_handle()
{
    let mut w = World::default();
    let old = w.create(Hp(1));
    w.destroy(old);
    let new = w.create(Hp(2));

    assert_eq!(new.idx(), old.idx(), "the freed slot should be reused");
    assert!(new.version() > old.version(), "a reused slot must bump its version");
    assert!(!w.exists(old), "the stale handle must not resolve to the new entity");
    assert!(w.exists(new));
    assert_eq!(testing::read_component::<Hp>(&w, new), Hp(2));
}

#[test]
fn t_destroy_last_row_needs_no_swap()
{
    let mut w = World::default();
    let e0 = w.create(Hp(0));
    let e1 = w.create(Hp(1));

    w.destroy(e1);

    assert!(w.exists(e0));
    assert_eq!(testing::read_component::<Hp>(&w, e0), Hp(0));
    assert_eq!(testing::entity_stored_at_row_of(&w, e0), e0);
}

/// Removing a row from the middle must move the last row into the hole and re-point
/// the moved entity at its new index.
#[test]
fn t_destroy_middle_row_swaps_the_last_row_back()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..5u32).map(|i| w.create(Hp(i))).collect();
    let (e2, e4) = (entities[2], entities[4]);
    let hole = testing::entity_location(&w, e2).idx_in_chunk;

    w.destroy(e2);

    assert_eq!(
        testing::entity_location(&w, e4).idx_in_chunk,
        hole,
        "the last row must land in the hole left by the destroyed entity"
    );
    assert_eq!(testing::entity_stored_at_row_of(&w, e4), e4, "the moved row must still carry its own handle");
    assert_eq!(testing::read_component::<Hp>(&w, e4), Hp(4), "the moved row must keep its component value");

    let live: Vec<Entity> = entities.iter().copied().filter(|&e| e != e2).collect();
    assert_entity_mapping_is_consistent(&w, &live);
    for (i, &e) in entities.iter().enumerate()
    {
        if e == e2
        {
            continue;
        }
        assert_eq!(testing::read_component::<Hp>(&w, e), Hp(i as u32), "{e} lost its value after an unrelated destroy");
    }
}

#[test]
fn t_destroy_every_entity_front_to_back()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..16u32).map(|i| w.create(Hp(i))).collect();

    for (i, &e) in entities.iter().enumerate()
    {
        w.destroy(e);
        let live: Vec<Entity> = entities[i + 1..].to_vec();
        assert_entity_mapping_is_consistent(&w, &live);
        for &survivor in &live
        {
            assert!(w.exists(survivor), "{survivor} must survive the destruction of {e}");
        }
    }
}

#[test]
fn t_destroy_every_entity_back_to_front()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..16u32).map(|i| w.create(Hp(i))).collect();

    for (i, &e) in entities.iter().enumerate().rev()
    {
        w.destroy(e);
        assert_entity_mapping_is_consistent(&w, &entities[..i]);
    }
}
