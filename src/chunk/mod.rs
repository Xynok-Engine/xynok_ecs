use std::any::TypeId;

use crate::{
    apis::{TComponent, XynokEcsError},
    chunk::layout::ChunkLayout,
};
mod layout;
mod raw_layout;
mod column;

pub struct Chunk
{
    ptr: *mut u8,
    len: usize,
}

impl Chunk
{
    pub fn new(layout: &ChunkLayout) -> Self
    {
        let ptr = unsafe { std::alloc::alloc(layout.alloc_layout) };
        unsafe {
            std::ptr::write_bytes(ptr, 0u8, layout.header_size);
        }
        Self { ptr: ptr, len: 0 }
    }

    pub fn column_ptr<T: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<*mut u8, XynokEcsError>
    {
        let col_idx = match layout.component_indices.get(&std::any::TypeId::of::<T::StorageDataType>())
        {
            Some(idx) => idx,
            None =>
            {
                return Err(XynokEcsError::ChunkDoesNotContainComponent(
                    std::any::type_name::<T::QueryDataType>(),
                    std::any::type_name::<T::StorageDataType>(),
                ));
            }
        };
        todo!()
    }
}
