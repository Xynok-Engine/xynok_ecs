#![allow(unused)]
use xynok_ecs::component;
use xynok_ecs::world::World;

#[component]
#[derive(Debug, Default)]
struct Hp(usize);
#[component]
#[derive(Debug, Default)]
struct Mana(usize);

fn main()
{
    let mut world = World::default();

    let a = world.create((Hp(100), Mana(10)));
    let b = world.create((Hp(80), Mana(5)));
    let c = world.create(Hp(50)); // no Mana: lives in a different archetype from a and b

    println!("---------------- query &Hp: reads every entity carrying Hp, across every archetype that has it");
    let hp_query = world.create_query::<&Hp>();
    for hp in hp_query
    {
        println!("hp: {hp:?}");
    }

    println!("\n---------------- query (&Hp, &Mana): only entities carrying both show up, c is skipped");
    let hp_mana_query = world.create_query::<(&Hp, &Mana)>();
    for (hp, mana) in hp_mana_query
    {
        println!("hp: {hp:?}, mana: {mana:?}");
    }

    println!("\n---------------- query &mut Hp: mutate every Hp in place through the iterator");
    let hp_mut_query = world.create_query::<&mut Hp>();
    for hp in hp_mut_query
    {
        hp.0 += 1;
    }
    let hp_query = world.create_query::<&Hp>();
    for hp in hp_query
    {
        println!("hp after +1: {hp:?}");
    }

    println!("\n---------------- query (&mut Hp, &Mana): combine a write and a read in one pass");
    let combined_query = world.create_query::<(&mut Hp, &Mana)>();
    for (hp, mana) in combined_query
    {
        hp.0 += mana.0;
    }
    let hp_query = world.create_query::<&Hp>();
    for hp in hp_query
    {
        println!("hp after += mana: {hp:?}");
    }
}
