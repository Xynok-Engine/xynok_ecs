//! Integration tests verifying `World` runs each component's drop glue exactly once, whatever
//! path an entity took to get there (destroy, remove_component, or the world itself dropping).
mod common;

use common::*;
use xynok_ecs::{entity::Entity, world::World};

#[test]
fn t_destroying_an_entity_drops_its_components_exactly_once()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let e = w.create(Tracked(1));
    assert_eq!(drop_count(), 0, "storing a value must not drop it");

    w.destroy(e);

    assert_eq!(drop_count(), 1, "destroy() must run the drop glue exactly once");
}

#[test]
fn t_removing_a_component_moves_it_out_without_dropping_it()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let e = w.create(Tracked(1));
    w.add_component(e, Hp(1));

    let taken = w.remove_component::<Tracked>(e);
    assert_eq!(drop_count(), 0, "the value was moved out, it must not have been dropped yet");

    drop(taken);
    assert_eq!(drop_count(), 1, "the moved-out value must drop exactly once, not twice");
}

#[test]
fn t_destroying_many_entities_drops_each_component_once()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let entities: Vec<Entity> = (0..32u32).map(|i| w.create(Tracked(i))).collect();
    for e in entities
    {
        w.destroy(e);
    }

    assert_eq!(drop_count(), 32, "each stored component must be dropped exactly once");
}

#[test]
fn t_dropping_the_world_drops_the_components_it_still_owns()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    {
        let mut w = World::default();
        for i in 0..8u32
        {
            w.create(Tracked(i));
        }
    }

    assert_eq!(drop_count(), 8, "dropping the world must drop the components it still owns");
}
