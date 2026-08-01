use std::{alloc::Layout, any::TypeId, collections::HashMap};

use crate::{
    apis::{
        constants::{BITS_PER_BYTE, CHUNK_SIZE_IN_BYTE},
        identifies::XynokEcsError,
        traits::TComponentDescriptor,
        ComponentDescriptor,
    },
    chunk::{column::ColumnDescriptor, header::Header},
    entity::Entity,
};

pub struct ChunkLayout
{
    pub max_len:                   usize,
    pub header:                    Header,
    pub alloc_layout:              Layout,
    pub component_col_descriptors: HashMap<TypeId, ColumnDescriptor>,
}

pub struct ChunkLayoutParams<'a>
{
    pub arch:                       &'a [ComponentDescriptor],
    pub component_descriptors_temp: &'a mut HashMap<TypeId, ColumnDescriptor>,
}

impl ChunkLayout
{
    pub fn new(mut params: ChunkLayoutParams) -> Result<Self, XynokEcsError>
    {
        let result = Self::compute_layout(&mut params)?;
        Ok(result)
    }
}
impl ChunkLayout
{
    fn compute_layout(params: &mut ChunkLayoutParams) -> Result<Self, XynokEcsError>
    {
        if params.arch.is_empty()
        {
            return Err(XynokEcsError::EmptyArchetype);
        }
        // Each entity costs its handle in the header plus one slot in every component column
        let bytes_per_entity = params
            .arch
            .iter()
            .fold(Entity::COMPONENT_DESCRIPTOR.byte_size, |acc, des| acc.saturating_add(des.byte_size));

        // arch.len() represents the number of components. We use this count to store the
        // enabled/disabled state of each component as a single bit
        let bits_per_entity = bytes_per_entity.saturating_mul(BITS_PER_BYTE).saturating_add(params.arch.len());

        let mut max_entities = (CHUNK_SIZE_IN_BYTE * BITS_PER_BYTE) / bits_per_entity;

        loop
        {
            if max_entities == 0
            {
                return Err(XynokEcsError::ArchetypeIsTooLarge);
            }

            if let Ok(valid_layout) = Self::try_layout(max_entities, params)
            {
                return Ok(valid_layout);
            }
            max_entities -= 1;
        }
    }

    /// Attempts to build a layout for `max_entities` rows, returns `None` if the total size exceeds [`CHUNK_SIZE_IN_BYTE`]
    fn try_layout(max_entities: usize, params: &mut ChunkLayoutParams) -> Result<ChunkLayout, XynokEcsError>
    {
        let header = Header::new(max_entities, params.arch.len());
        let mut cursor = header.size;
        let mut max_align = 0;
        params.component_descriptors_temp.clear();

        for des in params.arch
        {
            cursor = crate::chunk::header::align_up(cursor, des.align);

            if cursor > CHUNK_SIZE_IN_BYTE
            {
                return Err(XynokEcsError::ArchetypeIsTooLarge);
            }
            params.component_descriptors_temp.insert(des.storage_type_id, des.as_column_descriptor(cursor));

            let column_bytes = match des.byte_size.checked_mul(max_entities)
            {
                Some(r) => r,
                None => return Err(XynokEcsError::ArchetypeIsTooLarge),
            };
            cursor = match cursor.checked_add(column_bytes)
            {
                Some(r) => r,
                None => return Err(XynokEcsError::ArchetypeIsTooLarge),
            };
            if cursor > CHUNK_SIZE_IN_BYTE
            {
                return Err(XynokEcsError::ArchetypeIsTooLarge);
            }
            max_align = max_align.max(des.align);
        }
        let alloc_layout = match Layout::from_size_align(CHUNK_SIZE_IN_BYTE, max_align)
        {
            Ok(l) => l,
            Err(e) => return Err(XynokEcsError::ChunkLayoutAllocation(e)),
        };
        Ok(ChunkLayout {
            max_len:                   max_entities,
            component_col_descriptors: params.component_descriptors_temp.clone(),
            header:                    header,
            alloc_layout:              alloc_layout,
        })
    }
}
