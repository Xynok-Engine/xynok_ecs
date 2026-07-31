use std::any::TypeId;

use crate::apis::ComponentDescriptor;

use crate::apis::fn_ptr::FnComponentDropItSelf;

#[derive(Clone)]
pub struct ColumnDescriptor
{
    pub ty_id:     TypeId,
    pub offset:    usize,
    pub item_size: usize,
}

impl crate::apis::ComponentDescriptor
{
    pub fn as_column_descriptor(&self, offset: usize) -> ColumnDescriptor
    {
        ColumnDescriptor {
            ty_id:     self.storage_type_id,
            item_size: self.byte_size,
            offset:    offset,
        }
    }
}
