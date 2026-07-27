use crate::{archetype::Archetype, chunk::layout::ChunkLayout};

pub struct ArchetypeSpec
{
    pub arch:   Archetype,
    pub layout: ChunkLayout,
}

impl ArchetypeSpec
{
    pub fn new(layout: ChunkLayout) -> Self
    {
        Self {
            arch:   Archetype::new(),
            layout: layout,
        }
    }
}
