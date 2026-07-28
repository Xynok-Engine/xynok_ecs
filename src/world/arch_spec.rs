use crate::{
    apis::{FnRemoveEntity, TArchetype},
    archetype::Archetype,
    chunk::layout::ChunkLayout,
};

pub struct ArchetypeSpec
{
    pub arch:      Archetype,
    pub layout:    ChunkLayout,
    pub fn_remove: FnRemoveEntity,
}

impl ArchetypeSpec
{
    pub fn new<T: TArchetype + 'static>(layout: ChunkLayout) -> Self
    {
        Self {
            arch:      Archetype::new(),
            layout:    layout,
            fn_remove: T::remove_at,
        }
    }
}
