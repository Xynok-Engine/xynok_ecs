use crate::{
    apis::{TArchetype, TComponent, TComponentDescriptor},
    chunk::layout::ChunkLayout,
};

impl<T: TComponent + 'static> TArchetype for T
{
    const COMPONENT_DESCRIPTORS: &[crate::apis::ComponentDescriptor] = &[T::COMPONENT_DESCRIPTOR];

    const QUERY_TYPE_IDS: &[std::any::TypeId] = &[std::any::TypeId::of::<T::QueryDataType>()];

    const STORAGE_TYPE_IDS: &[std::any::TypeId] = &[std::any::TypeId::of::<T::StorageDataType>()];

    fn push_to(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, val: Self)
    {
        unsafe {
            match chunk.push(layout, val)
            {
                Ok(_) =>
                {}
                Err(e) => panic!("Failed to push Archetype `{}` to chunk", std::any::type_name::<T::StorageDataType>()),
            };
        }
    }

    fn remove_at(layout: &ChunkLayout, chunk: &mut crate::chunk::Chunk, idx: usize)
    {
        todo!()
    }
}
