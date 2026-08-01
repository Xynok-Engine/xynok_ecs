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
    (Hp::default(), Mana::default(), MoveSpeed::default())
}
fn main()
{
    let mut world = World::default();

    let a = world.create(new_hero());
    let b = world.create(new_hero());
    let c = world.create(new_hero());

    println!("a: {}. exist() = {}", a, world.exists(a));
    println!("b: {}. exist() = {}", b, world.exists(b));
    println!("c: {}. exist() = {}", c, world.exists(c));

    println!("\n---------------- destroy a");
    world.destroy(a);
    println!("a: {}. exist() = {}", a, world.exists(a));
    println!("b: {}. exist() = {}", b, world.exists(b));
    println!("c: {}. exist() = {}", c, world.exists(c));

    let a = world.create((Hp::default(), Mana::default(), MoveSpeed::default()));
    println!("\n---------------- create new entity for a, expect a.ver += 1");
    println!("a: {}. exist() = {}", a, world.exists(a));
    println!("b: {}. exist() = {}", b, world.exists(b));
    println!("c: {}. exist() = {}", c, world.exists(c));
}
