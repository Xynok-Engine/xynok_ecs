#![allow(unused)]
use xynok_ecs::{component, world::World};

#[component]
#[derive(Default)]
struct Hp(usize);
#[component]
#[derive(Default)]
struct Mana(usize);
#[component]
#[derive(Default)]
struct MoveSpeed(usize);

fn new_hero() -> (Hp, Mana, MoveSpeed)
{
    (Hp(10), Mana::default(), MoveSpeed::default())
}
fn main()
{
    let mut world = World::default();

    let a = world.create(new_hero());
    let mut hp = world.remove_component::<Hp>(a);
    println!("remove hp with current val: {}", hp.0);
    hp.0 = 100;
    println!("increase hp to 100 then add again");
    world.add_component(a, hp);
    let mut hp = world.remove_component::<Hp>(a);
    println!("remove hp with current val: {}", hp.0);
    let _ = world.remove_component::<(Mana, MoveSpeed)>(a);
}
