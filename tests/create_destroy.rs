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

/// The slot ABA: a handle is `(idx, version)`, and `Entity::new` used to clamp the version
/// instead of refusing it. The 2^24-th reuse of a slot therefore reissued the exact handle it
/// had just retired, and every stale copy of that handle went back to passing `exists` while
/// addressing whoever lives in the slot now.
#[test]
fn a_slot_out_of_versions_is_retired_instead_of_reissuing_a_handle()
{
    let mut w = World::default();
    let e = w.create(Hp(1));

    // pretend this slot has been through every version it will ever get
    let exhausted = testing::force_entity_version(&mut w, e, Entity::MAX_VERSION);
    assert!(w.exists(exhausted));
    assert_eq!(w.retired_entity_slot_count(), 0, "nothing is retired while the slot is still live");

    w.destroy(exhausted);
    assert_eq!(w.retired_entity_slot_count(), 1, "a slot with no version left must not go back into circulation");

    let next = w.create(Hp(2));
    assert_ne!(next, exhausted, "the retired handle must never be handed out a second time");
    assert_ne!(next.idx(), exhausted.idx(), "the retired slot must not be reused at all");
    assert!(!w.exists(exhausted), "the stale handle must stay dead");
    assert!(w.exists(next));
}

/// The ordinary case has to keep working exactly as before: a slot with versions left is
/// recycled, and the handle it gives back is distinguishable from the previous one.
#[test]
fn a_slot_with_versions_left_is_still_recycled()
{
    let mut w = World::default();
    let first = w.create(Hp(1));
    w.destroy(first);

    let second = w.create(Hp(2));
    assert_eq!(second.idx(), first.idx(), "a healthy slot is reused");
    assert_eq!(second.version(), first.version() + 1, "and its version moves on");
    assert!(!w.exists(first), "the old handle is dead");
    assert!(w.exists(second));
    assert_eq!(w.retired_entity_slot_count(), 0);
}

/// One slot running out must not drag the rest of the table down with it.
#[test]
fn retiring_one_slot_leaves_the_others_recyclable()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    let b = w.create(Hp(2));

    let a_exhausted = testing::force_entity_version(&mut w, a, Entity::MAX_VERSION);
    w.destroy(a_exhausted);
    w.destroy(b);

    let reused = w.create(Hp(3));
    assert_eq!(reused.idx(), b.idx(), "b's slot still had versions to spend");
    assert_eq!(w.retired_entity_slot_count(), 1, "only a's slot was retired");

    let fresh = w.create(Hp(4));
    assert!(
        fresh.idx() != a.idx() && fresh.idx() != b.idx(),
        "with the free list empty the next entity takes a brand new slot"
    );
}
