use std::alloc::Layout;

use crate::apis::ComponentDescriptor;

pub struct ChunkLayout
{
    pub max_len:      usize,
    pub header_size:  usize,
    pub alloc_layout: Layout,
}
impl ChunkLayout{

    #[track_caller]
    pub fn new(arch: &[ComponentDescriptor]) -> Self
    {

        debug_assert!(!arch.is_empty(), "ChunkLayout must contains at least one component");

        Self{
            max_len:0,
            header_size:0,
            alloc_layout:
        }
    }
}
