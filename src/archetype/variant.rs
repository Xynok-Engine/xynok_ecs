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

    const QUERY_TYPE_IDS: &[std::any::TypeId] = &[std::any::TypeId::of::<T::QueryDataType>()];

    const STORAGE_TYPE_IDS: &[std::any::TypeId] = &[std::any::TypeId::of::<T::StorageDataType>()];

    fn push_to(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, e: Entity, val: Self) -> Result<(), XynokEcsError>
    {
        unsafe {
            let write_idx = chunk.len();
            let e_slot = chunk.get_entity_uncheck_mut(layout, write_idx);
            *e_slot = e;
            chunk.write_at(layout, write_idx, val)
        }
    }
}
