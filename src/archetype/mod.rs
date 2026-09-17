use std::collections::HashSet;

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::params::{
    ArchetypeTakeAndRemoveComponentParams, ArchetypeTakeAndWriteComponentParams, ChunkTakeComponentParams, EntityInChunkIndices, ResultTakeAndRemove,
    ResultTakeAndWrite, SwappedRow,
};
use crate::apis::traits::TArchetype;
use crate::chunk::layout::ChunkLayout;
use crate::chunk::Chunk;
use crate::entity::Entity;
use xynok_std::collection::Queue;

mod variant;

pub struct Archetype
{
    chunks:             Vec<Chunk>,
    free_chunks:        Queue<usize>,
    free_chunks_stored: HashSet<usize>,
    // see `Archetype::singleton`
    is_singleton:       bool,
}
impl Default for Archetype
{
    fn default() -> Self
    {
        Self {
            chunks:             Vec::with_capacity(16),
            free_chunks:        Queue::new(),
            free_chunks_stored: HashSet::with_capacity(16),
            is_singleton:       false,
        }
    }
}
impl Archetype
{
    /// An archetype that holds at most one entity at a time.
    ///
    /// Its layout is built with `max_len = 1`, so "a second entity" is the same thing as "a second
    /// chunk". `World` checks [`can_take_a_row`](Self::can_take_a_row) before pushing (it has the
    /// archetype's name for the error), and structure changes on a singleton are locked, so
    /// `take_a_free_chunk_idx` only keeps a debug assert as a backstop. Once the entity is
    /// destroyed, the chunk is free again and can be reused.
    pub fn singleton() -> Self
    {
        Self {
            chunks:             Vec::with_capacity(1),
            free_chunks:        Queue::new(),
            free_chunks_stored: HashSet::with_capacity(1),
            is_singleton:       true,
        }
    }

    #[inline]
    pub fn is_singleton(&self) -> bool
    {
        self.is_singleton
    }

    /// `false` only for a singleton that already holds its entity
    #[inline]
    pub fn can_take_a_row(&self) -> bool
    {
        !self.is_singleton || self.chunks.is_empty() || !self.free_chunks.is_empty()
    }

    /// Write to a free chunk and increment its length
    ///
    /// `StorageType = T` restricts this to archetypes whose value is already in storage form.
    /// Query-only wrappers (e.g. `Disabled<Hp>`, where `StorageType = Hp`) are therefore not writable.
    pub fn push<T: TArchetype + 'static>(&mut self, layout: &ChunkLayout, e: Entity, val: T, tick: ChangedTick) -> Result<EntityInChunkIndices, XynokEcsError>
    {
        // ask first, write second: once this passes, the `write_at` below cannot fail partway and
        // leave half of `T` in a row nobody will ever finish
        T::validate_writable(layout)?;

        let free_chunk_idx = self.take_a_free_chunk_idx(layout);

        let chunk = unsafe { self.chunks.get_unchecked_mut(free_chunk_idx) };

        let idx_in_chunk = unsafe {
            let idx_in_chunk = chunk.len();
            T::write_at(layout, chunk, idx_in_chunk, val, tick)?;
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

    pub fn remove_at(&mut self, layout: &ChunkLayout, chunk_idx: usize, idx: usize) -> Result<Option<SwappedRow>, XynokEcsError>
    {
        let chunk = match self.chunks.get_mut(chunk_idx)
        {
            Some(r) => r,
            None => return Err(XynokEcsError::ChunkIdxIsNotInRange(chunk_idx, self.chunks.len())),
        };

        let swapped_row = unsafe { chunk.swap_remove_at(layout, idx)? };
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
        // The only fallible step in this function is `T::write_at`, and it used to run *after* the
        // row had already been moved out of the source chunk, with neither `len` updated yet: an
        // error there left both chunks inconsistent (issue #29). Asking the layout up front moves
        // the one thing that can fail in front of every mutation, so the failure path is now simply
        // "nothing happened at all".
        T::validate_writable(params.dst_layout)?;

        let free_chunk_idx = self.take_a_free_chunk_idx(params.dst_layout);

        let chunk = unsafe { self.chunks.get_unchecked_mut(free_chunk_idx) };

        let (idx_in_chunk, swapped_row_at_src_chunk) = unsafe {
            let idx_in_chunk = chunk.len();

            let src_chunk = params.src_arch.chunks.get_unchecked_mut(params.src_e.chunk_idx);
            // components src shares with `T` (merge_component) are moved like any other column, state
            // included. `write_or_replace_at` below then drops the old value when it overwrites it.
            let swapped_row = match chunk.take_from(ChunkTakeComponentParams {
                from:       params.src_e.idx_in_chunk,
                to:         idx_in_chunk,
                src_chunk:  src_chunk,
                src_layout: params.src_layout,
                dst_layout: params.dst_layout,
                migration:  params.migration,
            })
            {
                Ok(r) => r,
                Err(e) =>
                {
                    return Err(e);
                }
            };

            // unreachable failure: `validate_writable` above already resolved every column this
            // touches, and nothing since then could have changed the layout
            T::write_or_replace_at(params.src_layout, params.dst_layout, chunk, idx_in_chunk, params.write_val, params.tick)?;

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
    pub fn replace_at<T: TArchetype + 'static>(
        &mut self,
        layout: &ChunkLayout,
        chunk_idx: usize,
        idx_in_chunk: usize,
        val: T,
        tick: ChangedTick,
    ) -> Result<(), XynokEcsError>
    {
        // a tuple `T` replaces its components one by one, dropping each old value as it goes; a
        // failure halfway would leave the row holding a mix of new and stale components
        T::validate_writable(layout)?;

        let chunk = unsafe { self.chunks.get_unchecked_mut(chunk_idx) };
        T::replace_at(layout, chunk, idx_in_chunk, val, tick)
    }
    pub fn take_and_remove_from<T: TArchetype + 'static>(
        &mut self,
        params: ArchetypeTakeAndRemoveComponentParams<T>,
    ) -> Result<ResultTakeAndRemove<T>, XynokEcsError>
    {
        // `T::take_from` below reads each component of `T` out of the source row as a bitwise copy.
        // If it failed partway, the components already read would be dropped by the early return
        // while the source row still holds copies of them, and those get dropped a second time
        // later. Resolving the columns first rules that out.
        T::validate_takeable(params.src_layout)?;

        let free_chunk_idx = self.take_a_free_chunk_idx(params.dst_layout);

        let chunk = unsafe { self.chunks.get_unchecked_mut(free_chunk_idx) };

        let (idx_in_chunk, swapped_row_at_src_chunk, taken) = unsafe {
            let idx_in_chunk = chunk.len();

            let src_chunk = params.src_arch.chunks.get_unchecked_mut(params.src_e.chunk_idx);

            // we must get T first to avoid it being overwritten when chunk.take_from is called
            let taken = T::take_from(params.src_layout, src_chunk, params.src_e.idx_in_chunk)?;

            let swapped_row = match chunk.take_from(ChunkTakeComponentParams {
                from:       params.src_e.idx_in_chunk,
                to:         idx_in_chunk,
                src_chunk:  src_chunk,
                src_layout: params.src_layout,
                dst_layout: params.dst_layout,
                migration:  params.migration,
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
    #[inline]
    pub(crate) fn chunk_count(&self) -> usize
    {
        self.chunks.len()
    }
    #[inline]
    pub(crate) fn chunk_at(&self, chunk_idx: usize) -> &Chunk
    {
        &self.chunks[chunk_idx]
    }
    pub(crate) fn dispose(&mut self, layout: &ChunkLayout)
    {
        for c in self.chunks.iter_mut()
        {
            c.dispose(layout);
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
        debug_assert!(!self.is_singleton || self.chunks.is_empty(), "a singleton archetype must never get a second chunk");
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
#[cfg(any(test, feature = "test-util"))]
impl Archetype
{
    pub(crate) fn free_chunk_count(&self) -> usize
    {
        self.free_chunks.len()
    }
}
