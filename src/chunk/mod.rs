use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{
        identifies::XynokEcsError,
        params::{ChunkTakeComponentParams, ComponentSpec, SwappedRow},
        traits::TComponent,
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
        self.len() >= self.max_len
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
    /// Fetch data from another chunk and incorporate it into the current one
    /// Returns the swapped indices of an entity in the chunk that was taken and subsequently swapped
    pub(crate) unsafe fn take_from(&mut self, params: ChunkTakeComponentParams) -> Result<Option<SwappedRow>, XynokEcsError>
    {
        let last = params.src_chunk.len() - 1;
        let is_last = params.from == last;
        unsafe {
            for (k, src_col_des) in params.src_layout.component_col_descriptors.iter()
            {
                let spec = params.component_specs.get(k).unwrap();
                let item_size = spec.descriptor.byte_size;
                let src_slot = params.src_chunk.ptr().add(src_col_des.offset).add(params.from * item_size);
                let src_last_val = params.src_chunk.ptr.add(src_col_des.offset).add(last * item_size);

                // when removing a component, the dst often won't have all the components from the src
                let dst_col_des = match params.dst_layout.component_col_descriptors.get(k)
                {
                    Some(r) => r,
                    None =>
                    {
                        if !is_last
                        {
                            std::ptr::copy_nonoverlapping(src_last_val, src_slot, item_size);
                        }
                        continue;
                    }
                };

                let dst_slot = self.ptr().add(dst_col_des.offset).add(params.to * item_size);
                std::ptr::copy_nonoverlapping(src_slot, dst_slot, item_size);

                if !is_last
                {
                    std::ptr::copy_nonoverlapping(src_last_val, src_slot, item_size);
                }
            }
            let src_e = params.src_chunk.get_entity_uncheck_mut(params.src_layout, params.from);
            let dst_e = self.get_entity_uncheck_mut(params.dst_layout, params.to);
            *dst_e = *src_e;
            *src_e = match is_last
            {
                true => Entity::NULL,
                false => *params.src_chunk.get_entity_uncheck(params.src_layout, last),
            };
        }

        if is_last
        {
            return Ok(None);
        }

        let swapped = unsafe {
            SwappedRow {
                // get the entity from the `src.from` because we already swapped it
                e:    *params.src_chunk.get_entity_uncheck(params.src_layout, params.from),
                from: last,
                to:   params.from,
            }
        };
        Ok(Some(swapped))
    }
    pub(crate) unsafe fn swap_remove_at(
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
            for (k, des) in layout.component_col_descriptors.iter()
            {
                let spec = component_specs.get(k).unwrap();
                let item_size = spec.descriptor.byte_size;
                let target_slot = self.ptr.add(des.offset).add(idx * item_size);
                (spec.descriptor.fn_drop)(target_slot);

                if !is_last
                {
                    let last_val = self.ptr.add(des.offset).add(last * item_size);
                    std::ptr::copy_nonoverlapping(last_val, target_slot, item_size);
                }
            }
            let src_e = self.get_entity_uncheck_mut(layout, idx);
            *src_e = match is_last
            {
                true => Entity::NULL,
                false => *self.get_entity_uncheck(layout, last),
            };
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
    pub(crate) unsafe fn take_at<T: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize) -> Result<T, XynokEcsError>
    {
        let col_ptr = self.components_ptr::<T>(layout)?;
        unsafe {
            let slot = (col_ptr as *mut T).add(row);
            Ok(slot.read())
        }
    }

    pub(crate) fn dispose(&mut self, layout: &ChunkLayout, component_specs: &HashMap<TypeId, ComponentSpec>)
    {
        for (k, des) in layout.component_col_descriptors.iter()
        {
            if let Some(spec) = component_specs.get(k)
            {
                let mut counter = 0usize;
                while counter < self.len()
                {
                    let slot = unsafe { self.ptr().add(des.offset).add(counter * spec.descriptor.byte_size) };
                    (spec.descriptor.fn_drop)(slot);
                    counter += 1;
                }
            }
        }

        let alloc_layout = layout.alloc_layout;
        if alloc_layout.size() != 0
        {
            unsafe {
                std::alloc::dealloc(self.ptr, alloc_layout);
            }
        }
        self.len = 0;
    }
}
impl Chunk
{
    fn components_ptr<T: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<*mut u8, XynokEcsError>
    {
        let col_des = match layout.component_col_descriptors.get(&std::any::TypeId::of::<T::StorageType>())
        {
            Some(des) => des,
            None =>
            {
                return Err(XynokEcsError::ChunkDoesNotContainComponent(
                    std::any::type_name::<T::QueryType>(),
                    std::any::type_name::<T::StorageType>(),
                ));
            }
        };

        Ok(unsafe { self.ptr.add(col_des.offset) })
    }
}
