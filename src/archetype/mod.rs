use std::collections::HashMap;

use crate::{
    apis::TArchetype,
    archetype::entity_to_chunk::EntityToChunk,
    chunk::{
        layout::{self, ChunkLayout},
        Chunk,
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
pub struct ArchetypeComponentSpec
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

    pub fn push<T: TArchetype + 'static>(&mut self, layout: &ChunkLayout, val: T) -> ArchetypeComponentSpec
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

        T::push_to(layout, chunk, val);

        if !chunk.is_full()
        {
            self.free_chunks.enqueue(free_chunk_idx);
        }
        ArchetypeComponentSpec {
            chunk_idx:    free_chunk_idx,
            idx_in_chunk: chunk.len() - 1,
        }
    }
}
