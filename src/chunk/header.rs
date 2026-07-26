use crate::{
    apis::{TComponentDescriptor, BITS_PER_BYTE, CPU_WORD},
    entity::Entity,
};

pub struct Header
{
    pub entities_offset: usize,
    pub size:            usize,
}
impl Header
{
    pub const fn new(max_entities: usize, component_count: usize) -> Self
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

/// Rounds up `offset` to the nearest multiple of `align` (align must be a power of 2)
#[inline(always)]
pub const fn align_up(offset: usize, align: usize) -> usize
{
    (offset + align - 1) & !(align - 1)
}

/// Maps a `row` to a bitmask within a `u64` word (a `row` can exceed 63 when the header contains multiple words or flags)
#[inline(always)]
pub const fn row_to_bit_mask(row: usize) -> u64
{
    1u64 << (row % CPU_WORD)
}

#[cfg(test)]
mod test
{
    use crate::chunk::header::*;
    #[test]
    fn t_align_up()
    {
        assert!(align_up(45, 64) == 64);
        assert!(align_up(2, 8) == 8);
        assert!(align_up(8, 8) == 8);
        assert!(align_up(9, 8) == 16);
        assert!(align_up(1024, 64) == 1024);
        assert!(align_up(1021, 64) == 1024);
    }
}
