use crate::apis::fn_ptr::{FnCloneComponent, FnDropComponent};

use std::any::TypeId;

mod identifies;
mod fn_ptr;
mod traits;
pub mod constants;

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
    pub fn_drop:          FnDropComponent,
    //pub fn_clone:         FnCloneComponent,
}
