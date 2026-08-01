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

fn main()
{
    let mut world = World::default();
    //world.create((Hp::default(), Mana::default(), MoveSpeed::default()));
    println!("Hello ");
}
