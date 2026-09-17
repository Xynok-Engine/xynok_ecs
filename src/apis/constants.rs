/// Chunk size used when an archetype is created without an explicit [`ArchetypeCfg`](crate::apis::ArchetypeCfg),
/// e.g. spawned straight through `World::create` or produced by `add_component` ( 16 KB / chunk)
pub const DEFAULT_CHUNK_SIZE_IN_BYTE: usize = 16 * 1024;
pub const BITS_PER_BYTE: usize = 8;
pub const CPU_WORD: usize = std::mem::size_of::<u64>();

pub type ChangedTick = u32;
pub const CHANGED_TICK_BYTE_SIZE: usize = std::mem::size_of::<ChangedTick>();
