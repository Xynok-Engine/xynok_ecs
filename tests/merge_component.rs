//! Integration tests for `World::merge_component`.
mod common;

use common::*;
use xynok_ecs::apis::identifies::XynokEcsError;
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

/// Issue #27, same story as `add_component`: a stale handle is rejected in every build profile.
#[test]
#[should_panic(expected = "does not exist")]
fn t_merge_component_with_a_stale_handle_panics()
{
    let mut w = World::default();
    let e = w.create(Hp(1));
    w.destroy(e);
    w.merge_component(e, Hp(9));
}

#[test]
fn t_try_merge_component_reports_a_stale_handle_instead_of_panicking()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    w.destroy(a);
    let b = w.create(Hp(2));
    assert_eq!(a.idx(), b.idx(), "b is expected to recycle a's slot for this test to mean anything");

    let err = w.try_merge_component(a, Hp(9)).expect_err("a is stale");
    assert!(matches!(err, XynokEcsError::EntityDoesNotExist(idx, version) if idx == a.idx() && version == a.version()));
    assert_eq!(testing::read_component::<Hp>(&w, b), Hp(2), "b must be untouched");

    w.try_merge_component(b, Hp(9)).expect("b is alive");
    assert_eq!(testing::read_component::<Hp>(&w, b), Hp(9));
}

// Issue #43: an existing component keeps its state whether or not the merge moves the entity.
mod state_on_merge
{
    use xynok_ecs::component;
    use xynok_ecs::query::detail::Detail;
    use xynok_ecs::query::filter::{Added, Changed};
    use xynok_ecs::world::World;

    #[component(EnableAble, ChangeAble)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Shield(u32);

    #[component]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Armor(u32);

    fn disable_all_shields(w: &mut World)
    {
        for mut s in w.create_query::<Detail<&mut Shield>>()
        {
            s.set_enabled(false);
        }
    }

    fn shield_enabled_bits(w: &mut World) -> Vec<bool>
    {
        w.create_query::<Detail<&Shield>>().into_iter().map(|s| s.enabled()).collect()
    }

    #[test]
    fn t_merge_keeps_the_enable_bit_when_the_entity_moves()
    {
        let mut w = World::default();
        let e = w.create(Shield(1));
        disable_all_shields(&mut w);

        w.merge_component(e, (Shield(5), Armor(1)));

        assert_eq!(shield_enabled_bits(&mut w), vec![false], "a disabled component must stay disabled after a merge that moves the entity");
        let values: Vec<u32> = w.create_query::<&Shield>().into_iter().map(|s| s.0).collect();
        assert_eq!(values, vec![5], "the value must still be overwritten");
    }

    #[test]
    fn t_merge_keeps_the_enable_bit_when_the_entity_stays()
    {
        let mut w = World::default();
        let e = w.create(Shield(1));
        disable_all_shields(&mut w);

        w.merge_component(e, Shield(5));

        assert_eq!(shield_enabled_bits(&mut w), vec![false]);
    }

    #[test]
    fn t_merge_overwrite_counts_as_changed_but_not_added_when_the_entity_moves()
    {
        let mut w = World::default();
        let e = w.create(Shield(1));
        let seen_up_to = w.capture_current_tick();

        w.merge_component(e, (Shield(5), Armor(1)));

        assert_eq!(w.create_query_since::<Added<&Shield>>(seen_up_to).into_iter().count(), 0, "an overwritten component was not just added");
        assert_eq!(w.create_query_since::<Changed<&Shield>>(seen_up_to).into_iter().count(), 1, "overwriting the value is a change");
    }

    #[test]
    fn t_merge_overwrite_counts_as_changed_but_not_added_when_the_entity_stays()
    {
        let mut w = World::default();
        let e = w.create(Shield(1));
        let seen_up_to = w.capture_current_tick();

        w.merge_component(e, Shield(5));

        assert_eq!(w.create_query_since::<Added<&Shield>>(seen_up_to).into_iter().count(), 0);
        assert_eq!(w.create_query_since::<Changed<&Shield>>(seen_up_to).into_iter().count(), 1);
    }

    #[test]
    fn t_merge_seeds_a_new_component_like_a_fresh_insert()
    {
        let mut w = World::default();
        let e = w.create(Armor(1));
        let seen_up_to = w.capture_current_tick();

        w.merge_component(e, (Armor(2), Shield(5)));

        assert_eq!(shield_enabled_bits(&mut w), vec![true], "a new component starts from its default enable value");
        assert_eq!(w.create_query_since::<Added<&Shield>>(seen_up_to).into_iter().count(), 1);
    }
}
