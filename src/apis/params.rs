use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{component_spec::ComponentSpec, swapped_row::SwappedRow, traits::TArchetype},
    archetype::Archetype,
    chunk::{layout::ChunkLayout, Chunk},
    entity::Entity,
};

#[derive(Debug, Clone, Copy)]
pub struct EntityInChunkIndices
{
    pub chunk_idx:    usize,
    pub idx_in_chunk: usize,
}
pub struct EntityIndices
{
    pub e:            Entity,
    pub chunk_idx:    usize,
    pub idx_in_chunk: usize,
}
pub struct TakeComponentParams
{
    e:    Entity,
    from: EntityInChunkIndices,
    to:   EntityInChunkIndices,
}
pub struct ArchetypeTakeAndWriteComponentParams<'a, T: TArchetype + 'static>
{
    pub src_e:           EntityIndices,
    pub src_arch:        &'a mut Archetype,
    pub src_layout:      &'a ChunkLayout,
    pub dst_layout:      &'a ChunkLayout,
    pub component_specs: &'a HashMap<TypeId, ComponentSpec>,
    pub val:             T,
}

pub struct ResultTakeAndWrite
{
    pub new_e_indices: EntityInChunkIndices,
    pub swapped_e:     Option<SwappedRow>,
}
pub struct ChunkTakeComponentParams<'a>
{
    pub e:               Entity,
    pub from:            usize,
    pub to:              usize,
    pub src_chunk:       &'a mut Chunk,
    pub src_layout:      &'a ChunkLayout,
    pub dst_layout:      &'a ChunkLayout,
    pub component_specs: &'a HashMap<TypeId, ComponentSpec>,
}
