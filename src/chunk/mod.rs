use std::any::TypeId;

use crate::{
    apis::{TComponent, XynokEcsError},
    chunk::layout::ChunkLayout,
    entity::Entity,
};
mod layout;
mod column;
mod header;

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
            std::ptr::write_bytes(ptr, 0u8, layout.header.size);
        }
        Self { ptr: ptr, len: 0 }
    }

    pub fn len(&self) -> usize
    {
        self.len
    }
    pub fn is_empty(&self) -> bool
    {
        self.len < 1
    }
    /// A slice `&[C]` of the component column `C` (inline) within the chunk, containing all rows
    pub fn get_components<'a, C: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<&'a [C], XynokEcsError>
    {
        let base = self.column_ptr::<C>(layout)?;
        Ok(unsafe { std::slice::from_raw_parts(base as *const C, self.len()) })
    }

    pub fn get_components_mut<'a, C: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<&'a mut [C], XynokEcsError>
    {
        let base = self.column_ptr::<C>(layout)?;
        Ok(unsafe { std::slice::from_raw_parts_mut(base as *mut C, self.len()) })
    }
    pub fn get_entities<'a>(&self, layout: &ChunkLayout) -> Result<&'a [Entity], XynokEcsError>
    {
        let entities = self.get_components::<Entity>(layout)?;
        Ok(entities)
    }
    pub fn get_entities_components<'a, C: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<(&'a [Entity], &'a [C]), XynokEcsError>
    {
        let entities = self.get_entities(layout)?;
        let components = self.get_components::<C>(layout)?;
        Ok((entities, components))
    }

    pub fn get_entities_components_mut<'a, C: TComponent + 'static>(&self, layout: &ChunkLayout)
        -> Result<(&'a [Entity], &'a mut [C]), XynokEcsError>
    {
        let entities = self.get_entities(layout)?;
        let components = self.get_components_mut::<C>(layout)?;
        Ok((entities, components))
    }
}
impl Chunk
{
    /// Drop the old value and assign the new one
    #[track_caller]
    pub(crate) fn set_value<T: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize, value: T) -> Result<(), XynokEcsError>
    {
        let col_ptr = self.column_ptr::<T>(layout)?;
        unsafe {
            let slot = (col_ptr as *mut T).add(row);
            *slot = value;
        }
        Ok(())
    }
    /// Writes directly to memory without dropping the old value. Typically used when the memory has just been initialized
    #[track_caller]
    pub(crate) fn write_value<T: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize, value: T) -> Result<(), XynokEcsError>
    {
        let col_ptr = self.column_ptr::<T>(layout)?;
        unsafe {
            let slot = (col_ptr as *mut T).add(row);
            slot.write(value);
        }
        Ok(())
    }
}
impl Chunk
{
    fn column_ptr<T: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<*mut u8, XynokEcsError>
    {
        let col_des = match layout.component_col_descriptors.get(&std::any::TypeId::of::<T::StorageDataType>())
        {
            Some(des) => des,
            None =>
            {
                return Err(XynokEcsError::ChunkDoesNotContainComponent(
                    std::any::type_name::<T::QueryDataType>(),
                    std::any::type_name::<T::StorageDataType>(),
                ));
            }
        };

        Ok(unsafe { self.ptr.add(col_des.offset) })
    }
}
