use crate::apis::ComponentDescriptor;

use crate::apis::fn_ptr::FnDropComponent;
#[derive(Clone, Copy)]
pub struct ColumnDescriptor
{
    pub offset:    usize,
    pub item_size: usize,
    pub fn_drop:   FnDropComponent,
}

impl crate::apis::ComponentDescriptor
{
    pub fn as_column_descriptor(&self, offset: usize) -> ColumnDescriptor
    {
        ColumnDescriptor {
            offset:    offset,
            item_size: self.byte_size,
            fn_drop:   self.fn_drop,
        }
    }
}
