#[derive(Clone, Copy, Debug)]
pub struct EntityToChunk
{
    pub idx_in_chunk: usize,
    pub chunk_idx:    usize,
}
