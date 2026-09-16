use xynok_ecs::component;
use xynok_ecs::world::World;

#[component]
#[derive(Debug, Default)]
struct Position(f32);
#[component]
#[derive(Debug, Default)]
struct Velocity(f32);
#[component]
#[derive(Debug, Default)]
struct Tag;

fn main()
{
    let mut world = World::default();

    // two archetypes: (Position, Velocity) and (Position, Velocity, Tag)
    for i in 0..10
    {
        world.create((Position(0.0), Velocity(i as f32)));
    }
    for i in 0..5
    {
        world.create((Position(0.0), Velocity(100.0 + i as f32), Tag));
    }

    let mut query = world.create_query::<(&mut Position, &Velocity)>();

    println!("---------------- iter: one entity at a time, across every chunk and archetype");
    for (pos, vel) in query.iter()
    {
        pos.0 += vel.0;
    }

    println!("\n---------------- iter_chunk: one chunk at a time, with its row count known up front");
    for (i, mut chunk) in query.iter_chunk().enumerate()
    {
        println!("chunk {i}: {} rows", chunk.len());
        // `chunk.iter()` borrows the chunk, `for item in chunk` takes it by value
        for (pos, vel) in chunk.iter()
        {
            pos.0 += vel.0;
        }
    }

    println!("\n---------------- iter_batch: batches of 4 rows, a batch can span several chunks and archetypes");
    for (i, batch) in query.iter_batch(4).enumerate()
    {
        println!("batch {i}: {} rows", batch.len());
    }

    println!("\n---------------- iter_batch + thread: every batch is Send, so each one can run on its own thread");
    std::thread::scope(|scope| {
        for batch in query.iter_batch(4)
        {
            scope.spawn(move || {
                for (pos, vel) in batch
                {
                    pos.0 += vel.0;
                }
            });
        }
    });

    println!("\n---------------- result: every Position got its Velocity added 3 times");
    let read_query = world.create_query::<&Position>();
    for pos in read_query
    {
        println!("{pos:?}");
    }
}
