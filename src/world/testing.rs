//! Read-only introspection into `World`, `Archetype` and `Chunk` internals, plus the one
//! mutating hook (`force_entity_version`) a test cannot reach any other way.
//!
//! Integration tests under `tests/` only see this crate's normal public API, which is not
//! enough to check storage invariants like row-swap mapping or chunk reuse. This module is the
//! narrow, explicit surface those tests are allowed to reach through instead of widening the
//! visibility of `World`'s private fields. It only exists when the `test-util` feature is on.

use crate::apis::traits::TComponent;
use crate::entity::Entity;
use crate::world::World;

/// Where an entity's row currently lives.
pub struct EntityLocation
{
    pub arch_id:      usize,
    pub chunk_idx:    usize,
    pub idx_in_chunk: usize,
    pub version:      usize,
    pub has_value:    bool,
}

#[track_caller]
pub fn entity_location(w: &World, e: Entity) -> EntityLocation
{
    let spec = &w.entities[e.idx()];
    EntityLocation {
        arch_id:      spec.arch_id(),
        chunk_idx:    spec.chunk_idx(),
        idx_in_chunk: spec.idx_in_chunk(),
        version:      spec.version(),
        has_value:    spec.has_value(),
    }
}

/// Rewrites the version stored in a live entity's slot, and hands back the handle that now
/// matches it.
///
/// The one thing in here that writes. A slot only runs out of versions after 2^24 rounds of
/// create/destroy, which no test can afford to actually perform, so this drops it at the edge
/// of that range directly. Nothing outside `tests/` has any business calling it.
#[track_caller]
pub fn force_entity_version(w: &mut World, e: Entity, version: usize) -> Entity
{
    let spec = &mut w.entities[e.idx()];
    assert!(spec.has_value(), "{e} is not live, there is no version to rewrite");
    spec.force_version(version);
    Entity::new(e.idx(), version).expect("the caller picked the version, it must be representable")
}

/// Number of distinct archetypes the world has created so far.
pub fn archetype_count(w: &World) -> usize
{
    w.archetypes.len()
}

/// Index of `arch_owner`'s archetype inside the world's registry.
///
/// This is what `QuerySpec` caches. Archetype specs live inline in the registry's dense
/// storage and *do* relocate when it grows, so their addresses are deliberately not part of
/// any invariant - the index is.
#[track_caller]
pub fn archetype_index(w: &World, arch_owner: Entity) -> usize
{
    let arch_id = w.entities[arch_owner.idx()].arch_id();
    w.archetypes.index_of(&arch_id).expect("archetype must exist")
}

#[track_caller]
pub fn chunk_count(w: &World, arch_owner: Entity) -> usize
{
    let arch_id = w.entities[arch_owner.idx()].arch_id();
    w.archetypes.get(&arch_id).expect("archetype must exist").arch.chunk_count()
}

#[track_caller]
pub fn max_len(w: &World, arch_owner: Entity) -> usize
{
    let arch_id = w.entities[arch_owner.idx()].arch_id();
    w.archetypes.get(&arch_id).expect("archetype must exist").layout.max_len
}

#[track_caller]
pub fn free_chunk_count(w: &World, arch_owner: Entity) -> usize
{
    let arch_id = w.entities[arch_owner.idx()].arch_id();
    w.archetypes.get(&arch_id).expect("archetype must exist").arch.free_chunk_count()
}

/// Number of rows currently stored in chunk `chunk_idx` of `arch_owner`'s archetype.
#[track_caller]
pub fn chunk_len(w: &World, arch_owner: Entity, chunk_idx: usize) -> usize
{
    let arch_id = w.entities[arch_owner.idx()].arch_id();
    w.archetypes.get(&arch_id).expect("archetype must exist").arch.chunk_at(chunk_idx).len()
}

/// Reads the [`Entity`] handle stored inside the chunk row `e` is currently mapped to. This is
/// the core invariant swap-remove must preserve: every live entity must map to the row that
/// stores its own handle.
#[track_caller]
pub fn entity_stored_at_row_of(w: &World, e: Entity) -> Entity
{
    let spec = &w.entities[e.idx()];
    let arch_spec = w.archetypes.get(&spec.arch_id()).expect("archetype must exist");
    *arch_spec
        .arch
        .chunk_at(spec.chunk_idx())
        .get_entity(&arch_spec.layout, spec.idx_in_chunk())
        .expect("row must be within the chunk length")
}

/// Reads the [`Entity`] handle stored at an explicit `(chunk, row)` of `arch_owner`'s archetype.
#[track_caller]
pub fn entity_stored_at(w: &World, arch_owner: Entity, chunk_idx: usize, row: usize) -> Entity
{
    let arch_id = w.entities[arch_owner.idx()].arch_id();
    let arch_spec = w.archetypes.get(&arch_id).expect("archetype must exist");
    *arch_spec
        .arch
        .chunk_at(chunk_idx)
        .get_entity(&arch_spec.layout, row)
        .expect("row must be within the chunk length")
}

/// Reads component `C` back out of the chunk the entity currently lives in.
#[track_caller]
pub fn read_component<C: TComponent + Copy + 'static>(w: &World, e: Entity) -> C
{
    let spec = &w.entities[e.idx()];
    let arch_spec = w.archetypes.get(&spec.arch_id()).expect("archetype must exist");
    *arch_spec
        .arch
        .chunk_at(spec.chunk_idx())
        .get_component::<C>(&arch_spec.layout, spec.idx_in_chunk())
        .expect("component must be present in the entity's archetype")
}

/// A row's change-detection and enable state, read straight out of the chunk region it lives in.
///
/// The query filters (`Added`/`Changed`/`Enabled`/`Disabled`) compare these against a system's
/// own last-run tick, which makes them a coarse instrument for asserting that the state itself
/// moved correctly when a row got swapped or migrated. These helpers look at the raw values
/// instead, so a test can say "this row's changed tick is exactly the one it had before".
pub struct ComponentState
{
    pub enabled:      Option<bool>,
    pub added_tick:   Option<crate::apis::constants::ChangedTick>,
    pub changed_tick: Option<crate::apis::constants::ChangedTick>,
}

#[track_caller]
pub fn component_state<C: TComponent + 'static>(w: &World, e: Entity) -> ComponentState
{
    let spec = &w.entities[e.idx()];
    let arch_spec = w.archetypes.get(&spec.arch_id()).expect("archetype must exist");
    let col_des = arch_spec
        .layout
        .component_col_descriptors
        .get(&std::any::TypeId::of::<C::StorageType>())
        .expect("component must be present in the entity's archetype");

    let chunk_ptr = arch_spec.arch.chunk_at(spec.chunk_idx()).ptr();
    let row = spec.idx_in_chunk();
    unsafe {
        ComponentState {
            enabled:      col_des.state_offset.enable_offset.map(|o| crate::chunk::read_bit(chunk_ptr.add(o), row)),
            added_tick:   col_des
                .state_offset
                .added_offset
                .map(|o| crate::chunk::read_changed_tick(chunk_ptr.add(o), row)),
            changed_tick: col_des
                .state_offset
                .changed_offset
                .map(|o| crate::chunk::read_changed_tick(chunk_ptr.add(o), row)),
        }
    }
}
