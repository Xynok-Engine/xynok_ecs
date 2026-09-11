use xynok_ecs::apis::identifies::XynokEcsError;
mod common;
use common::{Hp, Mana};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use xynok_ecs::entity::Entity;
use xynok_ecs::query::Query;
use xynok_ecs::shared::*;
use xynok_ecs::world::{World, testing};

#[derive(Clone, Debug, PartialEq)]
struct Mesh
{
    id:       u32,
    vertices: Vec<u32>,
}
impl TSharedComponent for Mesh
{
    type Key = u32;
    fn shared_key(&self) -> u32
    {
        self.id
    }
}
fn mesh(id: u32) -> Mesh
{
    Mesh {
        id:       id,
        vertices: vec![id],
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Material
{
    id:   u32,
    name: String,
}
impl TSharedComponent for Material
{
    type Key = u32;
    fn shared_key(&self) -> u32
    {
        self.id
    }
}
fn material(id: u32, name: &str) -> Material
{
    Material {
        id:   id,
        name: name.into(),
    }
}
struct Grass;
static FILTER_CALLS: AtomicUsize = AtomicUsize::new(0);
impl TSharedFilter for Grass
{
    type SharedComponents = Mesh;
    fn filter_key() -> u32
    {
        FILTER_CALLS.fetch_add(1, Ordering::SeqCst);
        7
    }
}
/// Gives `e` the mesh `id`, joining the archetype that already holds it when there is one
fn add_mesh(w: &mut World, e: Entity, id: u32)
{
    match w.add_shared_component(e, mesh(id))
    {
        Err(XynokEcsError::SharedDestinationExists) => w.add_shared_component_by_key::<Mesh>(e, id).unwrap(),
        r => r.unwrap(),
    }
}
fn spawn(w: &mut World, hp: u32, id: u32) -> Entity
{
    let e = w.create(Hp(hp));
    add_mesh(w, e, id);
    e
}
#[test]
fn shares_one_value_across_chunks_and_mutation_keeps_identity()
{
    let mut w = World::default();
    let first = spawn(&mut w, 0, 7);
    let count = testing::max_len(&w, first) * 2 + 3;
    for i in 1..count
    {
        spawn(&mut w, i as u32, 7);
    }
    let mut q = w.create_shared_query::<&Hp, &mut Mesh>();
    assert_eq!(q.iter().count(), count);
    let mut arches = 0;
    for (mut mesh, mut arch) in q.iter_archetype()
    {
        arches += 1;
        assert_eq!(mesh.id, 7);
        mesh.vertices.push(42);
        let mut rows = 0;
        for chunk in arch.iter_chunk()
        {
            rows += chunk.into_iter().count();
        }
        assert_eq!(rows, count);
    }
    assert_eq!(arches, 1);
    let mut filtered = q.filter_shared::<Mesh>(&7);
    assert_eq!(filtered.iter().count(), count);
    assert_eq!(filtered.iter_archetype().next().unwrap().0.vertices, [7, 42]);
}
#[test]
fn structural_moves_preserve_other_keys_and_clone_values_only_for_new_archetypes()
{
    let mut w = World::default();
    let a = spawn(&mut w, 1, 7);
    let b = spawn(&mut w, 2, 7);
    w.add_shared_component(a, material(4, "grass")).unwrap();
    assert_eq!(
        w.add_shared_component(b, material(4, "ignored")),
        Err(XynokEcsError::SharedDestinationExists)
    );
    w.add_shared_component_by_key::<Material>(b, 4).unwrap();
    w.add_component(a, Mana(3));
    {
        let mut q = w.create_shared_query::<(&Hp, &Mana), (&mut Mesh, &Material)>();
        let ((mut mesh, material), _) = q.iter_archetype().next().unwrap();
        mesh.vertices.push(99);
        assert_eq!(material.name, "grass");
    }
    // Returning to an existing archetype uses its value, not the modified clone.
    assert_eq!(w.remove_component::<Mana>(a).0, 3);
    let mut q = w.create_shared_query::<&Hp, (&Mesh, &Material)>();
    let ((mesh, material), _) = q.iter_archetype().next().unwrap();
    assert_eq!(mesh.vertices, [7]);
    assert_eq!(material.id, 4);
    assert_eq!(q.iter().count(), 2);
}
#[test]
fn set_key_requires_full_destination_and_does_not_rename_other_entities()
{
    let mut w = World::default();
    let a = spawn(&mut w, 1, 7);
    let b = spawn(&mut w, 2, 7);
    let c = spawn(&mut w, 3, 8);
    w.add_component(c, Mana(1));
    let d = w.create(Hp(4));
    assert_eq!(w.set_shared_component_by_key::<Mesh>(d, 7), Err(XynokEcsError::SharedComponentDoesNotExist));
    // Mesh(8) only exists next to Mana, which is another layout
    assert_eq!(w.set_shared_component_by_key::<Mesh>(a, 8), Err(XynokEcsError::UnresolvedSharedKey));
    assert_eq!(w.set_shared_component_by_key::<Mesh>(a, 7), Ok(()));
    w.remove_shared_component::<Mesh>(a).unwrap();
    w.add_shared_component(
        a,
        Mesh {
            id:       8,
            vertices: vec![80],
        },
    )
    .unwrap();
    w.set_shared_component_by_key::<Mesh>(b, 8).unwrap();
    let mut q = w.create_shared_query::<&Hp, &Mesh>();
    assert_eq!(q.filter_shared::<Mesh>(&7).iter().count(), 0);
    let mut groups = q.group_by_shared::<Mesh>();
    let (key, mut group) = groups.next().unwrap();
    assert_eq!(key, 8);
    assert!(groups.next().is_none());
    let mut values = group.iter_archetype().map(|(mesh, _)| mesh.vertices.clone()).collect::<Vec<_>>();
    values.sort();
    assert_eq!(values, vec![vec![8], vec![80]]);
    assert_eq!(group.iter().count(), 3);
}
#[test]
fn add_reports_an_existing_destination_and_by_key_joins_it()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    let b = w.create(Hp(2));
    assert!(!w.has_shared_component::<Mesh>(a));
    assert_eq!(w.add_shared_component_by_key::<Mesh>(a, 7), Err(XynokEcsError::UnresolvedSharedKey));
    w.add_shared_component(a, mesh(7)).unwrap();
    assert!(w.has_shared_component::<Mesh>(a));
    assert_eq!(w.add_shared_component(a, mesh(8)), Err(XynokEcsError::SharedComponentAlreadyExists));
    assert_eq!(w.add_shared_component_by_key::<Mesh>(a, 7), Err(XynokEcsError::SharedComponentAlreadyExists));

    let other = Mesh {
        id:       7,
        vertices: vec![70],
    };
    assert_eq!(w.add_shared_component(b, other.clone()), Err(XynokEcsError::SharedDestinationExists));
    assert!(!w.has_shared_component::<Mesh>(b), "a failed add leaves the entity where it was");
    w.add_shared_component_by_key::<Mesh>(b, 7).unwrap();
    assert_eq!(w.get_shared_component::<Mesh>(b), Some(&mesh(7)));

    // the same key next to other normal components is another archetype, so it takes a new value
    let c = w.create(Hp(3));
    w.add_component(c, Mana(3));
    w.add_shared_component(c, other.clone()).unwrap();
    assert_eq!(w.get_shared_component::<Mesh>(c), Some(&other));
    assert_eq!(w.get_shared_component::<Mesh>(a), Some(&mesh(7)));

    w.destroy(b);
    assert_eq!(w.add_shared_component(b, mesh(9)), Err(XynokEcsError::EntityDoesNotExist));
    assert_eq!(w.remove_shared_component::<Mesh>(b), Err(XynokEcsError::EntityDoesNotExist));
    assert_eq!(w.set_shared_component(b, mesh(7)), Err(XynokEcsError::EntityDoesNotExist));
    assert_eq!(w.get_shared_component::<Mesh>(b), None);
    assert!(!w.has_shared_component::<Mesh>(b));
}
#[test]
fn set_checks_the_key_and_writes_the_whole_archetype()
{
    let mut w = World::default();
    let a = spawn(&mut w, 1, 7);
    let b = spawn(&mut w, 2, 7);
    let c = spawn(&mut w, 3, 7);
    w.add_component(c, Mana(1));
    let d = w.create(Hp(4));

    assert_eq!(w.set_shared_component(d, mesh(7)), Err(XynokEcsError::SharedComponentDoesNotExist));
    assert_eq!(w.override_shared_component(d, mesh(7)), Err(XynokEcsError::SharedComponentDoesNotExist));
    assert_eq!(w.set_shared_component(a, mesh(8)), Err(XynokEcsError::SharedKeyMismatch));
    assert_eq!(w.get_shared_component::<Mesh>(a), Some(&mesh(7)), "a rejected value is not written");

    let edited = Mesh {
        id:       7,
        vertices: vec![1, 2, 3],
    };
    w.set_shared_component(a, edited.clone()).unwrap();
    assert_eq!(w.get_shared_component::<Mesh>(b), Some(&edited), "b shares a's archetype");
    assert_eq!(
        w.get_shared_component::<Mesh>(c),
        Some(&mesh(7)),
        "c has other components, so its archetype keeps its own value"
    );
}
#[test]
fn override_skips_the_key_check_and_lookups_keep_the_original_key()
{
    let mut w = World::default();
    let a = spawn(&mut w, 1, 7);
    let b = w.create(Hp(2));
    let renamed = Mesh {
        id:       9,
        vertices: vec![9],
    };
    w.override_shared_component(a, renamed.clone()).unwrap();
    assert_eq!(w.get_shared_component::<Mesh>(a), Some(&renamed));

    // the archetype is still found through the key it was created with
    w.add_shared_component_by_key::<Mesh>(b, 7).unwrap();
    assert_eq!(w.get_shared_component::<Mesh>(b), Some(&renamed));
    assert_eq!(w.create_shared_query::<&Hp, &Mesh>().filter_shared::<Mesh>(&9).iter().count(), 0);
    assert_eq!(w.set_shared_component(a, mesh(9)), Err(XynokEcsError::SharedKeyMismatch));
    w.set_shared_component(a, mesh(7)).unwrap();
    assert_eq!(w.get_shared_component::<Mesh>(b), Some(&mesh(7)));
}
#[test]
#[should_panic(expected = "changed its key through a query")]
fn changing_the_key_through_a_query_panics()
{
    let mut w = World::default();
    spawn(&mut w, 1, 7);
    let mut q = w.create_shared_query::<&Hp, &mut Mesh>();
    for (mut mesh, _) in q.iter_archetype()
    {
        mesh.id = 8;
    }
}
#[test]
fn the_query_view_checks_the_key_only_when_it_ends()
{
    let mut w = World::default();
    let e = spawn(&mut w, 1, 7);
    {
        let mut q = w.create_shared_query::<&Hp, &mut Mesh>();
        for (mut mesh, _) in q.iter_archetype()
        {
            mesh.id = 8;
            mesh.vertices.push(8);
            mesh.id = 7;
        }
        // a panic that is already unwinding is reported as it is, not turned into an abort
        let failed = catch_unwind(AssertUnwindSafe(|| {
            let (mut mesh, _) = q.iter_archetype().next().unwrap();
            mesh.id = 8;
            panic!("system failed");
        }));
        assert_eq!(*failed.unwrap_err().downcast::<&str>().unwrap(), "system failed");
    }
    assert_eq!(w.get_shared_component::<Mesh>(e).unwrap().vertices, [7, 8]);
}
#[test]
fn static_runtime_filters_and_cache_refresh_after_empty_archetype()
{
    let mut w = World::default();
    assert_eq!(w.create_filtered_query::<&Hp, &Mesh, Grass>().iter().count(), 0);
    let e = spawn(&mut w, 1, 7);
    spawn(&mut w, 2, 8);
    assert_eq!(w.create_filtered_query::<&Hp, &Mesh, Grass>().iter().count(), 1);
    w.destroy(e);
    assert_eq!(w.create_filtered_query::<&Hp, &Mesh, Grass>().iter().count(), 0);
    spawn(&mut w, 3, 7);
    assert_eq!(w.create_filtered_query::<&Hp, &Mesh, Grass>().iter().count(), 1);
    assert_eq!(FILTER_CALLS.load(Ordering::SeqCst), 1);
}
struct Payload(Arc<AtomicUsize>);
impl Payload
{
    fn new(counter: &Arc<AtomicUsize>) -> Self
    {
        counter.fetch_add(1, Ordering::SeqCst);
        Self(counter.clone())
    }
}
impl Clone for Payload
{
    fn clone(&self) -> Self
    {
        Self::new(&self.0)
    }
}
impl Drop for Payload
{
    fn drop(&mut self)
    {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[derive(Clone)]
struct Resource(u32, #[allow(dead_code)] Payload);
impl TSharedComponent for Resource
{
    type Key = u32;
    fn shared_key(&self) -> u32
    {
        self.0
    }
}
#[test]
fn value_lifetime_tracks_nonempty_archetypes()
{
    let live = Arc::new(AtomicUsize::new(0));
    let count = || live.load(Ordering::SeqCst);
    {
        let mut w = World::default();
        let a = w.create(Hp(1));
        let b = w.create(Hp(2));
        w.add_shared_component(a, Resource(1, Payload::new(&live))).unwrap();
        assert_eq!(count(), 1);
        assert!(w.add_shared_component(b, Resource(1, Payload::new(&live))).is_err());
        assert_eq!(count(), 1, "the rejected value is dropped");
        w.add_shared_component_by_key::<Resource>(b, 1).unwrap();
        assert_eq!(count(), 1);
        w.set_shared_component(b, Resource(1, Payload::new(&live))).unwrap();
        assert_eq!(count(), 1, "the replaced value is dropped");
        w.add_component(a, Mana(1));
        assert_eq!(count(), 2);
        w.merge_component(a, Mana(2));
        assert_eq!(count(), 2);
        w.remove_component::<Mana>(a);
        assert_eq!(count(), 1);
        w.remove_shared_component::<Resource>(a).unwrap();
        assert_eq!(count(), 1);
        w.destroy(b);
        assert_eq!(count(), 0);
        w.add_shared_component(a, Resource(1, Payload::new(&live))).unwrap();
    }
    assert_eq!(count(), 0);
}
#[test]
fn commands_apply_after_queries_and_report_errors()
{
    let mut w = World::default();
    let e = spawn(&mut w, 1, 7);
    let f = w.create(Hp(2));
    let mut commands = SharedCommands::default();
    {
        let mut q = w.create_shared_query::<&Hp, &Mesh>();
        for _ in q.iter()
        {
            commands.remove_shared_component::<Mesh>(e);
            commands.add_shared_component(e, mesh(8));
            commands.add_shared_component_by_key::<Mesh>(f, 8);
        }
    }
    assert_eq!(commands.apply(&mut w), vec![Ok(()), Ok(()), Ok(())]);
    let renamed = Mesh {
        id:       9,
        vertices: vec![90],
    };
    commands.set_shared_component_by_key::<Mesh>(e, 9);
    commands.set_shared_component(e, mesh(9));
    commands.override_shared_component(e, renamed.clone());
    assert_eq!(
        commands.apply(&mut w),
        vec![Err(XynokEcsError::UnresolvedSharedKey), Err(XynokEcsError::SharedKeyMismatch), Ok(())]
    );
    assert_eq!(w.get_shared_component::<Mesh>(f), Some(&renamed));
    assert!(commands.apply(&mut w).is_empty());
}
#[test]
#[should_panic(expected = "origin query does not contain")]
fn filtering_undeclared_shared_component_panics()
{
    World::default().create_query::<&Hp>().filter_shared::<Mesh>(&7);
}
#[test]
#[should_panic(expected = "Query contains duplicated components")]
fn overlapping_shared_tuple_access_rejected()
{
    World::default().create_shared_query::<&Hp, (&Mesh, &mut Mesh)>();
}
// Also type-check the intended system parameter syntax.
#[allow(dead_code)]
fn render(mut q: Query<&Hp, &mut Mesh, Grass>)
{
    for (mut mesh, _) in q.iter_archetype()
    {
        mesh.vertices.push(1);
    }
}

#[test]
fn shared_only_entities_and_chained_filters()
{
    let mut w = World::default();
    let a = w.create(());
    let b = w.create(());
    for e in [a, b]
    {
        add_mesh(&mut w, e, 7);
    }
    w.add_shared_component(a, material(1, "one")).unwrap();
    w.add_shared_component(b, material(2, "two")).unwrap();
    let mut q = w.create_shared_query::<(), (&Mesh, &Material)>();
    assert_eq!(q.iter().count(), 2);
    assert_eq!(q.filter_shared::<Mesh>(&7).filter_shared::<Material>(&1).iter().count(), 1);
    assert_eq!(q.iter_archetype().count(), 2);
}

#[test]
fn shared_access_is_checked_by_scheduler()
{
    use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
    use xynok_std::unsafe_ptr::HeapPtr;
    fn writer(mut q: Query<&Hp, &mut Mesh>)
    {
        for (mut mesh, _) in q.iter_archetype()
        {
            mesh.vertices.push(42);
        }
    }
    fn reader(mut q: Query<&Hp, &Mesh>)
    {
        assert_eq!(q.iter_archetype().next().unwrap().0.vertices, [7, 42]);
    }
    fn conflicting(_: Query<&Hp, &Mesh>, _: Query<&Hp, &mut Mesh>) {}
    let world = HeapPtr::new(World::default());
    spawn(&mut world.as_ref_mut(), 1, 7);
    let mut scheduler = DefaultScheduler::new(world);
    scheduler
        .add_system(DefaultScheduleSession::Update, writer)
        .add_system(DefaultScheduleSession::Update, reader);
    scheduler.run(DefaultScheduleSession::Update);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            scheduler.add_system(DefaultScheduleSession::Start, conflicting);
        }))
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            scheduler.add_system_parallel(DefaultScheduleSession::Start, (writer, reader));
        }))
        .is_err()
    );
}

#[test]
fn destination_lookup_compares_other_shared_keys()
{
    let mut w = World::default();
    let a = spawn(&mut w, 1, 7);
    let b = spawn(&mut w, 2, 8);
    w.add_shared_component(a, material(1, "one")).unwrap();
    w.add_shared_component(b, material(2, "two")).unwrap();
    assert_eq!(w.set_shared_component_by_key::<Mesh>(a, 8), Err(XynokEcsError::UnresolvedSharedKey));
    w.remove_shared_component::<Mesh>(a).unwrap();
    w.add_shared_component(
        a,
        Mesh {
            id:       8,
            vertices: vec![80],
        },
    )
    .unwrap();
    let mut q = w.create_shared_query::<&Hp, (&Mesh, &Material)>();
    assert_eq!(q.iter_archetype().count(), 2);
    assert_eq!(q.filter_shared::<Material>(&1).iter_archetype().next().unwrap().0.0.vertices, [80]);
}

#[derive(Clone)]
#[repr(align(128))]
struct Huge([u8; 32 * 1024]);
impl TSharedComponent for Huge
{
    type Key = ();
    fn shared_key(&self) {}
}
#[test]
fn shared_value_can_exceed_chunk_size_and_alignment()
{
    let mut w = World::default();
    let e = w.create(());
    w.add_shared_component(e, Huge([9; 32 * 1024])).unwrap();
    let mut q = w.create_shared_query::<(), &mut Huge>();
    let (mut value, _) = q.iter_archetype().next().unwrap();
    assert_eq!(&*value as *const Huge as usize % 128, 0);
    value.0[32767] = 10;
    assert_eq!(value.0[32767], 10);
}

#[test]
fn repeated_key_changes_reuse_empty_shared_archetype_storage()
{
    let mut w = World::default();
    let e = spawn(&mut w, 1, 7);
    for key in 8..1000
    {
        w.remove_shared_component::<Mesh>(e).unwrap();
        w.add_shared_component(e, mesh(key)).unwrap();
    }
    assert!(testing::archetype_count(&w) <= 3);
    let ordinary = w.create(Hp(2));
    assert_eq!(w.create_shared_query::<&Hp, &Mesh>().iter().count(), 1);
    w.remove_shared_component::<Mesh>(e).unwrap();
    assert_eq!(w.create_query::<&Hp>().into_iter().count(), 2);
    assert!(w.exists(ordinary));
}

#[test]
fn shared_exclusion_filters_rows_archetypes_and_recycled_storage()
{
    let mut w = World::default();
    let ordinary = w.create(Hp(1));
    let shared = spawn(&mut w, 2, 7);
    {
        let mut q = w.create_shared_query::<&Hp, WithoutShared<Mesh>>();
        assert_eq!(q.iter().map(|hp| hp.0).collect::<Vec<_>>(), [1]);
        assert_eq!(q.iter_archetype().count(), 1);
    }
    w.remove_shared_component::<Mesh>(shared).unwrap();
    assert_eq!(w.create_shared_query::<&Hp, WithoutShared<Mesh>>().iter().count(), 2);
    w.add_shared_component(ordinary, mesh(8)).unwrap();
    assert_eq!(
        w.create_shared_query::<&Hp, WithoutShared<Mesh>>().iter().map(|hp| hp.0).collect::<Vec<_>>(),
        [2]
    );
    assert_eq!(w.create_shared_query::<&Hp, (&Mesh, WithoutShared<Mesh>)>().iter().count(), 0);
}

#[test]
fn shared_exclusion_keeps_mutable_normal_queries_disjoint()
{
    use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
    use xynok_std::unsafe_ptr::HeapPtr;
    fn update(with: Query<&mut Hp, &Mesh>, without: Query<&mut Hp, WithoutShared<Mesh>>)
    {
        for hp in with
        {
            hp.0 += 10;
        }
        for hp in without
        {
            hp.0 += 100;
        }
    }
    let world = HeapPtr::new(World::default());
    world.as_ref_mut().create(Hp(1));
    spawn(&mut world.as_ref_mut(), 2, 7);
    fn check(q: Query<&Hp>)
    {
        let mut values = q.into_iter().map(|hp| hp.0).collect::<Vec<_>>();
        values.sort();
        assert_eq!(values, [12, 101]);
    }
    let mut scheduler = DefaultScheduler::new(world);
    scheduler
        .add_system(DefaultScheduleSession::Update, update)
        .add_system(DefaultScheduleSession::Update, check);
    scheduler.run(DefaultScheduleSession::Update);
}

#[test]
fn shared_exclusion_keeps_mutable_shared_values_disjoint()
{
    use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
    use xynok_std::unsafe_ptr::HeapPtr;
    fn with_mesh(mut q: Query<(), (&mut Material, &Mesh)>)
    {
        for ((mut material, _), _) in q.iter_archetype()
        {
            material.name.push_str(" with");
        }
    }
    fn without_mesh(mut q: Query<(), (&mut Material, WithoutShared<Mesh>)>)
    {
        for ((mut material, ()), _) in q.iter_archetype()
        {
            material.name.push_str(" without");
        }
    }
    let world = HeapPtr::new(World::default());
    let a = spawn(&mut world.as_ref_mut(), 1, 7);
    let b = world.as_ref_mut().create(Hp(2));
    for e in [a, b]
    {
        world.as_ref_mut().add_shared_component(e, material(1, "value")).unwrap();
    }
    fn check(mut q: Query<(), &Material>)
    {
        let mut values = q.iter_archetype().map(|(value, _)| value.name.clone()).collect::<Vec<_>>();
        values.sort();
        assert_eq!(values, ["value with", "value without"]);
    }
    let mut scheduler = DefaultScheduler::new(world);
    scheduler
        .add_system_parallel(DefaultScheduleSession::Update, (with_mesh, without_mesh))
        .add_system(DefaultScheduleSession::Update, check);
    scheduler.run(DefaultScheduleSession::Update);
}

#[test]
#[should_panic(expected = "origin query does not contain")]
fn exclusion_does_not_grant_access_for_key_filtering()
{
    World::default().create_shared_query::<(), WithoutShared<Mesh>>().filter_shared::<Mesh>(&7);
}

#[test]
#[should_panic(expected = "Query contains duplicated components")]
fn exclusion_does_not_allow_duplicate_mutable_tuple_entries()
{
    World::default().create_shared_query::<(), (&mut Mesh, &mut Mesh, WithoutShared<Material>)>();
}

#[test]
fn static_shared_key_filter_also_contributes_to_query_scope()
{
    use xynok_ecs::schedule::scheduler::{DefaultScheduleSession, DefaultScheduler, TScheduler};
    use xynok_std::unsafe_ptr::HeapPtr;
    struct OnlyMesh;
    impl TSharedFilter for OnlyMesh
    {
        type SharedComponents = Mesh;
        fn filter_key() -> u32
        {
            7
        }
    }
    fn update(with: Query<&mut Hp, NoSharedComponent, OnlyMesh>, without: Query<&mut Hp, WithoutShared<Mesh>>)
    {
        for hp in with
        {
            hp.0 += 10;
        }
        for hp in without
        {
            hp.0 += 100;
        }
    }
    let world = HeapPtr::new(World::default());
    world.as_ref_mut().create(Hp(1));
    spawn(&mut world.as_ref_mut(), 2, 7);
    fn check(q: Query<&Hp>)
    {
        let mut values = q.into_iter().map(|hp| hp.0).collect::<Vec<_>>();
        values.sort();
        assert_eq!(values, [12, 101]);
    }
    let mut scheduler = DefaultScheduler::new(world);
    scheduler
        .add_system(DefaultScheduleSession::Update, update)
        .add_system(DefaultScheduleSession::Update, check);
    scheduler.run(DefaultScheduleSession::Update);
}

#[derive(Clone)]
struct SharedTag<const N: usize>(usize);
impl<const N: usize> TSharedComponent for SharedTag<N>
{
    type Key = ();
    fn shared_key(&self) {}
}

#[test]
fn shared_bitsets_span_words_and_use_world_local_ids()
{
    let mut first = World::default();
    // Register absent types through queries before inserting any shared values.
    macro_rules! register_tags { ($($n:expr),*) => { $(
        assert_eq!(first.create_shared_query::<(), &SharedTag<$n>>().iter().count(), 0);
    )* }; }
    register_tags!(
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39,
        40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69
    );
    let e = first.create(());
    first.add_shared_component(e, SharedTag::<70>(70)).unwrap();
    first.add_shared_component(e, mesh(7)).unwrap();
    first.add_component(e, Hp(10));
    assert_eq!(first.create_shared_query::<&Hp, &SharedTag<70>>().iter().count(), 1);
    assert_eq!(first.create_shared_query::<(), &SharedTag<6>>().iter().count(), 0);
    assert_eq!(first.create_shared_query::<(), WithoutShared<SharedTag<70>>>().iter().count(), 0);

    // The same type gets ID zero in this world, independently of the first world.
    let mut second = World::default();
    let other = second.create(());
    second.add_shared_component(other, SharedTag::<70>(700)).unwrap();
    assert_eq!(second.create_shared_query::<(), &SharedTag<70>>().iter_archetype().next().unwrap().0.0, 700);
    assert_eq!(first.create_shared_query::<(), &SharedTag<70>>().iter_archetype().next().unwrap().0.0, 70);

    first.remove_shared_component::<SharedTag<70>>(e).unwrap();
    assert_eq!(first.create_shared_query::<(), &SharedTag<70>>().iter().count(), 0);
    assert_eq!(first.create_shared_query::<(), (&Mesh, WithoutShared<SharedTag<70>>)>().iter().count(), 1);
    first.add_shared_component(e, SharedTag::<70>(71)).unwrap();
    assert_eq!(first.create_shared_query::<(), &SharedTag<70>>().iter_archetype().next().unwrap().0.0, 71);
}
