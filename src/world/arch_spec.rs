use crate::{
    apis::{FnArchtypeRemoveEntity, TArchetype},
    archetype::Archetype,
    chunk::layout::ChunkLayout,
};

pub struct ArchetypeSpec
{
    pub arch:             Archetype,
    pub layout:           ChunkLayout,
    pub fn_remove_entity: FnArchtypeRemoveEntity,
}

impl ArchetypeSpec
{
    pub fn new<T: TArchetype + 'static>(layout: ChunkLayout) -> Self
    {
        Self {
            arch:             Archetype::new(),
            layout:           layout,
            fn_remove_entity: T::remove_at,
        }
    }
}
