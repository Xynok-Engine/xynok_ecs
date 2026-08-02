use crate::world::arch_spec::ArchetypeSpec;

pub struct SrcAccess
{
    pub archetypes:        *mut *mut ArchetypeSpec,
    pub total_arch:        usize,
    pub current_arch_idx:  usize,
    pub current_chunk_len: usize,
    pub current_chunk_idx: usize,
}
