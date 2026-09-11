#[derive(Clone)]
pub struct ColumnDescriptor
{
    pub offset:       usize,
    pub state_offset: StateOffset,
}
#[derive(Clone, Default)]
pub struct StateOffset
{
    pub enable_offset:  Option<usize>,
    pub added_offset:   Option<usize>,
    pub changed_offset: Option<usize>,
}
impl crate::apis::ComponentDescriptor
{
    pub fn as_column_descriptor(&self, offset: usize, state_offset: StateOffset) -> ColumnDescriptor
    {
        ColumnDescriptor {
            offset:       offset,
            state_offset: state_offset,
        }
    }
}
