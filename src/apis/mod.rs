use crate::apis::{
    fn_ptr::{FnCloneComponent, FnDropComponent},
    identifies::StorageLocation,
};
use std::any::TypeId;

mod fn_ptr;
pub use fn_ptr::*;

mod constants;
pub use constants::*;

mod identifies;
pub use identifies::*;

mod traits;
pub use traits::*;

pub struct ComponentDescriptor
{
    pub storage_type_id:  TypeId,
    pub query_type_id:    TypeId,
    pub byte_size:        usize,
    pub align:            usize,
    pub storage_location: StorageLocation,
    pub fn_drop:          FnDropComponent,
    //pub fn_clone:         FnCloneComponent,
}
