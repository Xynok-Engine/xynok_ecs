use crate::apis::identifies::{StateDetection, StorageLocation, XynokEcsError};
use crate::apis::traits::TComponent;

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

    type MutPolicy = crate::query::mut_ref::NoTracking;

    const STORAGE_LOCATION: StorageLocation = StorageLocation::Chunk;

    const STATE_DETECTION: crate::apis::identifies::StateDetection = StateDetection::None;
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
    /// # Errors
    /// `version` past [`Self::MAX_VERSION`] is refused rather than clamped. Clamping is what
    /// makes a recycled slot hand back a handle identical to the one it just retired: every
    /// stale copy of that handle would start passing [`World::exists`] again and address
    /// whoever lives in the slot now. The world avoids ever asking for that by retiring a slot
    /// once its version runs out, see `World::erase_entity`.
    ///
    /// The lower end is still clamped up to [`Self::INITIALIZE_VERSION`], which is not the same
    /// kind of mistake: version `0` belongs to [`Self::NULL`], so a live handle simply starts
    /// at `1`.
    ///
    /// [`World::exists`]: crate::world::World::exists
    pub fn new(idx: usize, version: usize) -> Result<Self, XynokEcsError>
    {
        if idx > Self::MAX_IDX
        {
            return Err(XynokEcsError::EntityAmountOverflow(Self::MAX_IDX));
        }
        if version > Self::MAX_VERSION
        {
            return Err(XynokEcsError::EntityVersionOverflow(idx, Self::MAX_VERSION));
        }
        let version = version.max(Self::INITIALIZE_VERSION);
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
    use crate::apis::identifies::XynokEcsError;
    use crate::entity::Entity;

    /// A version past the 24 bits on offer is refused, not folded back onto `MAX_VERSION`.
    /// Folding it is what used to let a recycled slot reissue a handle it had already given
    /// out once.
    #[test]
    fn version_past_the_maximum_is_refused()
    {
        assert!(Entity::new(2, Entity::MAX_VERSION).is_ok(), "the maximum itself is still a valid version");

        for version in [Entity::MAX_VERSION + 1, (u32::MAX - 1) as usize, u32::MAX as usize]
        {
            match Entity::new(2, version)
            {
                Err(XynokEcsError::EntityVersionOverflow(idx, max)) =>
                {
                    assert_eq!((idx, max), (2, Entity::MAX_VERSION));
                }
                Err(e) => panic!("wrong error for version {version}: {e}"),
                Ok(e) => panic!("version {version} must not silently become {}", e.version()),
            }
        }
    }

    /// The low end is a different story: `0` is `NULL`'s, so a live handle starts at `1`
    #[test]
    fn version_below_the_minimum_is_lifted_to_one()
    {
        let e = Entity::new(2, 0).unwrap();
        assert_eq!(e.version(), Entity::INITIALIZE_VERSION);
        assert_ne!(e, Entity::NULL);
    }

    #[test]
    fn pack_roundtrip()
    {
        for (idx, version) in [(0usize, 1usize), (1, 1), (42, 7), (1_000_000, 3), (Entity::MAX_IDX, 5)]
        {
            let e = Entity::new(idx, version).unwrap();
            assert_eq!(e.idx(), idx, "idx must survive packing");
            assert_eq!(e.version(), version, "version must survive packing");
        }
    }

    #[test]
    fn max_bounds_are_representable()
    {
        let e = Entity::new(Entity::MAX_IDX, Entity::MAX_VERSION).unwrap();
        assert_eq!(e.idx(), Entity::MAX_IDX);
        assert_eq!(e.version(), Entity::MAX_VERSION);
    }

    #[test]
    fn null_is_distinct_from_any_live_handle()
    {
        assert_eq!(Entity::NULL.raw(), 0);
        assert_eq!(Entity::default(), Entity::NULL);
        // A live handle always carries version >= 1, so it can never collide with NULL.
        assert_ne!(Entity::new(0, Entity::INITIALIZE_VERSION).unwrap(), Entity::NULL);
    }
}
