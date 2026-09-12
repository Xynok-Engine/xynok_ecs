#![allow(unused)]
//! Demonstrates combining a query state filter (`Added`/`Changed`/`Enabled`/`Disabled`, from
//! `xynok_ecs::query::filter`) with a plain column inside one tuple query, and the two spawn
//! wrappers (`xynok_ecs::wrapper::Enable`/`Disable`) that control an entity's enable bit at
//! insert time.

use xynok_ecs::component;
use xynok_ecs::query::filter::{Disabled, Enabled};
use xynok_ecs::world::World;
use xynok_ecs::wrapper::{Disable, Enable};

#[component(EnableAble)]
#[derive(Debug, Default)]
struct Hp(i32);
#[component]
#[derive(Debug, Default)]
struct Mana(i32);

fn main()
{
    let mut world = World::default();

    // `Enable::new`/`Disable::new` set the wrapped component's enable bit at insert time,
    // overriding `TComponent::ENABLE_VALUE`'s default (`true`) for that one spawn.
    let hero = world.create((Enable::new(Hp(100)), Mana(10))); // explicitly enabled
    let ghost = world.create((Disable::new(Hp(0)), Mana(0))); // spawned already disabled
    let mage = world.create((Hp(80), Mana(40))); // no wrapper: falls back to ENABLE_VALUE = true

    // A tuple query can mix a plain column (`&Mana`) with a state filter (`Enabled<&Hp>`): the
    // row is only yielded when every element in the tuple accepts it, so this reads `Mana` only
    // for the entities whose `Hp` is currently enabled.
    println!("---------------- Query<(&Mana, Enabled<&Hp>)>: only hero and mage show up, ghost is skipped");
    for (mana, hp) in world.create_query::<(&Mana, Enabled<&Hp>)>()
    //for (mana, hp) in world.create_query::<(Enabled<&Mana>, Enabled<&Hp>)>()
    {
        println!("  mana: {mana:?}, hp: {hp:?}");
    }

    println!("\n---------------- Query<(&Mana, Disabled<&Hp>)>: only ghost shows up");
    for (mana, hp) in world.create_query::<(&Mana, Disabled<&Hp>)>()
    {
        println!("  mana: {mana:?}, hp: {hp:?}");
    }

    // `Disabled<&mut Hp>` still filters on the enable bit, but hands out a mutable reference -
    // useful for e.g. reviving a disabled entity's stats before re-enabling it elsewhere.
    println!("\n---------------- Query<Disabled<&mut Hp>>: heal every disabled Hp in place");
    for mut hp in world.create_query::<Disabled<&mut Hp>>()
    {
        hp.0 = 50;
    }
    for (mana, hp) in world.create_query::<(&Mana, Disabled<&Hp>)>()
    {
        println!("  mana: {mana:?}, hp after heal: {hp:?}");
    }
}
