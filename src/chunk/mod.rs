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
        let spec = unsafe { &*layout };

        // SAFETY: CHUNK_SIZE > 0
        let ptr = unsafe { std::alloc::alloc(spec.alloc_layout) };
        unsafe {
            std::ptr::write_bytes(ptr, 0u8, spec.header_size);
        }
        Self { ptr: ptr, len: 0 }
    }
}
