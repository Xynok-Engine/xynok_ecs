use crate::{
    apis::{ComponentDescriptor, TComponentDescriptor},
    entity::Entity,
};

/// Fixed size for a chunk ( 16 KB / chunk)
pub const CHUNK_SIZE_IN_BYTE: usize = 16 * 1024;
// A u64 holds 64 bits (64 rows)
pub const SIZE_PER_FLAG_BOX: usize = 64;
/// 1 byte = 8 bits
pub const BYTE_TO_BIT: usize = 8;
pub const STATE_HEADER_ALIGN: usize = std::mem::align_of::<u64>();
pub const WORD_SIZE_BYTE: usize = std::mem::size_of::<u64>();
pub const ENTITY_COL_IDX: usize = 0;

pub const DEFAULT_COLUMNS: &[ComponentDescriptor] = &[Entity::COMPONENT_DESCRIPTOR];
