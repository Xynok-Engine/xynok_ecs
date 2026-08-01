#![allow(unused)]
use xynok_ecs::{component, world::World};

#[component]
#[derive(Debug, Default)]
struct Hp(usize);
#[component]
#[derive(Debug, Default)]
struct Mana(usize);
#[component]
#[derive(Debug, Default)]
struct MoveSpeed(usize);

fn main()
{
    let mut world = World::default();

    let a = world.create(Hp(10));

    println!("---------------- merge Mana onto an entity that doesn't have it yet: behaves like add_component()");
    world.merge_component(a, Mana(30));
    let mana = world.remove_component::<Mana>(a);
    println!("a's Mana is now {mana:?}");
    world.add_component(a, mana);

    println!("\n---------------- merge Hp again: it's already present, so the old value is overwritten instead of panicking");
    world.merge_component(a, Hp(999));
    let hp = world.remove_component::<Hp>(a);
    println!("a's Hp is now {hp:?}");
    world.add_component(a, hp);

    println!("\n---------------- merge (Hp, MoveSpeed): Hp overlaps and gets overwritten, MoveSpeed is new and gets added");
    world.merge_component(a, (Hp(1), MoveSpeed(7)));
    let (hp, speed) = world.remove_component::<(Hp, MoveSpeed)>(a);
    println!("a ends with {hp:?} and {speed:?}");
}
