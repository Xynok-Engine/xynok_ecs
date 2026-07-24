use crate::chunk::layout::ChunkLayout;

pub mod layout;

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
        // Zero state header để các bit HAS_VALUE/... khởi đầu = 0. Phần còn lại buffer
        // (component data) có thể giữ uninit vì luôn được bảo vệ qua HAS_VALUE.
        // SAFETY: header_size byte đầu thuộc về buffer vừa alloc, không alias.
        unsafe {
            std::ptr::write_bytes(ptr, 0u8, spec.header_size);
        }
        Self { ptr: ptr, len: 0 }
    }
}
