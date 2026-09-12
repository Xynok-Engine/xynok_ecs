//! A row's state (enable bit, added tick, changed tick) lives in its own region of the chunk,
//! away from the component value. Every operation that moves a row around has to carry that
//! state along, otherwise the row lands on top of someone else's state and the `Enabled` /
//! `Disabled` / `Added` / `Changed` filters start answering about the wrong entity.

use std::collections::HashSet;

use xynok_ecs::component;
use xynok_ecs::query::filter::{Added, Changed, Disabled, Enabled};
use xynok_ecs::query::mut_ref::Mut;
use xynok_ecs::world::{testing, World};
use xynok_ecs::wrapper::Disable;

#[component(EnableAble, ChangeAble)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Hp(u32);

#[component(EnableAble, ChangeAble)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Mana(u32);

fn enabled_hp(w: &mut World) -> HashSet<u32>
{
    w.create_query::<Enabled<&Hp>>().into_iter().map(|hp| hp.0).collect()
}

fn disabled_hp(w: &mut World) -> HashSet<u32>
{
    w.create_query::<Disabled<&Hp>>().into_iter().map(|hp| hp.0).collect()
}

fn changed_hp(w: &mut World) -> HashSet<u32>
{
    w.create_query::<Changed<&Hp>>().into_iter().map(|hp| hp.0).collect()
}

#[test]
fn t_enable_state_follows_the_row_swapped_in_by_destroy()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    w.create(Hp(2));
    w.create(Disable::new(Hp(3))); // last row, and the one that gets swapped into `a`'s slot

    w.destroy(a);

    assert_eq!(enabled_hp(&mut w), HashSet::from([2]));
    assert_eq!(disabled_hp(&mut w), HashSet::from([3]), "Hp(3) was disabled before the swap and still is");
}

#[test]
fn t_destroying_the_last_row_leaves_the_others_alone()
{
    let mut w = World::default();
    w.create(Hp(1));
    w.create(Disable::new(Hp(2)));
    let c = w.create(Hp(3));

    w.destroy(c); // no swap happens at all

    assert_eq!(enabled_hp(&mut w), HashSet::from([1]));
    assert_eq!(disabled_hp(&mut w), HashSet::from([2]));
}

#[test]
fn t_enable_state_survives_gaining_a_component()
{
    let mut w = World::default();
    let a = w.create(Disable::new(Hp(1)));
    let b = w.create(Hp(2));

    w.add_component(a, Mana(10)); // moves `a` into the (Hp, Mana) archetype
    w.add_component(b, Mana(20));

    assert_eq!(disabled_hp(&mut w), HashSet::from([1]), "Hp stayed disabled through the migration");
    assert_eq!(enabled_hp(&mut w), HashSet::from([2]));
}

#[test]
fn t_enable_state_survives_losing_a_component()
{
    let mut w = World::default();
    let a = w.create((Disable::new(Hp(1)), Mana(10)));

    w.remove_component::<Mana>(a); // moves `a` back into the (Hp,) archetype

    assert_eq!(disabled_hp(&mut w), HashSet::from([1]));
    assert!(enabled_hp(&mut w).is_empty());
}

#[test]
fn t_migration_does_not_disturb_the_row_swapped_in_behind_it()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    w.create(Disable::new(Hp(2))); // last row: moves down into `a`'s slot when `a` leaves

    w.add_component(a, Mana(10));

    assert_eq!(enabled_hp(&mut w), HashSet::from([1]));
    assert_eq!(disabled_hp(&mut w), HashSet::from([2]));
}

/// Enable bits are packed 8 rows to a byte, so a swap has to read and rewrite a single bit
/// rather than copy bytes around. This walks a whole byte's worth of alternating rows to make
/// sure the neighbours in the same byte are left alone.
#[test]
fn t_enable_bits_of_neighbouring_rows_survive_a_swap()
{
    let mut w = World::default();
    let entities: Vec<_> = (0..10u32)
        .map(|i| match i % 2
        {
            0 => w.create(Hp(i)),
            _ => w.create(Disable::new(Hp(i))),
        })
        .collect();

    w.destroy(entities[1]); // disabled row, replaced by Hp(9) which is also disabled
    w.destroy(entities[2]); // enabled row, replaced by Hp(8) which is enabled

    assert_eq!(enabled_hp(&mut w), HashSet::from([0, 4, 6, 8]));
    assert_eq!(disabled_hp(&mut w), HashSet::from([3, 5, 7, 9]));
}

#[test]
fn t_change_detection_state_survives_a_migration()
{
    let mut w = World::default();
    let a = w.create(Hp(1));

    w.merge_component(a, Hp(5)); // bumps `a`'s changed tick
    let before = testing::component_state::<Hp>(&w, a);
    // outside a system run the world's tick never moves, so `0` is the only value that means
    // "never touched" - and the only value a lost tick could decay to
    assert_ne!(before.added_tick, Some(0), "sanity: the insert really was stamped");
    assert_ne!(before.changed_tick, Some(0), "sanity: the write really was stamped");

    w.add_component(a, Mana(10)); // and now `a` moves to another archetype
    let after = testing::component_state::<Hp>(&w, a);

    assert_eq!(after.added_tick, before.added_tick, "the insert is still dated when it happened");
    assert_eq!(after.changed_tick, before.changed_tick, "the write is still visible after the move");
    assert_eq!(after.enabled, before.enabled);
}

/// `Changed` from a plain `create_query` compares against tick `0`, so it reports every row ever
/// written. That is the whole contract outside a system run, and it still has to hold after rows
/// have been shuffled around.
#[test]
fn t_changed_filter_still_reports_written_rows_after_a_swap()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    w.create(Hp(2));

    w.destroy(a);

    assert_eq!(changed_hp(&mut w), HashSet::from([2]));
}

// ------------------------------------------------------------------------------------------------
// Writing through a query
// ------------------------------------------------------------------------------------------------

/// Handing a row out of a `&mut` query is not a write. Only going through `DerefMut` is, which
/// is what keeps `Changed` from reporting everything a read-modify-nothing system walked past.
#[test]
fn t_iterating_a_mut_query_without_writing_does_not_mark_anything_changed()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    let seen_up_to = w.current_tick();
    w.advance_tick();

    let mut total = 0;
    for hp in w.create_query::<&mut Hp>()
    {
        total += hp.0; // read only, through Deref
    }
    assert_eq!(total, 1);

    let changed: Vec<u32> = w
        .create_query_since::<Changed<&Hp>>(seen_up_to)
        .into_iter()
        .map(|hp| hp.0)
        .collect();
    assert!(changed.is_empty(), "nothing was written, so nothing changed");
    assert_eq!(testing::component_state::<Hp>(&w, a).changed_tick, Some(seen_up_to));
}

#[test]
fn t_writing_through_a_mut_query_marks_the_row_changed()
{
    let mut w = World::default();
    w.create(Hp(1));
    w.create(Hp(2));

    let seen_up_to = w.current_tick();
    w.advance_tick();

    for mut hp in w.create_query::<&mut Hp>()
    {
        if hp.0 == 1
        {
            hp.0 = 10; // only this row is written
        }
    }

    let changed: Vec<u32> = w
        .create_query_since::<Changed<&Hp>>(seen_up_to)
        .into_iter()
        .map(|hp| hp.0)
        .collect();
    assert_eq!(changed, vec![10], "only the row that was actually written reports as changed");
}

#[test]
fn t_writing_through_a_filtered_mut_query_marks_the_row_changed()
{
    let mut w = World::default();
    w.create(Hp(1));
    w.create(Disable::new(Hp(2)));

    let seen_up_to = w.current_tick();
    w.advance_tick();

    for mut hp in w.create_query::<Disabled<&mut Hp>>()
    {
        hp.0 += 100;
    }

    let changed: Vec<u32> = w
        .create_query_since::<Changed<&Hp>>(seen_up_to)
        .into_iter()
        .map(|hp| hp.0)
        .collect();
    assert_eq!(changed, vec![102], "the enable filter picked the row, the write still got recorded");
}

#[test]
fn t_writing_through_a_tuple_marks_only_the_written_column()
{
    let mut w = World::default();
    let a = w.create((Hp(1), Mana(1)));

    let seen_up_to = w.current_tick();
    w.advance_tick();

    for (mut hp, _mana) in w.create_query::<(&mut Hp, &mut Mana)>()
    {
        hp.0 = 5; // Mana is handed out mutably but never written
    }

    assert_eq!(
        w.create_query_since::<Changed<&Hp>>(seen_up_to).into_iter().count(),
        1,
        "Hp was written"
    );
    assert_eq!(
        w.create_query_since::<Changed<&Mana>>(seen_up_to).into_iter().count(),
        0,
        "Mana was only read"
    );
    assert_eq!(testing::component_state::<Mana>(&w, a).changed_tick, Some(seen_up_to));
}

#[test]
fn t_bypass_change_detection_writes_without_stamping()
{
    let mut w = World::default();
    w.create(Hp(1));

    let seen_up_to = w.current_tick();
    w.advance_tick();

    for mut hp in w.create_query::<&mut Hp>()
    {
        Mut::bypass_change_detection(&mut hp).0 = 42;
    }

    assert_eq!(w.create_query::<&Hp>().into_iter().next().map(|hp| hp.0), Some(42), "the write landed");
    assert_eq!(
        w.create_query_since::<Changed<&Hp>>(seen_up_to).into_iter().count(),
        0,
        "but it was deliberately not announced"
    );
}

// ------------------------------------------------------------------------------------------------
// Change-detection rounds outside a schedule
// ------------------------------------------------------------------------------------------------

#[test]
fn t_advance_tick_closes_one_round_of_change_detection()
{
    let mut w = World::default();
    let a = w.create(Hp(1));

    // round 1: the spawn itself
    let before_round_1 = 0;
    assert_eq!(w.create_query_since::<Changed<&Hp>>(before_round_1).into_iter().count(), 1);

    let before_round_2 = w.current_tick();
    w.advance_tick();
    assert_eq!(
        w.create_query_since::<Changed<&Hp>>(before_round_2).into_iter().count(),
        0,
        "the spawn belongs to the previous round"
    );

    w.merge_component(a, Hp(2));
    assert_eq!(
        w.create_query_since::<Changed<&Hp>>(before_round_2).into_iter().count(),
        1,
        "the write belongs to this one"
    );
}

#[test]
fn t_added_only_fires_for_the_round_the_row_was_inserted_in()
{
    let mut w = World::default();
    let a = w.create(Hp(1));

    let after_spawn = w.current_tick();
    w.advance_tick();

    w.merge_component(a, Hp(2)); // a write, not an insert
    let b = w.create(Hp(3)); // a real insert, this round

    let added: Vec<u32> = w.create_query_since::<Added<&Hp>>(after_spawn).into_iter().map(|hp| hp.0).collect();
    assert_eq!(added, vec![3], "only {b:?} was inserted this round");
}
