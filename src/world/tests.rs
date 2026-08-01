//! Integration tests for the storage layer: Entity -> World -> Archetype -> Chunk -> ChunkLayout.
//!
//! This module lives under `world` so it can reach the private fields of [`World`]
//! (`entities`, `archetypes`, `component_counter`, `free_entities`) without widening
//! their visibility. Reaching into chunks goes through the `#[cfg(test)]` accessors
//! on [`Archetype`].
//!
//! Run with:
//! ```text
//! cargo test
//! cargo test -- --nocapture --test-threads=1   # deterministic drop-counter tests
//! ```
//!
//! The whole suite is green. Two invariants are worth keeping in mind when editing it:
//!
//! * Row mapping — every live entity must map to the chunk row that stores its own handle.
//!   `assert_entity_mapping_is_consistent` checks it, and it is what catches a swap-remove
//!   that compacts the data but forgets to re-point the entity that moved.
//! * Memory — the drop-counter tests only prove the drop glue ran; they stay green even if
//!   the 16 KB buffer leaks. Actual reclamation is measured through the chunk-counting global
//!   allocator in the `alloc_probe` module at the bottom of this file.
#![allow(unused)]
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

use crate::{
    apis::{
        constants::CHUNK_SIZE_IN_BYTE,
        identifies::{StorageLocation, XynokEcsError},
        traits::{TComponent, TComponentDescriptor},
        ComponentDescriptor,
    },
    chunk::layout::{ChunkLayout, ChunkLayoutParams},
    entity::Entity,
    world::{arch_spec::ArchetypeSpec, entity_spec::EntitySpec, World},
};

// ------------------------------------------------------------------------------------------------
// Component fixtures
// ------------------------------------------------------------------------------------------------

macro_rules! declare_component {
    ($ty:ty) => {
        impl TComponent for $ty
        {
            type QueryType = Self;
            type StorageType = Self;

            const STORAGE_LOCATION: StorageLocation = StorageLocation::Chunk;
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hp(u32);
declare_component!(Hp);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mana(u32);
declare_component!(Mana);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pos
{
    x: f32,
    y: f32,
}
declare_component!(Pos);

/// Zero-sized component: every row shares the same offset and `byte_size == 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Marker;
declare_component!(Marker);

/// Over-aligned component, to check that column offsets honour `align_of`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(align(32))]
struct Aligned32(u64);
declare_component!(Aligned32);

/// Component with a non-trivial `Drop`, used to verify the drop glue is invoked
/// exactly once per stored value.
#[derive(Debug)]
#[allow(unused)]
struct Tracked(pub u32);
declare_component!(Tracked);

static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
/// Serialises the drop-counter tests so they stay meaningful under a parallel test runner.
static DROP_TEST_LOCK: Mutex<()> = Mutex::new(());

impl Drop for Tracked
{
    fn drop(&mut self)
    {
        DROP_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

fn reset_drop_count()
{
    DROP_COUNT.store(0, Ordering::SeqCst);
}
fn drop_count() -> usize
{
    DROP_COUNT.load(Ordering::SeqCst)
}

// ------------------------------------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------------------------------------

/// Rounds `offset` up to a multiple of `align`. Mirrors `chunk::header::align_up`, which is
/// unreachable from here because `chunk::header` is a private module.
fn align_up(offset: usize, align: usize) -> usize
{
    (offset + align - 1) & !(align - 1)
}

/// Small LCG so the stress tests are pseudo-random but fully reproducible.
struct Rng(u64);
impl Rng
{
    fn new(seed: u64) -> Self
    {
        Self(seed)
    }
    fn next_u32(&mut self) -> u32
    {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, upper: usize) -> usize
    {
        (self.next_u32() as usize) % upper
    }
}

fn layout_of(descriptors: &[ComponentDescriptor]) -> Result<ChunkLayout, XynokEcsError>
{
    let mut temp = std::collections::HashMap::new();
    ChunkLayout::new(ChunkLayoutParams {
        arch:                       descriptors,
        component_descriptors_temp: &mut temp,
    })
}

fn spec_of(w: &World, e: Entity) -> &EntitySpec
{
    &w.entities[e.idx()]
}

fn arch_spec_of(w: &World, e: Entity) -> &ArchetypeSpec
{
    w.archetypes.get(&spec_of(w, e).arch_id()).expect("archetype of entity must exist")
}

/// Reads component `C` back out of the chunk the entity currently lives in.
fn read<C: TComponent + 'static>(w: &World, e: Entity) -> &C
{
    let spec = spec_of(w, e);
    let arch_spec = arch_spec_of(w, e);
    arch_spec
        .arch
        .chunk_at(spec.chunk_idx())
        .get_component::<C>(&arch_spec.layout, spec.idx_in_chunk())
        .expect("component must be present in the entity's archetype")
}

/// Reads the [`Entity`] handle stored inside the chunk row the entity is mapped to.
fn entity_stored_at_row_of(w: &World, e: Entity) -> Entity
{
    let spec = spec_of(w, e);
    let arch_spec = arch_spec_of(w, e);
    *arch_spec
        .arch
        .chunk_at(spec.chunk_idx())
        .get_entity(&arch_spec.layout, spec.idx_in_chunk())
        .expect("row must be within the chunk length")
}

/// Reads the [`Entity`] handle stored at an explicit `(chunk, row)` of the entity's archetype.
fn entity_stored_at(w: &World, arch_owner: Entity, chunk_idx: usize, row: usize) -> Entity
{
    let arch_spec = arch_spec_of(w, arch_owner);
    *arch_spec
        .arch
        .chunk_at(chunk_idx)
        .get_entity(&arch_spec.layout, row)
        .expect("row must be within the chunk length")
}

fn chunk_count_of(w: &World, e: Entity) -> usize
{
    arch_spec_of(w, e).arch.chunk_count()
}

fn max_len_of(w: &World, e: Entity) -> usize
{
    arch_spec_of(w, e).layout.max_len
}

/// Asserts that every live entity still maps to the chunk row that stores its own handle.
/// This is the core invariant that swap-remove must preserve.
fn assert_entity_mapping_is_consistent(w: &World, live: &[Entity])
{
    for &e in live
    {
        let stored = entity_stored_at_row_of(w, e);
        assert_eq!(
            stored,
            e,
            "row {} of chunk {} should hold {} but holds {}",
            spec_of(w, e).idx_in_chunk(),
            spec_of(w, e).chunk_idx(),
            e,
            stored
        );
    }
}

// ------------------------------------------------------------------------------------------------
// Entity packing
// ------------------------------------------------------------------------------------------------

#[test]
fn t_entity_pack_roundtrip()
{
    for (idx, version) in [(0usize, 1usize), (1, 1), (42, 7), (1_000_000, 3), (Entity::MAX_IDX, 5)]
    {
        let e = Entity::new(idx, version).unwrap();
        assert_eq!(e.idx(), idx, "idx must survive packing");
        assert_eq!(e.version(), version, "version must survive packing");
    }
}

#[test]
fn t_entity_max_bounds_are_representable()
{
    let e = Entity::new(Entity::MAX_IDX, Entity::MAX_VERSION).unwrap();
    assert_eq!(e.idx(), Entity::MAX_IDX);
    assert_eq!(e.version(), Entity::MAX_VERSION);
}

#[test]
fn t_entity_null_is_distinct_from_any_live_handle()
{
    assert_eq!(Entity::NULL.raw(), 0);
    assert_eq!(Entity::default(), Entity::NULL);
    // A live handle always carries version >= 1, so it can never collide with NULL.
    assert_ne!(Entity::new(0, Entity::INITIALIZE_VERSION).unwrap(), Entity::NULL);
}

#[test]
fn t_layout_column_offsets_respect_alignment()
{
    let descriptors = [Hp::COMPONENT_DESCRIPTOR, Aligned32::COMPONENT_DESCRIPTOR, Pos::COMPONENT_DESCRIPTOR];
    let layout = layout_of(&descriptors).expect("layout must be constructible");

    for descriptor in &descriptors
    {
        let column = layout
            .component_col_descriptors
            .get(&descriptor.query_type_id)
            .expect("every component of the archetype must own a column");
        assert_eq!(
            column.offset % descriptor.align,
            0,
            "column at offset {} violates align {}",
            column.offset,
            descriptor.align
        );
    }
}

#[test]
fn t_layout_columns_do_not_overlap_and_stay_inside_the_chunk()
{
    let descriptors = [Hp::COMPONENT_DESCRIPTOR, Mana::COMPONENT_DESCRIPTOR, Pos::COMPONENT_DESCRIPTOR];
    let layout = layout_of(&descriptors).expect("layout must be constructible");

    let mut spans: Vec<(usize, usize)> = descriptors
        .iter()
        .map(|descriptor| {
            let column = layout.component_col_descriptors.get(&descriptor.query_type_id).unwrap();
            (column.offset, column.offset + descriptor.byte_size * layout.max_len)
        })
        .collect();
    spans.sort();

    let header_end = layout.header.entities_offset + layout.max_len * size_of::<Entity>();
    assert!(
        spans[0].0 >= header_end,
        "first column at {} overlaps the header ending at {}",
        spans[0].0,
        header_end
    );

    for pair in spans.windows(2)
    {
        assert!(pair[0].1 <= pair[1].0, "columns {:?} and {:?} overlap", pair[0], pair[1]);
    }
    assert!(
        spans.last().unwrap().1 <= CHUNK_SIZE_IN_BYTE,
        "last column ends at {} which exceeds the {CHUNK_SIZE_IN_BYTE} byte chunk",
        spans.last().unwrap().1
    );
}

#[test]
fn t_layout_uses_the_chunk_efficiently()
{
    let descriptors = [Hp::COMPONENT_DESCRIPTOR, Mana::COMPONENT_DESCRIPTOR];
    let layout = layout_of(&descriptors).expect("layout must be constructible");

    // `header.size` already covers the enable/disable bitset and the entity column,
    // so only the component columns are counted on top of it.
    let bytes_per_row = size_of::<Hp>() + size_of::<Mana>();
    let used = layout.header.size + bytes_per_row * layout.max_len;

    assert!(used <= CHUNK_SIZE_IN_BYTE, "layout claims {used} bytes for a {CHUNK_SIZE_IN_BYTE} byte chunk");
    assert!(
        used * 100 / CHUNK_SIZE_IN_BYTE >= 90,
        "layout wastes too much of the chunk: {used}/{CHUNK_SIZE_IN_BYTE} bytes used for max_len = {}",
        layout.max_len
    );
}

#[test]
fn t_layout_supports_zero_sized_components()
{
    let layout = layout_of(&[Marker::COMPONENT_DESCRIPTOR]).expect("a ZST-only archetype must be constructible");
    assert!(layout.max_len > 0, "a ZST archetype must still hold rows");
    assert!(layout.component_col_descriptors.contains_key(&Marker::COMPONENT_DESCRIPTOR.query_type_id));
}

#[test]
fn t_layout_header_reserves_room_for_the_entity_column()
{
    let layout = layout_of(&[Hp::COMPONENT_DESCRIPTOR]).expect("layout must be constructible");
    let entities_end = layout.header.entities_offset + layout.max_len * size_of::<Entity>();

    assert!(
        entities_end <= layout.header.size,
        "entity column ends at {entities_end} but the header is only {} bytes",
        layout.header.size
    );
    assert_eq!(
        layout.header.entities_offset % align_up(align_of::<Entity>(), align_of::<Entity>()),
        0,
        "the entity column must be aligned"
    );
}

// ------------------------------------------------------------------------------------------------
// create / exists / destroy
// ------------------------------------------------------------------------------------------------

#[test]
fn t_create_returns_distinct_handles()
{
    let mut w = World::default();
    let handles: Vec<Entity> = (0..64).map(|i| w.create(Hp(i))).collect();

    let unique: std::collections::HashSet<Entity> = handles.iter().copied().collect();
    assert_eq!(unique.len(), handles.len(), "create() must never hand out the same handle twice");
}

#[test]
fn t_create_stores_the_component_value()
{
    let mut w = World::default();
    let e = w.create(Pos { x: 1.5, y: -2.5 });
    assert_eq!(*read::<Pos>(&w, e), Pos { x: 1.5, y: -2.5 });
}

/// The chunk row must carry the owning entity handle, otherwise swap-remove cannot tell
/// the world which entity moved.
#[test]
fn t_create_writes_the_entity_handle_into_its_row()
{
    let mut w = World::default();
    let e0 = w.create(Hp(10));
    let e1 = w.create(Hp(20));

    assert_eq!(entity_stored_at_row_of(&w, e0), e0);
    assert_eq!(entity_stored_at_row_of(&w, e1), e1);
}

#[test]
fn t_exists_tracks_the_entity_lifecycle()
{
    let mut w = World::default();
    let e = w.create(Hp(1));
    assert!(w.exists(e), "a freshly created entity must exist");

    w.destroy(e);
    assert!(!w.exists(e), "a destroyed entity must not exist");
}

#[test]
fn t_exists_rejects_an_unknown_handle()
{
    let mut w = World::default();
    assert!(!w.exists(Entity::new(999, 1).unwrap()), "an index past the entity table must not exist");
    assert!(!w.exists(Entity::NULL), "the null handle must never exist");
}

#[test]
fn t_recycled_slot_gets_a_fresh_version_and_invalidates_the_stale_handle()
{
    let mut w = World::default();
    let old = w.create(Hp(1));
    w.destroy(old);
    let new = w.create(Hp(2));

    assert_eq!(new.idx(), old.idx(), "the freed slot should be reused");
    assert!(new.version() > old.version(), "a reused slot must bump its version");
    assert!(!w.exists(old), "the stale handle must not resolve to the new entity");
    assert!(w.exists(new));
    assert_eq!(*read::<Hp>(&w, new), Hp(2));
}

#[test]
fn t_destroy_last_row_needs_no_swap()
{
    let mut w = World::default();
    let e0 = w.create(Hp(0));
    let e1 = w.create(Hp(1));

    w.destroy(e1);

    assert!(w.exists(e0));
    assert_eq!(*read::<Hp>(&w, e0), Hp(0));
    assert_eq!(entity_stored_at_row_of(&w, e0), e0);
}

/// Removing a row from the middle must move the last row into the hole and re-point
/// the moved entity at its new index.
#[test]
fn t_destroy_middle_row_swaps_the_last_row_back()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..5u32).map(|i| w.create(Hp(i))).collect();
    let (e2, e4) = (entities[2], entities[4]);
    let hole = spec_of(&w, e2).idx_in_chunk();

    w.destroy(e2);

    assert_eq!(
        spec_of(&w, e4).idx_in_chunk(),
        hole,
        "the last row must land in the hole left by the destroyed entity"
    );
    assert_eq!(entity_stored_at_row_of(&w, e4), e4, "the moved row must still carry its own handle");
    assert_eq!(*read::<Hp>(&w, e4), Hp(4), "the moved row must keep its component value");

    let live: Vec<Entity> = entities.iter().copied().filter(|&e| e != e2).collect();
    assert_entity_mapping_is_consistent(&w, &live);
    for (i, &e) in entities.iter().enumerate()
    {
        if e == e2
        {
            continue;
        }
        assert_eq!(*read::<Hp>(&w, e), Hp(i as u32), "{e} lost its value after an unrelated destroy");
    }
}

#[test]
fn t_destroy_every_entity_front_to_back()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..16u32).map(|i| w.create(Hp(i))).collect();

    for (i, &e) in entities.iter().enumerate()
    {
        w.destroy(e);
        let live: Vec<Entity> = entities[i + 1..].to_vec();
        assert_entity_mapping_is_consistent(&w, &live);
        for &survivor in &live
        {
            assert!(w.exists(survivor), "{survivor} must survive the destruction of {e}");
        }
    }
}

#[test]
fn t_destroy_every_entity_back_to_front()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..16u32).map(|i| w.create(Hp(i))).collect();

    for (i, &e) in entities.iter().enumerate().rev()
    {
        w.destroy(e);
        assert_entity_mapping_is_consistent(&w, &entities[..i]);
    }
}

// ------------------------------------------------------------------------------------------------
// Chunk packing
// ------------------------------------------------------------------------------------------------

/// A chunk sized for `max_len` rows must actually accept `max_len` rows before a second
/// chunk is allocated.
#[test]
fn t_entities_pack_into_a_single_chunk_up_to_max_len()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = max_len_of(&w, probe);
    assert!(max_len > 1, "this test needs an archetype holding more than one row per chunk");

    let mut entities = vec![probe];
    for i in 1..max_len as u32
    {
        entities.push(w.create(Hp(i)));
    }

    assert_eq!(chunk_count_of(&w, probe), 1, "{max_len} rows must fit in a single chunk");
    for (i, &e) in entities.iter().enumerate()
    {
        assert_eq!(spec_of(&w, e).chunk_idx(), 0, "{e} should live in the first chunk");
        assert_eq!(*read::<Hp>(&w, e), Hp(i as u32));
    }
}

#[test]
fn t_overflowing_a_chunk_allocates_exactly_one_more()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = max_len_of(&w, probe);

    for i in 1..=max_len as u32
    {
        w.create(Hp(i));
    }

    assert_eq!(chunk_count_of(&w, probe), 2, "row max_len + 1 must open a second chunk, not more");
}

#[test]
fn t_a_full_chunk_never_receives_more_rows_than_it_can_hold()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = max_len_of(&w, probe);

    for i in 1..(max_len * 2) as u32
    {
        w.create(Hp(i));
    }

    let arch_spec = arch_spec_of(&w, probe);
    for chunk_idx in 0..arch_spec.arch.chunk_count()
    {
        let len = arch_spec.arch.chunk_at(chunk_idx).len();
        assert!(len <= max_len, "chunk {chunk_idx} holds {len} rows but only fits {max_len}");
    }
}

/// EXPECTED FAILURE — `Archetype::remove_at` never returns a chunk that just dropped below
/// capacity to `free_chunks`, so the freed slot is leaked and a new chunk is allocated instead.
#[test]
fn t_a_chunk_is_reused_after_a_row_is_freed()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = max_len_of(&w, probe);

    let mut entities = vec![probe];
    for i in 1..max_len as u32
    {
        entities.push(w.create(Hp(i)));
    }
    assert_eq!(chunk_count_of(&w, probe), 1);

    w.destroy(entities[0]);
    let recycled = w.create(Hp(999));

    assert_eq!(
        chunk_count_of(&w, recycled),
        1,
        "the freed row should be reused instead of allocating a new chunk"
    );
}

// ------------------------------------------------------------------------------------------------
// add_component
// ------------------------------------------------------------------------------------------------

#[test]
fn t_add_component_moves_the_entity_to_another_archetype()
{
    let mut w = World::default();
    let e = w.create(Hp(7));
    let before = spec_of(&w, e).arch_id();

    w.add_component(e, Mana(3));

    assert_ne!(spec_of(&w, e).arch_id(), before, "gaining a component must move the entity to a new archetype");
}

#[test]
fn t_add_component_preserves_the_existing_values()
{
    let mut w = World::default();
    let e = w.create(Hp(7));

    w.add_component(e, Mana(3));

    assert_eq!(*read::<Hp>(&w, e), Hp(7), "the pre-existing component must survive the move");
    assert_eq!(*read::<Mana>(&w, e), Mana(3), "the new component must be readable");
}

#[test]
fn t_add_component_keeps_the_entity_handle_in_its_new_row()
{
    let mut w = World::default();
    let e = w.create(Hp(7));

    w.add_component(e, Mana(3));

    assert_eq!(entity_stored_at_row_of(&w, e), e, "the destination row must carry the entity handle");
}

/// EXPECTED FAILURE (`SWAPPED_ROW`) — the world re-points the wrong `EntitySpec` after the
/// source chunk is compacted, so the entity that moved into the hole is never told about it.
#[test]
fn t_add_component_repairs_the_mapping_of_the_swapped_entity()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..5u32).map(|i| w.create(Hp(i))).collect();
    let (e1, e4) = (entities[1], entities[4]);
    let hole = spec_of(&w, e1).idx_in_chunk();

    w.add_component(e1, Mana(100));

    assert_eq!(
        spec_of(&w, e4).idx_in_chunk(),
        hole,
        "the last row of the source chunk must be re-pointed to the hole"
    );
    assert_eq!(entity_stored_at_row_of(&w, e4), e4);
    assert_eq!(*read::<Hp>(&w, e4), Hp(4));

    let untouched: Vec<Entity> = vec![entities[0], entities[2], entities[3], e4];
    assert_entity_mapping_is_consistent(&w, &untouched);
}

/// EXPECTED FAILURE (`SWAPPED_ROW`).
#[test]
fn t_add_component_leaves_the_other_entities_intact()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..8u32).map(|i| w.create(Hp(i))).collect();

    w.add_component(entities[3], Mana(42));

    for (i, &e) in entities.iter().enumerate()
    {
        assert_eq!(*read::<Hp>(&w, e), Hp(i as u32), "{e} lost its Hp when an unrelated entity gained a component");
    }
    assert_eq!(*read::<Mana>(&w, entities[3]), Mana(42));
}

#[test]
fn t_adding_two_components_in_sequence()
{
    let mut w = World::default();
    let e = w.create(Hp(1));

    w.add_component(e, Mana(2));
    w.add_component(e, Pos { x: 3.0, y: 4.0 });

    assert_eq!(*read::<Hp>(&w, e), Hp(1));
    assert_eq!(*read::<Mana>(&w, e), Mana(2));
    assert_eq!(*read::<Pos>(&w, e), Pos { x: 3.0, y: 4.0 });
    assert_eq!(entity_stored_at_row_of(&w, e), e);
}

#[test]
#[should_panic(expected = "already exists")]
fn t_adding_a_component_twice_is_rejected()
{
    let mut w = World::default();
    let e = w.create(Hp(1));
    w.add_component(e, Mana(2));
    w.add_component(e, Mana(3));
}

/// EXPECTED FAILURE (`SWAPPED_ROW`).
#[test]
fn t_entities_with_the_same_component_set_share_one_archetype()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    let b = w.create(Hp(2));
    w.add_component(a, Mana(1));
    w.add_component(b, Mana(2));

    assert_eq!(
        spec_of(&w, a).arch_id(),
        spec_of(&w, b).arch_id(),
        "two entities holding {{Hp, Mana}} must end up in the same archetype"
    );
    assert_eq!(*read::<Hp>(&w, a), Hp(1));
    assert_eq!(*read::<Hp>(&w, b), Hp(2));
}

// ------------------------------------------------------------------------------------------------
// remove_component
// ------------------------------------------------------------------------------------------------

#[test]
fn t_remove_component_returns_the_stored_value()
{
    let mut w = World::default();
    let e = w.create(Hp(9));
    w.add_component(e, Mana(77));

    let taken: Mana = w.remove_component::<Mana>(e);

    assert_eq!(taken, Mana(77), "remove_component must hand back the value that was stored");
}

#[test]
fn t_remove_component_keeps_the_remaining_values()
{
    let mut w = World::default();
    let e = w.create(Hp(9));
    w.add_component(e, Mana(77));

    let _ = w.remove_component::<Mana>(e);

    assert_eq!(*read::<Hp>(&w, e), Hp(9), "the surviving component must keep its value");
    assert_eq!(entity_stored_at_row_of(&w, e), e);
    assert!(w.exists(e));
}

#[test]
fn t_add_then_remove_returns_to_the_original_archetype()
{
    let mut w = World::default();
    let e = w.create(Hp(5));
    let original = spec_of(&w, e).arch_id();

    w.add_component(e, Mana(6));
    let _ = w.remove_component::<Mana>(e);

    assert_eq!(
        spec_of(&w, e).arch_id(),
        original,
        "{{Hp, Mana}} minus Mana must be the original {{Hp}} archetype"
    );
    assert_eq!(*read::<Hp>(&w, e), Hp(5));
}

/// EXPECTED FAILURE (`SWAPPED_ROW`).
#[test]
fn t_remove_component_from_the_middle_of_a_chunk()
{
    let mut w = World::default();
    let mut entities = Vec::new();
    for i in 0..5u32
    {
        let e = w.create(Hp(i));
        w.add_component(e, Mana(100 + i));
        entities.push(e);
    }

    let taken = w.remove_component::<Mana>(entities[2]);

    assert_eq!(taken, Mana(102));
    assert_eq!(*read::<Hp>(&w, entities[2]), Hp(2));
}

/// EXPECTED FAILURE (`DROPPED_COLUMN`, currently masked by `SWAPPED_ROW`) — the entity
/// swapped into the hole reads the Mana of the entity that left, because the Mana column of
/// the source chunk is never compacted. Fix `SWAPPED_ROW` first: until then this test dies
/// on the `idx old(..) != idx new(..)` assertion before it can reach its own check.
#[test]
fn t_remove_component_compacts_the_dropped_column_in_the_source_chunk()
{
    let mut w = World::default();
    let mut entities = Vec::new();
    for i in 0..5u32
    {
        let e = w.create(Hp(i));
        w.add_component(e, Mana(100 + i));
        entities.push(e);
    }
    let last = entities[4];

    let _ = w.remove_component::<Mana>(entities[2]);

    assert_eq!(
        *read::<Mana>(&w, last),
        Mana(104),
        "the entity swapped into the hole must keep its own Mana, not inherit the removed row's"
    );
    assert_eq!(*read::<Hp>(&w, last), Hp(4));
}

/// EXPECTED FAILURE (`SWAPPED_ROW`).
#[test]
fn t_remove_component_repairs_the_mapping_of_the_swapped_entity()
{
    let mut w = World::default();
    let mut entities = Vec::new();
    for i in 0..5u32
    {
        let e = w.create(Hp(i));
        w.add_component(e, Mana(100 + i));
        entities.push(e);
    }
    let hole = spec_of(&w, entities[1]).idx_in_chunk();

    let _ = w.remove_component::<Mana>(entities[1]);

    let moved = entities[4];
    assert_eq!(spec_of(&w, moved).idx_in_chunk(), hole);
    assert_eq!(entity_stored_at_row_of(&w, moved), moved);
    assert_entity_mapping_is_consistent(&w, &[entities[0], entities[2], entities[3], moved]);
}

#[test]
fn t_removing_the_last_component_is_unsupported()
{
    let mut w = World::default();
    let e = w.create(Hp(1));
    let _ = w.remove_component::<Hp>(e);
}

// ------------------------------------------------------------------------------------------------
// merge_component
// ------------------------------------------------------------------------------------------------

#[test]
fn t_merge_component_adds_a_missing_component_like_add_component()
{
    let mut w = World::default();
    let e = w.create(Hp(7));
    let before = spec_of(&w, e).arch_id();

    w.merge_component(e, Mana(3));

    assert_ne!(spec_of(&w, e).arch_id(), before, "gaining a new component must move the entity to a new archetype");
    assert_eq!(*read::<Hp>(&w, e), Hp(7), "the pre-existing component must survive the move");
    assert_eq!(*read::<Mana>(&w, e), Mana(3), "the new component must be readable");
}

#[test]
fn t_merge_component_overwrites_an_existing_component_in_place()
{
    let mut w = World::default();
    let e = w.create(Hp(7));
    let before = spec_of(&w, e).arch_id();

    w.merge_component(e, Hp(99));

    assert_eq!(
        spec_of(&w, e).arch_id(),
        before,
        "merging a component that's already fully covered must not move the entity to another archetype"
    );
    assert_eq!(*read::<Hp>(&w, e), Hp(99), "merge_component must overwrite the existing value");
    assert_eq!(entity_stored_at_row_of(&w, e), e);
}

#[test]
fn t_merge_component_overwrites_overlap_while_adding_new_components()
{
    let mut w = World::default();
    let e = w.create(Hp(7));
    w.add_component(e, Mana(3));

    w.merge_component(e, (Mana(50), Pos { x: 1.0, y: 2.0 }));

    assert_eq!(*read::<Hp>(&w, e), Hp(7), "the unrelated component must survive the merge");
    assert_eq!(*read::<Mana>(&w, e), Mana(50), "merge_component must overwrite the overlapping component");
    assert_eq!(*read::<Pos>(&w, e), Pos { x: 1.0, y: 2.0 }, "the new component must be readable");
    assert_eq!(entity_stored_at_row_of(&w, e), e);
}

/// EXPECTED FAILURE (`SWAPPED_ROW`).
#[test]
fn t_merge_component_repairs_the_mapping_of_the_swapped_entity()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..5u32).map(|i| w.create(Hp(i))).collect();
    let (e1, e4) = (entities[1], entities[4]);
    let hole = spec_of(&w, e1).idx_in_chunk();

    w.merge_component(e1, Mana(100));

    assert_eq!(
        spec_of(&w, e4).idx_in_chunk(),
        hole,
        "the last row of the source chunk must be re-pointed to the hole"
    );
    assert_eq!(entity_stored_at_row_of(&w, e4), e4);
    assert_eq!(*read::<Hp>(&w, e4), Hp(4));

    let untouched: Vec<Entity> = vec![entities[0], entities[2], entities[3], e4];
    assert_entity_mapping_is_consistent(&w, &untouched);
}

#[test]
fn t_merge_component_in_place_drops_the_overwritten_value_exactly_once()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let e = w.create(Tracked(1));
    assert_eq!(drop_count(), 0, "storing a value must not drop it");

    w.merge_component(e, Tracked(2));
    assert_eq!(drop_count(), 1, "overwriting an existing component in place must drop the old value exactly once");

    w.destroy(e);
    assert_eq!(drop_count(), 2, "the new value must still be dropped exactly once when the entity is destroyed");
}

#[test]
fn t_merge_component_moving_archetype_drops_the_overwritten_value_exactly_once()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let e = w.create((Tracked(1), Hp(1)));
    assert_eq!(drop_count(), 0, "storing a value must not drop it");

    w.merge_component(e, (Tracked(2), Mana(1)));
    assert_eq!(
        drop_count(),
        1,
        "the component shared with the entity's current archetype must be dropped, not migrated and leaked"
    );
    assert_eq!(*read::<Hp>(&w, e), Hp(1), "the unrelated component must survive the archetype move");
    assert_eq!(*read::<Mana>(&w, e), Mana(1));

    w.destroy(e);
    assert_eq!(drop_count(), 2, "the new value must still be dropped exactly once when the entity is destroyed");
}

// ------------------------------------------------------------------------------------------------
// Drop glue
// ------------------------------------------------------------------------------------------------

#[test]
fn t_destroying_an_entity_drops_its_components_exactly_once()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let e = w.create(Tracked(1));
    assert_eq!(drop_count(), 0, "storing a value must not drop it");

    w.destroy(e);

    assert_eq!(drop_count(), 1, "destroy() must run the drop glue exactly once");
}

#[test]
fn t_removing_a_component_moves_it_out_without_dropping_it()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let e = w.create(Tracked(1));
    w.add_component(e, Hp(1));

    let taken = w.remove_component::<Tracked>(e);
    assert_eq!(drop_count(), 0, "the value was moved out, it must not have been dropped yet");

    drop(taken);
    assert_eq!(drop_count(), 1, "the moved-out value must drop exactly once, not twice");
}

#[test]
fn t_destroying_many_entities_drops_each_component_once()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    let mut w = World::default();
    let entities: Vec<Entity> = (0..32u32).map(|i| w.create(Tracked(i))).collect();
    for e in entities
    {
        w.destroy(e);
    }

    assert_eq!(drop_count(), 32, "each stored component must be dropped exactly once");
}

/// EXPECTED FAILURE (`NO_DROP`).
#[test]
fn t_dropping_the_world_drops_the_components_it_still_owns()
{
    let _guard = DROP_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    reset_drop_count();

    {
        let mut w = World::default();
        for i in 0..8u32
        {
            w.create(Tracked(i));
        }
    }

    assert_eq!(drop_count(), 8, "dropping the world must drop the components it still owns");
}

// ------------------------------------------------------------------------------------------------
// Stress
// ------------------------------------------------------------------------------------------------

#[test]
fn t_stress_interleaved_create_and_destroy()
{
    let mut w = World::default();
    let mut rng = Rng::new(0xC0FFEE);
    // (handle, expected value) for every entity we believe is alive
    let mut live: Vec<(Entity, u32)> = Vec::new();

    for step in 0..4_000u32
    {
        if live.is_empty() || rng.below(100) < 60
        {
            let e = w.create(Hp(step));
            live.push((e, step));
        }
        else
        {
            let victim = rng.below(live.len());
            let (e, _) = live.swap_remove(victim);
            w.destroy(e);
        }

        if step % 250 == 0
        {
            for &(e, expected) in &live
            {
                assert!(w.exists(e), "{e} vanished at step {step}");
                assert_eq!(*read::<Hp>(&w, e), Hp(expected), "{e} was corrupted at step {step}");
                assert_eq!(entity_stored_at_row_of(&w, e), e, "{e} lost its row mapping at step {step}");
            }
        }
    }

    for &(e, expected) in &live
    {
        assert!(w.exists(e));
        assert_eq!(*read::<Hp>(&w, e), Hp(expected));
    }
}

/// EXPECTED FAILURE (`SWAPPED_ROW`, then `DROPPED_COLUMN`).
#[test]
fn t_stress_add_and_remove_components()
{
    let mut w = World::default();
    let mut rng = Rng::new(0xBADC0DE);

    // (handle, hp, mana if currently attached)
    let mut live: Vec<(Entity, u32, Option<u32>)> = (0..64u32).map(|i| (w.create(Hp(i)), i, None)).collect();

    for step in 0..1_000u32
    {
        let pick = rng.below(live.len());
        match live[pick].2
        {
            None =>
            {
                let mana = 1_000 + step;
                w.add_component(live[pick].0, Mana(mana));
                live[pick].2 = Some(mana);
            }
            Some(expected) =>
            {
                let taken = w.remove_component::<Mana>(live[pick].0);
                assert_eq!(taken, Mana(expected), "remove_component returned the wrong value at step {step}");
                live[pick].2 = None;
            }
        }

        for &(e, hp, mana) in &live
        {
            assert!(w.exists(e), "{e} vanished at step {step}");
            assert_eq!(*read::<Hp>(&w, e), Hp(hp), "{e} lost its Hp at step {step}");
            assert_eq!(entity_stored_at_row_of(&w, e), e, "{e} lost its row mapping at step {step}");
            if let Some(mana) = mana
            {
                assert_eq!(*read::<Mana>(&w, e), Mana(mana), "{e} lost its Mana at step {step}");
            }
        }
    }
}

#[test]
fn t_stress_entities_spanning_many_chunks()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = max_len_of(&w, probe);
    let total = max_len * 3;

    let mut entities = vec![probe];
    for i in 1..total as u32
    {
        entities.push(w.create(Hp(i)));
    }

    for (i, &e) in entities.iter().enumerate()
    {
        assert_eq!(*read::<Hp>(&w, e), Hp(i as u32), "{e} was corrupted while filling {total} rows");
        assert_eq!(entity_stored_at_row_of(&w, e), e);
    }

    // Destroy every third entity, then re-check the survivors.
    let mut survivors = Vec::new();
    for (i, &e) in entities.iter().enumerate()
    {
        if i % 3 == 0
        {
            w.destroy(e);
        }
        else
        {
            survivors.push((e, i as u32));
        }
    }
    for &(e, expected) in &survivors
    {
        assert!(w.exists(e));
        assert_eq!(*read::<Hp>(&w, e), Hp(expected));
        assert_eq!(entity_stored_at_row_of(&w, e), e);
    }
}

// ------------------------------------------------------------------------------------------------
// Misc regression guards
// ------------------------------------------------------------------------------------------------

#[test]
fn t_zero_sized_components_can_be_stored_and_destroyed()
{
    let mut w = World::default();
    let entities: Vec<Entity> = (0..8).map(|_| w.create(Marker)).collect();

    for &e in &entities
    {
        assert_eq!(*read::<Marker>(&w, e), Marker);
        assert_eq!(entity_stored_at_row_of(&w, e), e);
    }
    for &e in &entities
    {
        w.destroy(e);
    }
}

#[test]
fn t_over_aligned_components_land_on_an_aligned_address()
{
    let mut w = World::default();
    let e = w.create(Aligned32(0xDEAD_BEEF));

    let value: &Aligned32 = read::<Aligned32>(&w, e);
    assert_eq!(value.0, 0xDEAD_BEEF);
    assert_eq!(
        (value as *const Aligned32).addr() % align_of::<Aligned32>(),
        0,
        "an over-aligned component must be stored at a correctly aligned address"
    );
}

#[test]
fn t_register_archetype_is_idempotent()
{
    let mut w = World::default();
    w.register_archetype::<Hp>();
    let after_first = w.archetypes.len();
    w.register_archetype::<Hp>();

    assert_eq!(
        w.archetypes.len(),
        after_first,
        "registering the same archetype twice must not create a second one"
    );
}

#[test]
fn t_unrelated_archetypes_do_not_share_rows()
{
    let mut w = World::default();
    let a = w.create(Hp(1));
    let b = w.create(Mana(2));

    assert_ne!(
        spec_of(&w, a).arch_id(),
        spec_of(&w, b).arch_id(),
        "{{Hp}} and {{Mana}} are different archetypes"
    );
    assert_eq!(*read::<Hp>(&w, a), Hp(1));
    assert_eq!(*read::<Mana>(&w, b), Mana(2));
}

/// EXPECTED FAILURE (`DUPLICATE_FREE_CHUNK`) — freeing three rows of the same chunk pushes
/// that chunk into `free_chunks` three times. Each refill pops one copy, and the copies that
/// outlive the refill hand out a chunk that is already full, so `push` writes past `max_len`
/// and corrupts whatever follows the last column.
#[test]
fn t_freeing_rows_must_not_enqueue_the_same_chunk_twice()
{
    let mut w = World::default();
    let probe = w.create(Hp(0));
    let max_len = max_len_of(&w, probe);

    let mut entities = vec![probe];
    for i in 1..max_len as u32
    {
        entities.push(w.create(Hp(i)));
    }
    assert_eq!(
        arch_spec_of(&w, probe).arch.free_chunk_count(),
        0,
        "a chunk filled to max_len must not stay in the free list"
    );

    for &e in entities.iter().take(3)
    {
        w.destroy(e);
    }
    assert_eq!(
        arch_spec_of(&w, probe).arch.free_chunk_count(),
        1,
        "freeing three rows of one chunk must leave that chunk in the free list exactly once"
    );

    for i in 0..4u32
    {
        let e = w.create(Hp(900 + i));
        let spec = spec_of(&w, e);
        assert!(
            spec.idx_in_chunk() < max_len,
            "row {} of chunk {} is outside the chunk (max_len = {max_len})",
            spec.idx_in_chunk(),
            spec.chunk_idx()
        );
    }
}

// ------------------------------------------------------------------------------------------------
// Allocator instrumentation
// ------------------------------------------------------------------------------------------------

/// Counts live chunk-sized allocations so a leaked `Chunk` buffer is observable from a test.
/// The counter is thread-local, which keeps it accurate under the parallel test runner: every
/// test runs on its own thread and only sees its own allocations.
mod alloc_probe
{
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };

    use crate::apis::constants::CHUNK_SIZE_IN_BYTE;

    thread_local! {
        static LIVE_CHUNKS: Cell<isize> = const { Cell::new(0) };
    }

    pub struct ChunkCountingAllocator;

    unsafe impl GlobalAlloc for ChunkCountingAllocator
    {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8
        {
            if layout.size() == CHUNK_SIZE_IN_BYTE
            {
                // `try_with` because TLS is unavailable while a thread is being torn down
                let _ = LIVE_CHUNKS.try_with(|live| live.set(live.get() + 1));
            }
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout)
        {
            if layout.size() == CHUNK_SIZE_IN_BYTE
            {
                let _ = LIVE_CHUNKS.try_with(|live| live.set(live.get() - 1));
            }
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    pub fn live_chunks() -> isize
    {
        LIVE_CHUNKS.with(|live| live.get())
    }
}

#[global_allocator]
static CHUNK_ALLOC_PROBE: alloc_probe::ChunkCountingAllocator = alloc_probe::ChunkCountingAllocator;

/// `Chunk::dispose` runs the drop glue for every stored component, but the 16 KB buffer behind
/// `Chunk::ptr` is never handed back: there is no `dealloc` anywhere in the crate.
#[test]
fn t_dropping_the_world_releases_its_chunk_memory()
{
    let before = alloc_probe::live_chunks();

    {
        let mut w = World::default();
        for i in 0..8u32
        {
            w.create(Hp(i));
        }
        assert!(alloc_probe::live_chunks() > before, "creating entities must allocate at least one chunk");
    }

    assert_eq!(
        alloc_probe::live_chunks(),
        before,
        "dropping the world must release every chunk buffer it allocated"
    );
}

/// Archetype migrations allocate a chunk in the destination archetype; those must be released too.
#[test]
fn t_dropping_the_world_releases_chunks_created_by_migrations()
{
    let before = alloc_probe::live_chunks();

    {
        let mut w = World::default();
        let e = w.create(Hp(1));
        w.add_component(e, Mana(2));
        w.add_component(e, Pos { x: 0.0, y: 0.0 });
        let _ = w.remove_component::<Mana>(e);
    }

    assert_eq!(
        alloc_probe::live_chunks(),
        before,
        "every archetype's chunks must be released, not just the first"
    );
}
