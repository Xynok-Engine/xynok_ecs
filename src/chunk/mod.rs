use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{
        component_spec::{self, ComponentSpec},
        swapped_row::SwappedRow,
        TComponent, XynokEcsError,
    },
    chunk::layout::ChunkLayout,
    entity::Entity,
};
pub(crate) mod layout;
pub(crate) mod column;
mod header;

pub struct Chunk
{
    ptr:     *mut u8,
    len:     usize,
    max_len: usize,
}

impl Chunk
{
    pub fn new(layout: &ChunkLayout) -> Self
    {
        let ptr = unsafe { std::alloc::alloc(layout.alloc_layout) };
        unsafe {
            std::ptr::write_bytes(ptr, 0u8, layout.header.size);
        }
        Self {
            ptr:     ptr,
            len:     0,
            max_len: layout.max_len,
        }
    }

    pub fn ptr(&self) -> *mut u8
    {
        self.ptr
    }
    pub fn len(&self) -> usize
    {
        self.len
    }

    pub fn is_full(&self) -> bool
    {
        self.len() < self.max_len
    }
    pub fn is_empty(&self) -> bool
    {
        self.len < 1
    }

    pub fn get_component<'a, C: TComponent + 'static>(&self, layout: &ChunkLayout, row: usize) -> Result<&'a C, XynokEcsError>
    {
        if row >= self.len()
        {
            return Err(XynokEcsError::IdxIsOutOfChunkLen(row, self.len()));
        }
        let base = self.components_ptr::<C>(layout)?;
        Ok(unsafe { &*(base as *const C).add(row) })
    }
    pub fn get_component_mut<'a, C: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize) -> Result<&'a mut C, XynokEcsError>
    {
        if row >= self.len()
        {
            return Err(XynokEcsError::IdxIsOutOfChunkLen(row, self.len()));
        }
        let base = self.components_ptr::<C>(layout)?;
        Ok(unsafe { &mut *(base as *mut C).add(row) })
    }

    /// A slice `&[C]` of the component column `C` (inline) within the chunk, containing all rows
    pub fn get_components<'a, C: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<&'a [C], XynokEcsError>
    {
        let base = self.components_ptr::<C>(layout)?;
        Ok(unsafe { std::slice::from_raw_parts(base as *const C, self.len()) })
    }

    pub fn get_components_mut<'a, C: TComponent + 'static>(&mut self, layout: &ChunkLayout) -> Result<&'a mut [C], XynokEcsError>
    {
        let base = self.components_ptr::<C>(layout)?;
        Ok(unsafe { std::slice::from_raw_parts_mut(base as *mut C, self.len()) })
    }

    pub fn get_entity<'a>(&self, layout: &ChunkLayout, row: usize) -> Result<&'a Entity, XynokEcsError>
    {
        if row >= self.len()
        {
            return Err(XynokEcsError::IdxIsOutOfChunkLen(row, self.len()));
        }
        Ok(unsafe { self.get_entity_uncheck(layout, row) })
    }
    pub fn get_entities<'a>(&self, layout: &ChunkLayout) -> Result<&'a [Entity], XynokEcsError>
    {
        unsafe {
            let entities_ptr = self.ptr.add(layout.header.entities_offset);
            Ok(std::slice::from_raw_parts(entities_ptr as *const Entity, self.len()))
        }
    }
    pub fn get_entities_components<'a, C: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<(&'a [Entity], &'a [C]), XynokEcsError>
    {
        let entities = self.get_entities(layout)?;
        let components = self.get_components::<C>(layout)?;
        Ok((entities, components))
    }

    pub fn get_entities_components_mut<'a, C: TComponent + 'static>(&mut self, layout: &ChunkLayout) -> Result<(&'a [Entity], &'a mut [C]), XynokEcsError>
    {
        let entities = self.get_entities(layout)?;
        let components = self.get_components_mut::<C>(layout)?;
        Ok((entities, components))
    }
}

impl Chunk
{
    #[track_caller]
    pub unsafe fn remove_at(
        &mut self,
        layout: &ChunkLayout,
        component_specs: &HashMap<TypeId, ComponentSpec>,
        idx: usize,
    ) -> Result<Option<SwappedRow>, XynokEcsError>
    {
        if idx >= self.len()
        {
            return Err(XynokEcsError::IdxIsOutOfChunkLen(idx, self.len()));
        }

        let last = self.len - 1;
        let is_last = idx == last;
        unsafe {
            if is_last
            {
                for (k, des) in layout.component_col_descriptors.iter()
                {
                    let target_slot = self.ptr.add(des.offset).add(idx * des.item_size);
                    let spec = component_specs.get(k).unwrap();
                    (spec.fn_drop)(target_slot);
                }
            }
            else
            {
                for (k, des) in layout.component_col_descriptors.iter()
                {
                    let target_slot = self.ptr.add(des.offset).add(idx * des.item_size);
                    let spec = component_specs.get(k).unwrap();
                    (spec.fn_drop)(target_slot);

                    let last_val = self.ptr.add(des.offset).add(last * des.item_size);
                    std::ptr::copy_nonoverlapping(last_val, target_slot, des.item_size);
                }
            }
            self.decrease_len();
        }

        if is_last
        {
            return Ok(None);
        }

        Ok(Some(unsafe {
            SwappedRow {
                e:    *self.get_entity_uncheck(layout, last),
                from: last,
                to:   idx,
            }
        }))
    }
}
impl Chunk
{
    pub(crate) unsafe fn get_entity_uncheck<'a>(&self, layout: &ChunkLayout, row: usize) -> &'a Entity
    {
        unsafe {
            let entities_ptr = self.ptr.add(layout.header.entities_offset);
            &*(entities_ptr as *const Entity).add(row)
        }
    }
    pub(crate) unsafe fn get_entity_uncheck_mut<'a>(&mut self, layout: &ChunkLayout, row: usize) -> &'a mut Entity
    {
        unsafe {
            let entities_ptr = self.ptr.add(layout.header.entities_offset);
            &mut *(entities_ptr as *mut Entity).add(row)
        }
    }
}
impl Chunk
{
    pub(crate) unsafe fn increase_len(&mut self)
    {
        self.len += 1;
    }
    pub(crate) unsafe fn decrease_len(&mut self)
    {
        self.len -= 1;
    }
    /// Drop the old value and assign the new one
    pub(crate) unsafe fn replace_at<T: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize, value: T) -> Result<(), XynokEcsError>
    {
        let col_ptr = self.components_ptr::<T>(layout)?;
        unsafe {
            let slot = (col_ptr as *mut T).add(row);
            *slot = value;
        }
        Ok(())
    }
    /// Writes directly to memory without dropping the old value. Typically used when the memory has just been initialized
    pub(crate) unsafe fn write_at<T: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize, value: T) -> Result<(), XynokEcsError>
    {
        let col_ptr = self.components_ptr::<T>(layout)?;
        unsafe {
            let slot = (col_ptr as *mut T).add(row);
            slot.write(value);
        }
        Ok(())
    }
}
impl Chunk
{
    pub(crate) fn components_ptr<T: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<*mut u8, XynokEcsError>
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
