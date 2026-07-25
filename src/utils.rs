use crate::apis::CPU_WORD;

#[inline(always)]
pub const fn header_size_for(max_entities: usize, component_count: usize) -> usize
{
    align_up(max_entities * component_count, CPU_WORD)
}

/// Rounds up `offset` to the nearest multiple of `align` (align must be a power of 2)
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
    use crate::utils::*;
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
