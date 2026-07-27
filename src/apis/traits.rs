use std::any::TypeId;

use crate::{
    apis::{identifies::StorageLocation, ComponentDescriptor},
    chunk::{layout::ChunkLayout, Chunk},
};
pub trait TComponent: Sized
{
    type StorageDataType: TComponent + 'static;
    type QueryDataType: TComponent + 'static;
    const STORAGE_LOCATION: StorageLocation;
}
pub trait TComponentDescriptor
{
    const COMPONENT_DESCRIPTOR: ComponentDescriptor;
}
impl<T: TComponent + 'static> TComponentDescriptor for T
{
    const COMPONENT_DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
        storage_type_id:  std::any::TypeId::of::<T::StorageDataType>(),
        query_type_id:    std::any::TypeId::of::<T::QueryDataType>(),
        byte_size:        std::mem::size_of::<T::StorageDataType>(),
        align:            std::mem::align_of::<T::StorageDataType>(),
        storage_location: T::STORAGE_LOCATION,
        fn_drop:          drop_glue::<T::StorageDataType>,
    };
}

/// Drop glue: Calls the Drop implementation for T at the specified slot. This is a no-op for ZSTs or types that don't require dropping.
fn drop_glue<T>(ptr: *mut u8)
{
    unsafe {
        std::ptr::drop_in_place(ptr as *mut T);
    }
}

pub trait TArchetype
{
    const COMPONENT_DESCRIPTORS: &[ComponentDescriptor];
    const QUERY_TYPE_IDS: &[TypeId];
    const STORAGE_TYPE_IDS: &[TypeId];

    #[track_caller]
    fn push_to(layout: &ChunkLayout, chunk: &mut Chunk, val: Self);
    fn remove_at(layout: &ChunkLayout, chunk: &mut Chunk, idx: usize);
}

pub trait TSystemParam {}
pub trait TQueryParam {}
