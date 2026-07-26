use crate::apis::{StorageLocation, TComponent};

/// Stores an Index and Version, packed into a u64
/// Layout: bits 0..40 = idx (up to ~1 trillion slots), bits 40..64 = version (~16 million reuses per slot)
/// usecase of `#[repr(transparent)]`: https://users.rust-lang.org/t/repr-transparent-why/67636
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct Entity(u64);

impl TComponent for Entity
{
    type StorageDataType = Self;

    type QueryDataType = Self;

    const STORAGE_LOCATION: StorageLocation = StorageLocation::Chunk;
}

impl Entity
{
    /// An empty entity: idx = 0, version = 0.
    pub const NULL: Self = Self(0);

    /// To avoid conflicts with NULL, the default entity version always starts at 1
    pub const INITIALIZE_VERSION: u32 = 1u32;

    /// The maximum representable index value (2^40 - 1)
    pub const MAX_IDX: usize = Self::IDX_MASK as usize;
    /// The maximum representable version value (2^24 - 1)
    pub const MAX_VERSION: u32 = Self::VERSION_MASK as u32;
}
impl Entity
{
    const IDX_BITS: u32 = 40;
    const VERSION_BITS: u32 = 24;
    const IDX_MASK: u64 = (1u64 << Self::IDX_BITS) - 1;
    const VERSION_MASK: u64 = (1u64 << Self::VERSION_BITS) - 1;
}
impl Entity
{
    #[track_caller]
    pub fn new(idx: usize, version: u32) -> Self
    {
        debug_assert!(idx <= Self::MAX_IDX, "Entity idx overflow: {} > {}", idx, Self::MAX_IDX);
        debug_assert!(
            version <= Self::MAX_VERSION,
            "Entity version overflow: {} > {}",
            version,
            Self::MAX_VERSION
        );
        let packed = (idx as u64 & Self::IDX_MASK) | ((version as u64 & Self::VERSION_MASK) << Self::IDX_BITS);
        Self(packed)
    }

    pub fn idx(self) -> usize
    {
        (self.0 & Self::IDX_MASK) as usize
    }

    pub fn version(self) -> u32
    {
        ((self.0 >> Self::IDX_BITS) & Self::VERSION_MASK) as u32
    }

    pub fn raw(self) -> u64
    {
        self.0
    }
}
impl Default for Entity
{
    fn default() -> Self
    {
        Self::NULL
    }
}
impl std::fmt::Display for Entity
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        write!(f, "Entity(idx={}, version={})", self.idx(), self.version())
    }
}
