use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{
        fn_ptr::FnArchtypeRemoveEntity,
        identifies::XynokEcsError,
        params::{
            ArchetypeTakeAndWriteComponentParams, ChunkTakeComponentParams, ComponentSpec, EntityInChunkIndices, EntityIndices, ResultTakeAndWrite, SwappedRow,
        },
        traits::TArchetype,
    },
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

impl Archetype
{
    pub fn new() -> Self
    {
        Self {
            chunks:      Vec::with_capacity(16),
            free_chunks: Queue::with_capacity(8),
        }
    }

    /// Write to a free chunk and increment its length
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
            let idx_in_chunk = chunk.len();
            match T::write_at(layout, chunk, idx_in_chunk, e, val)
            {
                Ok(_) =>
                {}
                Err(e) => return Err(e),
            };
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
        unsafe {
            chunk.decrease_len();
        }
        Ok(swapped_row)
    }

    pub fn take_and_write_from<T: TArchetype + 'static>(&mut self, params: ArchetypeTakeAndWriteComponentParams<T>)
    -> Result<ResultTakeAndWrite, XynokEcsError>
    {
        let free_chunk_idx = match self.free_chunks.dequeue()
        {
            Some(r) => r,
            None =>
            {
                let new_chunk = Chunk::new(params.dst_layout);
                let idx = self.chunks.len();
                self.chunks.push(new_chunk);
                idx
            }
        };

        let chunk = unsafe { self.chunks.get_unchecked_mut(free_chunk_idx) };

        let (idx_in_chunk, swapped_row_at_src_chunk) = unsafe {
            let idx_in_chunk = chunk.len();

            let src_chunk = params.src_arch.chunks.get_unchecked_mut(params.src_e.chunk_idx);
            let swapped_row = match chunk.take_from(ChunkTakeComponentParams {
                e:               params.src_e.e,
                from:            params.src_e.idx_in_chunk,
                to:              idx_in_chunk,
                src_chunk:       src_chunk,
                src_layout:      params.src_layout,
                dst_layout:      params.dst_layout,
                component_specs: params.component_specs,
            })
            {
                Ok(r) => r,
                Err(e) =>
                {
                    return Err(e);
                }
            };
            match T::write_at(params.dst_layout, chunk, idx_in_chunk, params.src_e.e, params.write_val)
            {
                Ok(_) =>
                {}
                Err(e) => return Err(e),
            };

            chunk.increase_len();
            src_chunk.decrease_len();
            (idx_in_chunk, swapped_row)
        };

        if !chunk.is_full()
        {
            self.free_chunks.enqueue(free_chunk_idx);
        }

        let result = ResultTakeAndWrite {
            new_indices_took: EntityInChunkIndices {
                chunk_idx:    free_chunk_idx,
                idx_in_chunk: idx_in_chunk,
            },
            swapped_e:        swapped_row_at_src_chunk,
        };
        Ok(result)
    }
}
