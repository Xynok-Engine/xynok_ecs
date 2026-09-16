//! `Detail<Q>` hands out a component next to its enable bit. These tests check that the bit it
//! reports is the one filters see, that flipping it sticks without counting as a change, and that
//! batches flipping bits on their own threads never step on each other.

use xynok_ecs::component;
use xynok_ecs::query::detail::Detail;
use xynok_ecs::query::filter::{Changed, Disabled, Enabled};
use xynok_ecs::world::World;
use xynok_ecs::wrapper::Disable;

#[component(EnableAble, ChangeAble)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mesh(u32);

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hp(u32);

// several chunks worth of rows
const COUNT: u32 = 5_000;

/// Even meshes start enabled, odd ones disabled
fn world_with_meshes() -> World
{
    let mut w = World::default();
    for i in 0..COUNT
    {
        if i % 2 == 0
        {
            w.create((Hp(i), Mesh(i)));
        }
        else
        {
            w.create((Hp(i), Disable::new(Mesh(i))));
        }
    }
    w
}

#[test]
fn t_detail_keeps_every_row_and_reports_its_bit()
{
    let mut w = world_with_meshes();

    let mut count = 0;
    for mesh in w.create_query::<Detail<&Mesh>>()
    {
        assert_eq!(mesh.enabled(), mesh.0 % 2 == 0);
        assert_eq!(mesh.value().0, mesh.0);
        count += 1;
    }
    assert_eq!(count, COUNT);
}

#[test]
fn t_set_enabled_is_what_filters_see_and_is_not_a_change()
{
    let mut w = world_with_meshes();
    let seen_up_to = w.capture_current_tick();

    // flip every bit
    for (_hp, mut mesh) in w.create_query::<(&Hp, Detail<&mut Mesh>)>()
    {
        let enabled = mesh.enabled();
        mesh.set_enabled(!enabled);
        assert_eq!(mesh.enabled(), !enabled);
    }

    let enabled: Vec<u32> = w.create_query::<Enabled<&Mesh>>().into_iter().map(|m| m.0).collect();
    assert_eq!(enabled.len() as u32, COUNT / 2);
    assert!(enabled.iter().all(|i| i % 2 == 1));
    assert_eq!(w.create_query::<Disabled<&Mesh>>().into_iter().count() as u32, COUNT / 2);

    assert_eq!(w.create_query_since::<Changed<&Mesh>>(seen_up_to).into_iter().count(), 0);
}

#[test]
fn t_writing_through_detail_still_stamps_changed()
{
    let mut w = world_with_meshes();
    let seen_up_to = w.capture_current_tick();

    for mut mesh in w.create_query::<Detail<&mut Mesh>>()
    {
        if mesh.0 < 10
        {
            mesh.0 += 1_000_000;
        }
        else if mesh.0 < 20
        {
            mesh.value_mut().0 += 1_000_000;
        }
    }

    let changed = w.create_query_since::<Changed<&Mesh>>(seen_up_to).into_iter().count();
    assert_eq!(changed, 20);
}

#[test]
fn t_detail_under_a_filter_only_sees_matching_rows()
{
    let mut w = world_with_meshes();
    let seen_up_to = w.capture_current_tick();

    for mut mesh in w.create_query::<Detail<&mut Mesh>>()
    {
        if mesh.0 % 3 == 0
        {
            mesh.0 += 0;
        }
    }

    let mut query = w.create_query_since::<(Changed<&Mesh>, Detail<&Mesh>)>(seen_up_to);
    for (_, mesh) in query.iter()
    {
        assert_eq!(mesh.0 % 3, 0);
    }
}

/// Every batch sets the bit of its own rows from its own thread. With batch sizes that are not a
/// multiple of 8 the cuts would land inside a byte of the enable region if they were not snapped,
/// and a lost write would show up as a wrong bit here.
#[test]
fn t_batches_flip_bits_on_their_own_threads()
{
    for batch_amount in [8, 13, 777]
    {
        let mut w = world_with_meshes();

        for _ in 0..20
        {
            let mut query = w.create_query::<Detail<&mut Mesh>>();
            std::thread::scope(|scope| {
                for batch in query.iter_batch(batch_amount)
                {
                    scope.spawn(move || {
                        for mut mesh in batch
                        {
                            let enabled = mesh.enabled();
                            mesh.set_enabled(!enabled);
                        }
                    });
                }
            });
        }

        // an even number of flips, so every bit is back where it started
        for mesh in w.create_query::<Detail<&Mesh>>()
        {
            assert_eq!(mesh.enabled(), mesh.0 % 2 == 0, "batch_amount {batch_amount}, mesh {}", mesh.0);
        }
    }
}
