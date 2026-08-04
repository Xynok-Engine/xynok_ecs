use std::alloc::Layout;
use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::constants::{BITS_PER_BYTE, CHUNK_SIZE_IN_BYTE, CPU_WORD};
use crate::apis::identifies::XynokEcsError;
use crate::chunk::column::ColumnDescriptor;
use crate::chunk::layout::ChunkLayoutParams;
use crate::utils::align_up;

pub struct SharedChunkLayout
{
    pub header:                    SharedChunkHeader,
    pub alloc_layout:              Layout,
    pub component_col_descriptors: HashMap<TypeId, ColumnDescriptor>,
}
pub struct SharedChunkHeader
{
    pub size: usize,
}

impl SharedChunkLayout
{
    /// use for shared component, `params` must be archetype component
    pub fn new(params: &mut ChunkLayoutParams) -> Result<Self, XynokEcsError>
    {
        #[cfg(debug_assertions)]
        {
            use crate::apis::identifies::StorageLocation;
            if params.components.iter().any(|e| e.storage_location != StorageLocation::Archetype)
            {
                return Err(XynokEcsError::ThisArchetypeDoesNotContainShareAbleComponent);
            }
        }

        let result = create_fit_layout(params)?;
        Ok(result)
    }
}
impl SharedChunkHeader
{
    pub fn new(component_count: usize) -> Self
    {
        // enable value bits
        let bit_count = component_count;
        let bitset_size = align_up(bit_count.div_ceil(BITS_PER_BYTE), CPU_WORD);

        Self { size: bitset_size }
    }
}
fn create_fit_layout(params: &mut ChunkLayoutParams) -> Result<SharedChunkLayout, XynokEcsError>
{
    let header = SharedChunkHeader::new(params.components.len());
    let mut cursor = header.size;
    // Header's own bitset requires CPU_WORD alignment, so this is the floor even when
    // the archetype has no components (and thus no des.align to fold over)
    let mut max_align = CPU_WORD;
    params.component_descriptors_temp.clear();

    for des in params.components
    {
        cursor = align_up(cursor, des.align);

        if cursor > CHUNK_SIZE_IN_BYTE
        {
            return Err(XynokEcsError::ArchetypeIsTooLarge);
        }
        params.component_descriptors_temp.insert(des.storage_type_id, des.as_column_descriptor(cursor));

        let column_bytes = match des.byte_size.checked_mul(1)
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
    let total_size = align_up(cursor, max_align);
    let alloc_layout = match Layout::from_size_align(total_size, max_align)
    {
        Ok(l) => l,
        Err(e) => return Err(XynokEcsError::ChunkLayoutAllocation(e)),
    };
    Ok(SharedChunkLayout {
        component_col_descriptors: params.component_descriptors_temp.clone(),
        header:                    header,
        alloc_layout:              alloc_layout,
    })
}
