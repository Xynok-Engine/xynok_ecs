use crate::apis::constants::{BITS_PER_BYTE, CPU_WORD};
use crate::apis::traits::TComponentDescriptor;
use crate::entity::Entity;
use crate::utils::align_up;

pub struct Header
{
    pub entities_offset: usize,
    pub size:            usize,
}
impl Header
{
    pub fn new(max_entities: usize, component_count: usize) -> Self
    {
        // enable value bits
        let bit_count = max_entities * component_count;
        let bitset_size = align_up(bit_count.div_ceil(BITS_PER_BYTE), CPU_WORD);

        // entities
        let entities_offset = align_up(bitset_size, Entity::COMPONENT_DESCRIPTOR.align);
        let entities_size = max_entities * Entity::COMPONENT_DESCRIPTOR.byte_size;

        let size = align_up(entities_offset + entities_size, CPU_WORD);
        Self {
            entities_offset: entities_offset,
            size:            size,
        }
    }
}
