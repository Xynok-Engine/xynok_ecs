use crate::apis::{SIZE_PER_FLAG_BOX, WORD_SIZE_BYTE};

/// Số u64 word cần thiết để chứa `max_entities` bit cho MỘT flag.
#[inline(always)]
pub const fn row_state_box_count_per_flag_for(max_entities: usize) -> usize
{
    max_entities.div_ceil(SIZE_PER_FLAG_BOX)
}

/// Số u64 word cần thiết để chứa `max_entities * total_component` bit cho MỘT flag.
#[inline(always)]
pub const fn col_state_box_count_per_flag_for(max_entities: usize, component_count: usize) -> usize
{
    (max_entities * component_count).div_ceil(SIZE_PER_FLAG_BOX)
}

#[inline(always)]
pub const fn header_size_for(max_entities: usize, component_count: usize) -> usize
{
    (max_entities + component_count) * WORD_SIZE_BYTE
}

/// làm tròn `offset` lên bội số gần nhất của `align` (align phải là luỹ thừa 2)
pub const fn align_up(offset: usize, align: usize) -> usize
{
    (offset + align - 1) & !(align - 1)
}

/// Map `row` -> bit mask trong u64 word chứa nó (row có thể > 63 khi header có nhiều word/flag).
#[inline(always)]
pub const fn row_to_bit_mask(row: usize) -> u64
{
    1u64 << (row % SIZE_PER_FLAG_BOX) // idx bit thực của row: (row % ROWS_PER_WORD)
}
