use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{component_spec::ComponentSpec, fn_ptr::FnArchtypeRemoveEntity, identifies::XynokEcsError, swapped_row::SwappedRow, traits::TArchetype},
    archetype::entity_to_chunk::EntityToChunk,
    chunk::{
        Chunk,
        layout::{self, ChunkLayout},
    },
    entity::Entity,
    std::queue::Queue,
};

mod entity_to_chunk;
mod variant;

pub struct Archetype
{
    chunks:      Vec<Chunk>,
    free_chunks: Queue<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct EntityInChunkIndices
{
    pub chunk_idx:    usize,
    pub idx_in_chunk: usize,
}
impl Archetype
{
    pub fn new() -> Self
    {
        Self {
            chunks:      Vec::with_capacity(16),
            free_chunks: Queue::with_capacity(8),
        }
    }

    pub fn push<T: TArchetype + 'static>(&mut self, layout: &ChunkLayout, e: Entity, val: T) -> Result<EntityInChunkIndices, XynokEcsError>
    {
        let free_chunk_idx = match self.free_chunks.dequeue()
        {
            Some(r) => r,
            None =>
            {
                let new_chunk = Chunk::new(layout);
                let idx = self.chunks.len();
                self.chunks.push(new_chunk);
                idx
            }
        };

        let chunk = unsafe { self.chunks.get_unchecked_mut(free_chunk_idx) };

        let idx_in_chunk = unsafe {
            match T::push_to(layout, chunk, e, val)
            {
                Ok(_) =>
                {}
                Err(e) => return Err(e),
            };
            let idx_in_chunk = chunk.len();
            chunk.increase_len();
            idx_in_chunk
        };

        if !chunk.is_full()
        {
            self.free_chunks.enqueue(free_chunk_idx);
        }

        Ok(EntityInChunkIndices {
            chunk_idx:    free_chunk_idx,
            idx_in_chunk: idx_in_chunk,
        })
    }

    pub fn remove_at(
        &mut self,
        fn_removes: &Vec<FnArchtypeRemoveEntity>,
        layout: &ChunkLayout,
        component_specs: &HashMap<TypeId, ComponentSpec>,
        chunk_idx: usize,
        idx: usize,
    ) -> Result<Option<SwappedRow>, XynokEcsError>
    {
        let chunk = match self.chunks.get_mut(chunk_idx)
        {
            Some(r) => r,
            None => return Err(XynokEcsError::ChunkIdxIsNotInRange(chunk_idx, self.chunks.len())),
        };
        let mut swapped_row: Option<SwappedRow> = None;

        // swapped_row must be the same because the small archetypes within the larger archetype all share the same chunk and entity
        for f in fn_removes
        {
            match (f)(layout, component_specs, chunk, idx)
            {
                Ok(swapped) => match swapped
                {
                    Some(s) =>
                    {
                        if let Some(current_swapped) = swapped_row
                            && s != current_swapped
                        {
                            return Err(XynokEcsError::ConflictSubArchetype);
                        }
                        swapped_row = Some(s);
                    }
                    None =>
                    {
                        if swapped_row.is_some()
                        {
                            return Err(XynokEcsError::ConflictSubArchetype);
                        }
                    }
                },
                Err(e) =>
                {
                    return Err(e);
                }
            }
        }
        Ok(swapped_row)
    }
}
