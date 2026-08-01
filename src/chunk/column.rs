#[derive(Clone)]
pub struct ColumnDescriptor
{
    pub offset: usize,
}

impl crate::apis::ComponentDescriptor
{
    pub fn as_column_descriptor(&self, offset: usize) -> ColumnDescriptor
    {
        ColumnDescriptor { offset: offset }
    }
}
