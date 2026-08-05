use std::any::TypeId;

pub(crate) mod params;
pub(crate) mod custom_type;
pub(crate) mod internal_traits;
pub(crate) mod safe_counter;

pub mod identifies;
pub mod constants;
pub mod traits;

#[derive(Clone)]
pub struct ComponentDescriptor
{
    pub storage_type_id:  TypeId,
    pub query_type_id:    TypeId,
    pub byte_size:        usize,
    pub align:            usize,
    pub storage_location: identifies::StorageLocation,
    pub fn_drop:          custom_type::FnComponentDropItSelf,
}
