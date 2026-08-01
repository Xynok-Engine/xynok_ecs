use std::{any::TypeId, collections::HashMap, marker::PhantomData};

use crate::{
    apis::{traits::TArchetype, ComponentDescriptor},
    archetype::Archetype,
    chunk::{layout::ChunkLayout, Chunk},
    entity::Entity,
};
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SwappedRow
{
    pub e:    Entity,
    pub from: usize,
    pub to:   usize,
}
pub struct ComponentSpec
{
    pub id:         usize,
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
    pub component_specs: &'a HashMap<TypeId, ComponentSpec>,
    pub write_val:       T,
}
pub struct ArchetypeTakeAndRemoveComponentParams<'a, T: TArchetype + 'static>
{
    pub src_e:           EntityIndices,
    pub src_arch:        &'a mut Archetype,
    pub src_layout:      &'a ChunkLayout,
    pub dst_layout:      &'a ChunkLayout,
    pub component_specs: &'a HashMap<TypeId, ComponentSpec>,
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
    pub component_specs:      &'a HashMap<TypeId, ComponentSpec>,
    /// Component types that the caller will overwrite right after this call (e.g. merge_component's `T`).
    /// Their old values in `src_chunk` are dropped in place instead of being migrated into `dst_layout`.
    pub overwritten_type_ids: &'a [TypeId],
}
