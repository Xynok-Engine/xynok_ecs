/// Fixed size for a chunk ( 16 KB / chunk)
pub const CHUNK_SIZE_IN_BYTE: usize = 16 * 1024;
pub const BITS_PER_BYTE: usize = 8;
pub const CPU_WORD: usize = std::mem::size_of::<u64>();

pub type ChangedTick = u32;
pub const CHANGED_TICK_BYTE_SIZE: usize = std::mem::size_of::<ChangedTick>();
