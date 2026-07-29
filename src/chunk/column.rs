use crate::apis::ComponentDescriptor;

use crate::apis::fn_ptr::FnComponentDropItSelf;
#[derive(Clone, Copy)]
pub struct ColumnDescriptor
{
    pub offset:    usize,
    pub item_size: usize,
}

impl crate::apis::ComponentDescriptor
{
    pub fn as_column_descriptor(&self, offset: usize) -> ColumnDescriptor
    {
        ColumnDescriptor {
            offset:    offset,
            item_size: self.byte_size,
        }
    }
}
