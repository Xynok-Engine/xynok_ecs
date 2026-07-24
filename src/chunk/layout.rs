use std::alloc::Layout;

pub struct ChunkLayout
{
    pub max_len:      usize,
    pub header_size:  usize,
    pub alloc_layout: Layout,
}
