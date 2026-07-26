#[derive(Clone, Copy)]
pub struct Column
{
    pub offset:    usize,
    pub item_size: usize,
}

impl crate::apis::ComponentDescriptor
{
    pub fn as_block(&self, offset: usize) -> Column
    {
        Column {
            offset:    offset,
            item_size: self.byte_size,
        }
    }
}
