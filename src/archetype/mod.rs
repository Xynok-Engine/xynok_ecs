use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{
        fn_ptr::FnArchtypeRemoveEntity,
        identifies::XynokEcsError,
        params::{
            ArchetypeTakeAndRemoveComponentParams, ArchetypeTakeAndWriteComponentParams, ChunkTakeComponentParams, ComponentSpec, EntityInChunkIndices,
            EntityIndices, ResultTakeAndRemove, ResultTakeAndWrite, SwappedRow,
        },
        traits::TArchetype,
    },
    chunk::{
        layout::{self, ChunkLayout},
        Chunk,
    },
    entity::Entity,
    std::queue::Queue,
};

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
    ///
    /// `StorageType = T` restricts this to archetypes whose value is already in storage form.
    /// Query-only wrappers (e.g. `Disabled<Hp>`, where `StorageType = Hp`) are therefore not writable.
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

        let swapped_row = unsafe { chunk.swap_remove_at(layout, component_specs, idx)? };
        unsafe {
            chunk.decrease_len();
        }
        Ok(swapped_row)
    }

    /// used when calling add_component(), this archetype takes components from src_arch, adds the T value,
    /// and returns the new index along with the index that was swapped in src_arch
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

            T::write_at(params.dst_layout, chunk, idx_in_chunk, params.src_e.e, params.write_val)?;

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
    pub fn take_and_remove_from<T: TArchetype + 'static>(
        &mut self,
        params: ArchetypeTakeAndRemoveComponentParams<T>,
    ) -> Result<ResultTakeAndRemove<T>, XynokEcsError>
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

        let (idx_in_chunk, swapped_row_at_src_chunk, taken) = unsafe {
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
            let taken = T::take_from(params.dst_layout, src_chunk, params.src_e.idx_in_chunk)?;

            chunk.increase_len();
            src_chunk.decrease_len();
            (idx_in_chunk, swapped_row, taken)
        };

        if !chunk.is_full()
        {
            self.free_chunks.enqueue(free_chunk_idx);
        }

        let result = ResultTakeAndRemove {
            new_indices_took: EntityInChunkIndices {
                chunk_idx:    free_chunk_idx,
                idx_in_chunk: idx_in_chunk,
            },
            swapped_e:        swapped_row_at_src_chunk,
            val:              taken,
        };
        Ok(result)
    }
}
