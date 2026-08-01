use crate::apis::{
    identifies::{StorageLocation, XynokEcsError},
    traits::TComponent,
};

/// Stores an Index and Version, packed into a u64
/// Layout: bits 0..40 = idx (up to ~1 trillion slots), bits 40..64 = version (~16 million reuses per slot)
/// usecase of `#[repr(transparent)]`: https://users.rust-lang.org/t/repr-transparent-why/67636
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[repr(transparent)]
pub struct Entity(u64);

impl TComponent for Entity
{
    type StorageType = Self;

    type QueryType = Self;

    const STORAGE_LOCATION: StorageLocation = StorageLocation::Chunk;
}

impl Entity
{
    /// An empty entity: idx = 0, version = 0
    pub const NULL: Self = Self(0);

    /// To avoid conflicts with NULL, the default entity version always starts at 1
    pub const INITIALIZE_VERSION: usize = 1usize;

    /// The maximum representable index value (2^40 - 1)
    pub const MAX_IDX: usize = Self::IDX_MASK as usize;
    /// The maximum representable version value (2^24 - 1)
    pub const MAX_VERSION: usize = Self::VERSION_MASK as usize;
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
    pub fn new(idx: usize, version: usize) -> Result<Self, XynokEcsError>
    {
        if idx > Self::MAX_IDX
        {
            return Err(XynokEcsError::EntityAmountOverflow(Self::MAX_IDX));
        }
        let version = version.clamp(Self::INITIALIZE_VERSION, Self::MAX_VERSION);
        let packed = (idx as u64 & Self::IDX_MASK) | ((version as u64 & Self::VERSION_MASK) << Self::IDX_BITS);
        Ok(Self(packed))
    }

    pub fn idx(self) -> usize
    {
        (self.0 & Self::IDX_MASK) as usize
    }

    pub fn version(self) -> usize
    {
        ((self.0 >> Self::IDX_BITS) & Self::VERSION_MASK) as usize
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

#[cfg(test)]
mod test
{
    use crate::entity::Entity;

    fn new_e(v: usize) -> Entity
    {
        Entity::new(2, v).unwrap()
    }
    #[test]
    fn version_overflow()
    {
        let e = new_e((u32::MAX - 1) as usize);
        println!("INITIALIZE_VERSION: {}", Entity::INITIALIZE_VERSION);
        println!("MAX_VERSION: {}", Entity::MAX_VERSION);
        println!("ver: {}", e.version());
        debug_assert!(e.version() == Entity::MAX_VERSION);
        let e = new_e(u32::MAX as usize);
        println!("ver: {}", e.version());
        debug_assert!(e.version() == Entity::MAX_VERSION);
    }
}
