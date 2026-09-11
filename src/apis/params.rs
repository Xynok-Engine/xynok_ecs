use std::any::TypeId;
use std::marker::PhantomData;

use crate::apis::ComponentDescriptor;
use crate::apis::identifies::StorageLocation;
use crate::apis::traits::TArchetype;
use crate::archetype::Archetype;
use crate::archetype_chunk::SharedComponentDescriptor;
use crate::chunk::Chunk;
use crate::chunk::layout::ChunkLayout;
use crate::collection::sequence_value_hash_map::SequenceValueHashMap;
use crate::entity::Entity;
use crate::shared::TSharedComponent;

/// The world's component registry, for normal and shared components alike. A component's id is
/// its index in here, so `index_of(type_id)` is what turns a `TypeId` into a `ComponentBitSet` bit.
pub type ComponentSpecs = SequenceValueHashMap<TypeId, ComponentSpec>;

impl ComponentSpecs
{
    /// Resolves a normal component to its id, registering it on first sight.
    #[track_caller]
    pub fn register_component(&mut self, descriptor: ComponentDescriptor) -> usize
    {
        let id = self.get_or_insert_with(descriptor.storage_type_id, || ComponentSpec { descriptor });
        assert!(
            self.value_at(id).unwrap().descriptor.storage_location == StorageLocation::Chunk,
            "a type registered as a shared component cannot be used as a normal component"
        );
        id
    }

    /// Resolves a shared component to its id, registering it on first sight. It takes an id
    /// from the same space as normal components, so one `AccessScope` covers both kinds.
    #[track_caller]
    pub fn register_shared<C: TSharedComponent>(&mut self) -> usize
    {
        let id = self.get_or_insert_with(TypeId::of::<C>(), || ComponentSpec {
            descriptor: SharedComponentDescriptor::of::<C>().component_descriptor(),
        });
        assert!(
            self.value_at(id).unwrap().descriptor.storage_location == StorageLocation::Archetype,
            "a type registered as a normal component cannot be used as a shared component"
        );
        id
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SwappedRow
{
    pub e:    Entity,
    pub from: usize,
    pub to:   usize,
}
pub struct ComponentSpec
{
    pub descriptor: ComponentDescriptor,
}
#[derive(Debug, Clone, Copy)]
pub struct EntityInChunkIndices
{
    pub chunk_idx:    usize,
    pub idx_in_chunk: usize,
}
pub struct EntityIndices
{
    pub chunk_idx:    usize,
    pub idx_in_chunk: usize,
}

pub struct ArchetypeTakeAndWriteComponentParams<'a, T: TArchetype + 'static>
{
    pub src_e:           EntityIndices,
    pub src_arch:        &'a mut Archetype,
    pub src_layout:      &'a ChunkLayout,
    pub dst_layout:      &'a ChunkLayout,
    pub component_specs: &'a ComponentSpecs,
    pub write_val:       T,
}
pub struct ArchetypeTakeAndRemoveComponentParams<'a, T: TArchetype + 'static>
{
    pub src_e:           EntityIndices,
    pub src_arch:        &'a mut Archetype,
    pub src_layout:      &'a ChunkLayout,
    pub dst_layout:      &'a ChunkLayout,
    pub component_specs: &'a ComponentSpecs,
    pub phantom:         PhantomData<T>,
}
pub struct ResultTakeAndWrite
{
    pub new_indices_took: EntityInChunkIndices,
    pub swapped_e:        Option<SwappedRow>,
}
pub struct ResultTakeAndRemove<T: TArchetype + 'static>
{
    pub new_indices_took: EntityInChunkIndices,
    pub swapped_e:        Option<SwappedRow>,
    pub val:              T,
}
pub struct ChunkTakeComponentParams<'a>
{
    pub from:                 usize,
    pub to:                   usize,
    pub src_chunk:            &'a mut Chunk,
    pub src_layout:           &'a ChunkLayout,
    pub dst_layout:           &'a ChunkLayout,
    pub component_specs:      &'a ComponentSpecs,
    /// Component types that the caller will overwrite right after this call (e.g. merge_component's `T`).
    /// Their old values in `src_chunk` are dropped in place instead of being migrated into `dst_layout`.
    pub overwritten_type_ids: &'a [TypeId],
}
