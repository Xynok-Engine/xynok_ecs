use std::any::TypeId;

mod identifies;
pub mod fn_ptr;
mod traits;
pub mod constants;
pub(crate) mod component_spec;

pub mod swapped_row;

pub use constants::*;
pub use fn_ptr::*;
pub use identifies::*;
pub use traits::*;

pub struct ComponentDescriptor
{
    pub storage_type_id:  TypeId,
    pub query_type_id:    TypeId,
    pub byte_size:        usize,
    pub align:            usize,
    pub storage_location: StorageLocation,
    pub fn_drop:          FnComponentDropItSelf,
}
