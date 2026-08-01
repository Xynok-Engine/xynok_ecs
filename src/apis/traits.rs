use std::any::TypeId;

use crate::{
    apis::{
        identifies::{StorageLocation, XynokEcsError},
        ComponentDescriptor,
    },
    chunk::{layout::ChunkLayout, Chunk},
    query::access_scope::AccessScope,
};
pub trait TComponent: Sized
{
    type QueryType: TComponent + 'static;
    type StorageType: TComponent + 'static;
    const STORAGE_LOCATION: StorageLocation;
}
pub trait TComponentDescriptor
{
    const COMPONENT_DESCRIPTOR: ComponentDescriptor;
}
impl<T: TComponent + 'static> TComponentDescriptor for T
{
    const COMPONENT_DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
        storage_type_id:  std::any::TypeId::of::<T::StorageType>(),
        query_type_id:    std::any::TypeId::of::<T::QueryType>(),
        byte_size:        std::mem::size_of::<T::StorageType>(),
        align:            std::mem::align_of::<T::StorageType>(),
        storage_location: T::STORAGE_LOCATION,
        fn_drop:          drop_glue::<T::StorageType>,
    };
}

/// Drop glue: Calls the Drop implementation for T at the specified slot. This is a no-op for ZSTs or types that don't require dropping.
fn drop_glue<T>(ptr: *mut u8)
{
    unsafe {
        std::ptr::drop_in_place(ptr as *mut T);
    }
}

pub trait TArchetype: Sized
{
    const COMPONENT_DESCRIPTORS: &[ComponentDescriptor];
    const QUERY_TYPE_IDS: &[TypeId];
    const STORAGE_TYPE_IDS: &[TypeId];

    fn write_at(layout: &ChunkLayout, chunk: &mut Chunk, write_idx: usize, val: Self) -> Result<(), XynokEcsError>;
    fn take_from(layout: &ChunkLayout, chunk: &mut Chunk, idx: usize) -> Result<Self, XynokEcsError>;
    /// Drops the old values already stored at `row` and writes `val` in their place. Used when merging
    /// components into an entity whose archetype already contains every component of `Self`.
    fn replace_at(layout: &ChunkLayout, chunk: &mut Chunk, row: usize, val: Self) -> Result<(), XynokEcsError>;

    //fn remove_at(
    //    layout: &ChunkLayout,
    //    component_specs: &HashMap<TypeId, ComponentSpec>,
    //    chunk: &mut Chunk,
    //    idx: usize,
    //) -> Result<Option<SwappedRow>, XynokEcsError>
    //{
    //    unsafe {
    //        match chunk.swap_remove_at(layout, component_specs, idx)
    //        {
    //            Ok(r) => Ok(r),
    //            Err(e) => Err(e),
    //        }
    //    }
    //}
}

pub trait TSystemParam {}
pub trait TQueryParam
{
    fn access_scope() -> Result<AccessScope, XynokEcsError>;
}
