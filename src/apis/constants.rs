use crate::{
    apis::{ComponentDescriptor, TComponentDescriptor},
    entity::Entity,
};

/// Fixed size for a chunk ( 16 KB / chunk)
pub const CHUNK_SIZE_IN_BYTE: usize = 16 * 1024;

pub const BITS_PER_BYTE: usize = 8;
pub const CPU_WORD: usize = std::mem::size_of::<u64>();

pub const DEFAULT_COLUMNS: &[ComponentDescriptor] = &[Entity::COMPONENT_DESCRIPTOR];
pub const ENTITY_COL_IDX: usize = 0;
