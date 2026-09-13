use std::any::TypeId;

use crate::apis::custom_type::FnComponentDropItSelf;

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

/// One column in the layout's dense form.
///
/// `component_col_descriptors` (the HashMap) stays as the `TypeId` lookup used at setup time,
/// this is what the hot loops walk instead: everything needed to touch a column is right here,
/// so nothing has to go back to `ComponentSpecs` for `byte_size` and `fn_drop`.
#[derive(Clone)]
pub struct ColumnEntry
{
    pub storage_type_id: TypeId,
    pub offset:          usize,
    pub byte_size:       usize,
    pub state_offset:    StateOffset,
    pub fn_drop:         FnComponentDropItSelf,
}
