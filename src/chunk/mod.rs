use crate::chunk::layout::ChunkLayout;

mod layout;
mod raw_layout;
mod block;

pub use layout::*;

pub struct Chunk
{
    ptr: *mut u8,
    len: usize,
}

impl Chunk
{
    pub fn new(layout: *const ChunkLayout) -> Self
    {
        let chunk_layout = unsafe { &*layout };

        let ptr = unsafe { std::alloc::alloc(chunk_layout.alloc_layout) };
        unsafe {
            std::ptr::write_bytes(ptr, 0u8, chunk_layout.header_size);
        }
        Self { ptr: ptr, len: 0 }
    }
}
