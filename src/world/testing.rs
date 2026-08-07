//! Read-only introspection into `World`, `Archetype` and `Chunk` internals.
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
