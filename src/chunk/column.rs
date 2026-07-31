use std::any::TypeId;

use crate::apis::ComponentDescriptor;

use crate::apis::fn_ptr::FnComponentDropItSelf;

#[derive(Clone)]
pub struct ColumnDescriptor
{
    pub offset: usize,
    pub ty_id:  TypeId,
}

impl crate::apis::ComponentDescriptor
{
    pub fn as_column_descriptor(&self, offset: usize) -> ColumnDescriptor
    {
        ColumnDescriptor {
            ty_id:  self.storage_type_id,
            offset: offset,
        }
    }
}
