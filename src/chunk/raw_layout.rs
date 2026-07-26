use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{ComponentDescriptor, TComponentDescriptor, XynokEcsError, BITS_PER_BYTE, CHUNK_SIZE_IN_BYTE, CPU_WORD, DEFAULT_COLUMNS},
    chunk::{column::ColumnDescriptor, layout::ChunkLayoutParams},
    entity::Entity,
    utils::{align_up, header_size_for},
};

pub struct RawLayout
{
    pub header_size:         usize,
    pub max_align:           usize,
    pub max_entities_amount: usize,
    pub component_indices:   HashMap<TypeId, ColumnDescriptor>,
}

impl RawLayout
{
    #[track_caller]
    pub fn new(params: &mut ChunkLayoutParams) -> Result<Self, XynokEcsError>
    {
        if params.arch.is_empty()
        {
            return Err(XynokEcsError::EmptyArchetype);
        }
        let mut bytes_per_entity = Entity::COMPONENT_DESCRIPTOR.byte_size;

        // arch.len() represents the number of components. We use this count to store the
        // enabled/disabled state of each component as a single bit
        let bits_per_entity = bytes_per_entity * BITS_PER_BYTE + params.arch.len();

        let mut max_entities = (CHUNK_SIZE_IN_BYTE * BITS_PER_BYTE) / bits_per_entity;

        loop
        {
            if max_entities == 0
            {
                return Err(XynokEcsError::ArchetypeIsTooLarge);
            }
            if let Some(valid_layout) = try_layout(max_entities, params)
            {
                return Ok(valid_layout);
            }
            max_entities -= 1;
        }
    }
}

/// Attempts to build a layout for `max_entities` rows, returns `None` if the total size exceeds [`CHUNK_SIZE_IN_BYTE`]
fn try_layout(max_entities: usize, params: &mut ChunkLayoutParams) -> Option<RawLayout>
{
    let header_size = header_size_for(max_entities, params.arch.len());
    let mut cursor = header_size;
    let mut max_align = 0;
    params.component_descriptors_temp.clear();

    for des in params.arch
    {
        cursor = align_up(cursor, des.align);

        if cursor > CHUNK_SIZE_IN_BYTE
        {
            return None;
        }
        params
            .component_descriptors_temp
            .insert(des.query_type_id, des.as_column_descriptor(cursor));

        let column_bytes = des.byte_size.checked_mul(max_entities)?;
        cursor = cursor.checked_add(column_bytes)?;
        if cursor > CHUNK_SIZE_IN_BYTE
        {
            return None;
        }
        max_align = max_align.max(des.align);
    }

    Some(RawLayout {
        max_entities_amount: max_entities,
        max_align:           max_align,
        component_indices:   params.component_descriptors_temp.clone(),
        header_size:         header_size,
    })
}
