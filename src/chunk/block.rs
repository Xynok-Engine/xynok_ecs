pub struct Block
{
    pub offset:    usize,
    pub item_size: usize,
}

impl crate::apis::ComponentDescriptor
{
    pub fn as_block(&self, offset: usize) -> Block
    {
        Block {
            offset:    offset,
            item_size: self.byte_size,
        }
    }
}
