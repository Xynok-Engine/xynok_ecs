use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{ComponentDescriptor, BYTE_TO_BIT, CHUNK_SIZE_IN_BYTE, CPU_WORD, DEFAULT_COLUMNS},
    chunk::block::Block,
};

struct RawLayout
{
    max_entities_amount: usize,
    max_align:           usize,
    blocks:              Vec<Block>,
    component_indices:   HashMap<TypeId, usize>,
    header_size:         usize,
}

impl RawLayout
{
    #[track_caller]
    pub fn new(arch: &[ComponentDescriptor]) -> Self
    {
        let mut bytes_per_entity = 0usize;
        for e in DEFAULT_COLUMNS
        {
            bytes_per_entity += e.byte_size;
        }

        // arch.len() represents the number of components. We use this count to store the
        // enabled/disabled state of each component as a single bit
        let bits_per_entity = bytes_per_entity * BYTE_TO_BIT + arch.len();

        debug_assert!(bits_per_entity > 0, "This Arch is empty. Cannot calculate chunk size: division by zero!");

        let mut max_entities = (CHUNK_SIZE_IN_BYTE * BYTE_TO_BIT) / bits_per_entity;

        loop
        {
            debug_assert!(
                max_entities > 0,
                "components are too large, chunk size of {} bytes is insufficient!",
                CHUNK_SIZE_IN_BYTE
            );
            if let Some(plan) = try_layout(max_entities, arch)
            {
                return plan;
            }
            max_entities -= 1;
        }
        todo!()
    }
}

/// Attempts to build a layout for `max_entities` rows, returns `None` if the total size exceeds `CHUNK_SIZE`
fn try_layout(max_entities: usize, components: &[ComponentDescriptor]) -> Option<RawLayout>
{
    let header_size = crate::utils::header_size_for(max_entities, components.len());
    let total_cols = DEFAULT_COLUMNS.len() + components.len();
    let mut cursor = header_size;
    let mut max_align = 0;
    let mut blocks = Vec::with_capacity(total_cols);
    let mut component_indices = HashMap::with_capacity(total_cols);

    for spec in DEFAULT_COLUMNS.iter().chain(components.iter())
    {
        cursor = crate::utils::align_up(cursor, spec.align);

        if cursor > CHUNK_SIZE_IN_BYTE
        {
            return None;
        }
        blocks.push(spec.as_block(cursor));
        component_indices.insert(spec.query_type_id, component_indices.len());
        let column_bytes = spec.byte_size.checked_mul(max_entities)?;
        cursor = cursor.checked_add(column_bytes)?;
        if cursor > CHUNK_SIZE_IN_BYTE
        {
            return None;
        }
        max_align = max_align.max(spec.align);
    }

    Some(RawLayout {
        max_entities_amount: max_entities,
        max_align:           max_align,
        blocks:              blocks,
        component_indices:   component_indices,
        header_size:         header_size,
    })
}
