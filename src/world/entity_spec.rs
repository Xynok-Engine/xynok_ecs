pub struct EntitySpec
{
    arch_id:      usize,
    chunk_idx:    usize,
    idx_in_chunk: usize,
    version:      usize,
    has_value:    bool,
}

impl EntitySpec
{
    pub fn version(&self) -> usize
    {
        self.version
    }
    pub fn arch_id(&self) -> usize
    {
        self.arch_id
    }
    pub fn idx_in_chunk(&self) -> usize
    {
        self.idx_in_chunk
    }
    pub fn chunk_idx(&self) -> usize
    {
        self.chunk_idx
    }
    pub fn has_value(&self) -> bool
    {
        self.has_value
    }
}
impl EntitySpec
{
    pub fn new_empty_slot(version: usize) -> Self
    {
        Self {
            arch_id:      0,
            chunk_idx:    0,
            idx_in_chunk: 0,
            version:      version,
            has_value:    false,
        }
    }
    pub fn new(arch_id: usize, chunk_idx: usize, idx_in_chunk: usize, version: usize) -> Self
    {
        Self {
            arch_id,
            chunk_idx,
            idx_in_chunk,
            version,
            has_value: true,
        }
    }
    pub fn errase(&mut self)
    {
        self.has_value = false;
    }

    #[track_caller]
    pub fn update_idx_in_chunk(&mut self, from: usize, to: usize)
    {
        debug_assert!(self.idx_in_chunk == from, "idx old({}) != idx new({})", self.idx_in_chunk, from);
        self.idx_in_chunk = to;
    }
}
