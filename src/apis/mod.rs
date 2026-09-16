use std::any::TypeId;

pub(crate) mod params;
pub(crate) mod custom_type;
pub(crate) mod internal_traits;
pub(crate) mod safe_counter;
pub(crate) mod internal_identifies;

pub mod identifies;
pub mod constants;
pub mod traits;

/// Per-archetype settings, passed to `World::register_archetype`.
///
/// There is no `Default` on purpose: registering is where you decide how the archetype sits in
/// memory, so the chunk size has to be written out. Archetypes that are never registered (spawned
/// straight through `World::create`, or reached through `add_component`/`remove_component`) use
/// [`constants::DEFAULT_CHUNK_SIZE_IN_BYTE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchetypeCfg
{
    /// Bytes allocated per chunk. Must be greater than 0 and a multiple of
    /// [`constants::CPU_WORD`]. A chunk is also the unit a query hands to one thread, so this
    /// decides how finely the work can be split.
    pub chunk_size_in_byte: usize,
}

impl ArchetypeCfg
{
    pub fn validate(&self) -> Result<(), identifies::XynokEcsError>
    {
        if self.chunk_size_in_byte == 0 || !self.chunk_size_in_byte.is_multiple_of(constants::CPU_WORD)
        {
            return Err(identifies::XynokEcsError::InvalidChunkSize(self.chunk_size_in_byte, constants::CPU_WORD));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct ComponentDescriptor
{
    /// Resolves to the full name of `StorageType`, for error messages only. A `TypeId` cannot
    /// be turned back into a name, and every place that reports a problem only has the
    /// descriptor at hand. Reach it through [`ComponentDescriptor::name`].
    pub fn_name:          custom_type::FnComponentName,
    pub storage_type_id:  TypeId,
    pub query_type_id:    TypeId,
    pub byte_size:        usize,
    pub align:            usize,
    pub storage_location: identifies::StorageLocation,
    pub state_detection:  identifies::StateDetection,
    pub fn_drop:          custom_type::FnComponentDropItSelf,
}

impl ComponentDescriptor
{
    /// Full name of the component's `StorageType`, for error messages
    #[inline]
    pub fn name(&self) -> &'static str
    {
        (self.fn_name)()
    }
}
