use crate::{
    apis::{
        identifies::XynokEcsError,
        traits::{TArchetype, TComponent, TComponentDescriptor},
    },
    chunk::layout::ChunkLayout,
    entity::Entity,
};

impl<T: TComponent + 'static> TArchetype for T
{
    const COMPONENT_DESCRIPTORS: &[crate::apis::ComponentDescriptor] = &[T::COMPONENT_DESCRIPTOR];
    const QUERY_TYPE_IDS: &[std::any::TypeId] = &[std::any::TypeId::of::<T::QueryType>()];
    const STORAGE_TYPE_IDS: &[std::any::TypeId] = &[std::any::TypeId::of::<T::StorageType>()];

    fn write_at(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, write_idx: usize, val: Self) -> Result<(), XynokEcsError>
    {
        unsafe { chunk.write_at::<T>(layout, write_idx, val) }
    }

    fn take_from(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, idx: usize) -> Result<Self, XynokEcsError>
    {
        unsafe { chunk.take_at::<T>(layout, idx) }
    }
}
