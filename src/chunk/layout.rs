use std::{alloc::Layout, any::TypeId, collections::HashMap};

use crate::{
    apis::{ComponentDescriptor, XynokEcsError, CHUNK_SIZE_IN_BYTE},
    chunk::{column::Column, raw_layout::RawLayout},
};

pub struct ChunkLayout
{
    pub max_len:           usize,
    pub header_size:       usize,
    pub alloc_layout:      Layout,
    pub blocks:            Vec<Column>,
    pub component_indices: HashMap<TypeId, usize>,
}

pub struct ChunkLayoutParams<'a>
{
    pub arch:         &'a [ComponentDescriptor],
    pub blocks_temp:  &'a mut Vec<Column>,
    pub indices_temp: &'a mut HashMap<TypeId, usize>,
}

impl ChunkLayout
{
    pub fn new(mut params: ChunkLayoutParams) -> Result<Self, XynokEcsError>
    {
        if params.arch.is_empty()
        {
            return Err(XynokEcsError::EmptyArchetype);
        }
        let raw_layout = RawLayout::new(&mut params)?;

        let alloc_layout = match Layout::from_size_align(CHUNK_SIZE_IN_BYTE, raw_layout.max_align)
        {
            Ok(l) => l,
            Err(e) => return Err(XynokEcsError::ChunkLayoutAllocation(e)),
        };

        Ok(Self {
            max_len:           raw_layout.max_entities_amount,
            header_size:       raw_layout.header_size,
            alloc_layout:      alloc_layout,
            blocks:            raw_layout.cols,
            component_indices: raw_layout.component_indices,
        })
    }
}
