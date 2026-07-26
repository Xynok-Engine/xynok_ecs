use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{ComponentDescriptor, XynokEcsError, BYTE_TO_BIT, CHUNK_SIZE_IN_BYTE, CPU_WORD, DEFAULT_COLUMNS},
    chunk::{block::Block, ChunkLayoutParams},
};

pub struct RawLayout
{
    pub max_entities_amount: usize,
    pub max_align:           usize,
    pub blocks:              Vec<Block>,
    pub component_indices:   HashMap<TypeId, usize>,
    pub header_size:         usize,
}

impl RawLayout
{
    #[track_caller]
    pub fn new(params: &mut ChunkLayoutParams) -> Result<Self, XynokEcsError>
    {
        let mut bytes_per_entity = 0usize;
        for e in DEFAULT_COLUMNS
        {
            bytes_per_entity += e.byte_size;
        }

        // arch.len() represents the number of components. We use this count to store the
        // enabled/disabled state of each component as a single bit
        let bits_per_entity = bytes_per_entity * BYTE_TO_BIT + params.arch.len();

        debug_assert!(bits_per_entity > 0, "This Arch is empty. Cannot calculate chunk size: division by zero!");

        let mut max_entities = (CHUNK_SIZE_IN_BYTE * BYTE_TO_BIT) / bits_per_entity;

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
    let header_size = crate::utils::header_size_for(max_entities, params.arch.len());
    let total_cols = DEFAULT_COLUMNS.len() + params.arch.len();
    let mut cursor = header_size;
    let mut max_align = 0;
    params.blocks_temp.clear();
    params.indices_temp.clear();

    for des in DEFAULT_COLUMNS.iter().chain(params.arch.iter())
    {
        cursor = crate::utils::align_up(cursor, des.align);

        if cursor > CHUNK_SIZE_IN_BYTE
        {
            return None;
        }
        params.indices_temp.insert(des.query_type_id, params.blocks_temp.len());
        params.blocks_temp.push(des.as_block(cursor));

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
        blocks:              params.blocks_temp.clone(),
        component_indices:   params.indices_temp.clone(),
        header_size:         header_size,
    })
}
