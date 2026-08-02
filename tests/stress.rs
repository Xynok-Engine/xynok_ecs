//! Longer-running integration tests that exercise `World` under randomized or large-scale
//! sequences of operations.
mod common;

use common::*;
use xynok_ecs::{entity::Entity, world::testing, world::World};

#[test]
fn t_stress_interleaved_create_and_destroy()
{
    let mut w = World::default();
    let mut rng = Rng::new(0xC0FFEE);
    // (handle, expected value) for every entity we believe is alive
    let mut live: Vec<(Entity, u32)> = Vec::new();

    for step in 0..4_000u32
    {
        if live.is_empty() || rng.below(100) < 60
        {
            let e = w.create(Hp(step));
            live.push((e, step));
        }
        else
        {
            let victim = rng.below(live.len());
            let (e, _) = live.swap_remove(victim);
            w.destroy(e);
        }

        if step % 250 == 0
        {
            for &(e, expected) in &live
            {
                assert!(w.exists(e), "{e} vanished at step {step}");
                assert_eq!(testing::read_component::<Hp>(&w, e), Hp(expected), "{e} was corrupted at step {step}");
                assert_eq!(testing::entity_stored_at_row_of(&w, e), e, "{e} lost its row mapping at step {step}");
            }
        }
    }

    for &(e, expected) in &live
    {
        assert!(w.exists(e));
        assert_eq!(testing::read_component::<Hp>(&w, e), Hp(expected));
    }
}

#[test]
fn t_stress_add_and_remove_components()
{
    let mut w = World::default();
    let mut rng = Rng::new(0xBADC0DE);

    // (handle, hp, mana if currently attached)
    let mut live: Vec<(Entity, u32, Option<u32>)> = (0..64u32).map(|i| (w.create(Hp(i)), i, None)).collect();

    for step in 0..1_000u32
    {
        let pick = rng.below(live.len());
        match live[pick].2
        {
            None =>
            {
                let mana = 1_000 + step;
                w.add_component(live[pick].0, Mana(mana));
                live[pick].2 = Some(mana);
            }
            Some(expected) =>
            {
                let taken = w.remove_component::<Mana>(live[pick].0);
                assert_eq!(taken, Mana(expected), "remove_component returned the wrong value at step {step}");
                live[pick].2 = None;
            }
        }

        for &(e, hp, mana) in &live
        {
            assert!(w.exists(e), "{e} vanished at step {step}");
            assert_eq!(testing::read_component::<Hp>(&w, e), Hp(hp), "{e} lost its Hp at step {step}");
            assert_eq!(testing::entity_stored_at_row_of(&w, e), e, "{e} lost its row mapping at step {step}");
            if let Some(mana) = mana
            {
                assert_eq!(testing::read_component::<Mana>(&w, e), Mana(mana), "{e} lost its Mana at step {step}");
            }
        }
    }
}

#[test]
fn t_stress_entities_spanning_many_chunks()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = testing::max_len(&w, probe);
    let total = max_len * 3;

    let mut entities = vec![probe];
    for i in 1..total as u32
    {
        entities.push(w.create(Hp(i)));
    }

    for (i, &e) in entities.iter().enumerate()
    {
        assert_eq!(testing::read_component::<Hp>(&w, e), Hp(i as u32), "{e} was corrupted while filling {total} rows");
        assert_eq!(testing::entity_stored_at_row_of(&w, e), e);
    }

    // Destroy every third entity, then re-check the survivors.
    let mut survivors = Vec::new();
    for (i, &e) in entities.iter().enumerate()
    {
        if i % 3 == 0
        {
            w.destroy(e);
        }
        else
        {
            survivors.push((e, i as u32));
        }
    }
    for &(e, expected) in &survivors
    {
        assert!(w.exists(e));
        assert_eq!(testing::read_component::<Hp>(&w, e), Hp(expected));
        assert_eq!(testing::entity_stored_at_row_of(&w, e), e);
    }
}
