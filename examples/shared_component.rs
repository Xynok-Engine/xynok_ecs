use xynok_ecs::component;
use xynok_ecs::shared::TSharedComponent;
use xynok_ecs::world::World;

#[component]
struct Hp(u32);

#[derive(Clone)]
struct Mesh
{
    id:       u32,
    vertices: Vec<f32>,
}
impl TSharedComponent for Mesh
{
    type Key = u32;
    fn shared_key(&self) -> u32
    {
        self.id
    }
}

fn main()
{
    let mut world = World::default();
    let a = world.create(Hp(100));
    let b = world.create(Hp(80));
    world
        .add_shared_component(
            a,
            Mesh {
                id:       7,
                vertices: vec![0.0, 1.0, 2.0],
            },
        )
        .unwrap();
    // Mesh 7 already exists next to Hp, so b joins it by key
    world.add_shared_component_by_key::<Mesh>(b, 7).unwrap();

    let mut query = world.create_shared_query::<&Hp, &mut Mesh>();
    for (mut mesh, mut archetype) in query.iter_archetype()
    {
        mesh.vertices.push(3.0);
        println!("Mesh {}: {:?}", mesh.id, mesh.vertices);
        for chunk in archetype.iter_chunk()
        {
            for hp in chunk
            {
                println!("Entity HP: {}", hp.0);
            }
        }
    }

    // Query borrows end before structural changes. Moving to a key nobody holds yet takes a
    // remove, then an add with the new value.
    world.remove_shared_component::<Mesh>(a).unwrap();
    world
        .add_shared_component(
            a,
            Mesh {
                id:       8,
                vertices: vec![4.0, 5.0],
            },
        )
        .unwrap();
    println!("a now uses mesh {}", world.get_shared_component::<Mesh>(a).unwrap().id);
}
