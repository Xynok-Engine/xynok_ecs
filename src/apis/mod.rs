use std::any::TypeId;

pub(crate) mod params;
pub(crate) mod custom_type;
pub(crate) mod internal_traits;
pub(crate) mod safe_counter;
pub(crate) mod internal_identifies;

pub mod identifies;
pub mod constants;
pub mod traits;

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
