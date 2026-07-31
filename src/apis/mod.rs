use std::any::TypeId;

pub(crate) mod component_spec;
pub(crate) mod params;
pub(crate) mod fn_ptr;
pub(crate) mod swapped_row;

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
    pub fn_drop:          fn_ptr::FnComponentDropItSelf,
}
