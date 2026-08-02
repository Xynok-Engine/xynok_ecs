use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::{
    ArchetypeTakeAndRemoveComponentParams, ArchetypeTakeAndWriteComponentParams, ChunkTakeComponentParams, ComponentSpec, EntityInChunkIndices,
    ResultTakeAndRemove, ResultTakeAndWrite, SwappedRow,
};
use crate::apis::traits::TArchetype;
use crate::chunk::layout::ChunkLayout;
use crate::chunk::Chunk;
use crate::entity::Entity;
use crate::std::queue::Queue;

mod variant;

pub struct Archetype
{
    chunks:             Vec<Chunk>,
    free_chunks:        Queue<usize>,
    free_chunks_stored: HashSet<usize>,
}

impl Archetype
{
    pub fn new() -> Self
    {
        Self {
            chunks:             Vec::with_capacity(16),
            free_chunks:        Queue::with_capacity(8),
            free_chunks_stored: HashSet::with_capacity(16),
        }
    }

    /// Write to a free chunk and increment its length
    ///
    /// `StorageType = T` restricts this to archetypes whose value is already in storage form.
    /// Query-only wrappers (e.g. `Disabled<Hp>`, where `StorageType = Hp`) are therefore not writable.
    pub fn push<T: TArchetype + 'static>(&mut self, layout: &ChunkLayout, e: Entity, val: T) -> Result<EntityInChunkIndices, XynokEcsError>
    {
        let free_chunk_idx = self.take_a_free_chunk_idx(layout);

        let chunk = unsafe { self.chunks.get_unchecked_mut(free_chunk_idx) };

        let idx_in_chunk = unsafe {
            let idx_in_chunk = chunk.len();
            match T::write_at(layout, chunk, idx_in_chunk, val)
            {
                Ok(_) =>
                {}
                Err(e) => return Err(e),
            };
            let dst_e = chunk.get_entity_uncheck_mut(layout, idx_in_chunk);
            *dst_e = e;
            chunk.increase_len();
            idx_in_chunk
        };

        if !chunk.is_full()
        {
            self.cache_free_chunk(free_chunk_idx);
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
        self.cache_free_chunk(chunk_idx);
        Ok(swapped_row)
    }

    /// used when calling add_component(), this archetype takes components from src_arch, adds the T value,
    /// and returns the new index along with the index that was swapped in src_arch
    pub fn take_and_write_from<T: TArchetype + 'static>(&mut self, params: ArchetypeTakeAndWriteComponentParams<T>)
        -> Result<ResultTakeAndWrite, XynokEcsError>
    {
        let free_chunk_idx = self.take_a_free_chunk_idx(params.dst_layout);

        let chunk = unsafe { self.chunks.get_unchecked_mut(free_chunk_idx) };

        let (idx_in_chunk, swapped_row_at_src_chunk) = unsafe {
            let idx_in_chunk = chunk.len();

            let src_chunk = params.src_arch.chunks.get_unchecked_mut(params.src_e.chunk_idx);
            let swapped_row = match chunk.take_from(ChunkTakeComponentParams {
                from:                 params.src_e.idx_in_chunk,
                to:                   idx_in_chunk,
                src_chunk:            src_chunk,
                src_layout:           params.src_layout,
                dst_layout:           params.dst_layout,
                component_specs:      params.component_specs,
                // `T`'s own columns are about to be written below; any old value src shares with T must be
                // dropped instead of migrated, otherwise it would be silently leaked when write_at overwrites it
                overwritten_type_ids: T::STORAGE_TYPE_IDS,
            })
            {
                Ok(r) => r,
                Err(e) =>
                {
                    return Err(e);
                }
            };

            T::write_at(params.dst_layout, chunk, idx_in_chunk, params.write_val)?;

            chunk.increase_len();
            src_chunk.decrease_len();
            params.src_arch.cache_free_chunk(params.src_e.chunk_idx);
            (idx_in_chunk, swapped_row)
        };

        if !chunk.is_full()
        {
            self.cache_free_chunk(free_chunk_idx);
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
    /// used by merge_component() when every component of `T` is already present in this archetype:
    /// overwrites the existing values of the row in place, dropping the old ones, without moving the entity
    pub fn replace_at<T: TArchetype + 'static>(&mut self, layout: &ChunkLayout, chunk_idx: usize, idx_in_chunk: usize, val: T) -> Result<(), XynokEcsError>
    {
        let chunk = unsafe { self.chunks.get_unchecked_mut(chunk_idx) };
        T::replace_at(layout, chunk, idx_in_chunk, val)
    }
    pub fn take_and_remove_from<T: TArchetype + 'static>(
        &mut self,
        params: ArchetypeTakeAndRemoveComponentParams<T>,
    ) -> Result<ResultTakeAndRemove<T>, XynokEcsError>
    {
        let free_chunk_idx = self.take_a_free_chunk_idx(params.dst_layout);

        let chunk = unsafe { self.chunks.get_unchecked_mut(free_chunk_idx) };

        let (idx_in_chunk, swapped_row_at_src_chunk, taken) = unsafe {
            let idx_in_chunk = chunk.len();

            let src_chunk = params.src_arch.chunks.get_unchecked_mut(params.src_e.chunk_idx);

            // we must get T first to avoid it being overwritten when chunk.take_from is called
            let taken = T::take_from(params.src_layout, src_chunk, params.src_e.idx_in_chunk)?;

            let swapped_row = match chunk.take_from(ChunkTakeComponentParams {
                from:                 params.src_e.idx_in_chunk,
                to:                   idx_in_chunk,
                src_chunk:            src_chunk,
                src_layout:           params.src_layout,
                dst_layout:           params.dst_layout,
                component_specs:      params.component_specs,
                overwritten_type_ids: &[],
            })
            {
                Ok(r) => r,
                Err(e) =>
                {
                    return Err(e);
                }
            };

            chunk.increase_len();
            src_chunk.decrease_len();
            params.src_arch.cache_free_chunk(params.src_e.chunk_idx);
            (idx_in_chunk, swapped_row, taken)
        };

        if !chunk.is_full()
        {
            self.cache_free_chunk(free_chunk_idx);
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
impl Archetype
{
    pub(crate) fn chunk_count(&self) -> usize
    {
        self.chunks.len()
    }
    pub(crate) fn dispose(&mut self, layout: &ChunkLayout, component_specs: &HashMap<TypeId, ComponentSpec>)
    {
        for c in self.chunks.iter_mut()
        {
            c.dispose(layout, component_specs);
        }
    }
}
impl Archetype
{
    fn take_a_free_chunk_idx(&mut self, layout: &ChunkLayout) -> usize
    {
        if let Some(free_idx) = self.free_chunks.dequeue()
        {
            self.free_chunks_stored.remove(&free_idx);
            return free_idx;
        }
        let new_chunk = Chunk::new(layout);
        let idx = self.chunks.len();
        self.chunks.push(new_chunk);
        idx
    }
    fn cache_free_chunk(&mut self, chunk_idx: usize)
    {
        if !self.free_chunks_stored.contains(&chunk_idx)
        {
            self.free_chunks_stored.insert(chunk_idx);
            self.free_chunks.enqueue(chunk_idx);
        }
    }
}
#[cfg(test)]
impl Archetype
{
    pub(crate) fn chunk_at(&self, chunk_idx: usize) -> &Chunk
    {
        &self.chunks[chunk_idx]
    }
    pub(crate) fn free_chunk_count(&self) -> usize
    {
        self.free_chunks.len()
    }
}
