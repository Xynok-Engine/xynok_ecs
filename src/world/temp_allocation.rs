use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use crate::apis::ComponentDescriptor;
use crate::chunk::column::{ColumnDescriptor, ColumnEntry, StateOffset};
use crate::collection::component_bit_set::ComponentBitSet;

pub struct WorldTempAllocation
{
    pub vec_usize:           Vec<usize>,
    pub comp_descriptors:    Vec<ComponentDescriptor>,
    pub col_descriptors:     HashMap<TypeId, ColumnDescriptor>,
    /// Dense column list `try_layout` fills in. It is rebuilt on every step of the layout's
    /// binary search, so it lives here instead of being allocated fresh each time.
    pub col_entries:         Vec<ColumnEntry>,
    pub state_offsets:       HashMap<TypeId, StateOffset>,
    pub hashset_type_ids:    HashSet<TypeId>,
    pub component_bit_set_a: ComponentBitSet,
}
impl WorldTempAllocation
{
    pub fn new() -> Self
    {
        Self {
            comp_descriptors:    Vec::with_capacity(16),
            hashset_type_ids:    HashSet::with_capacity(16),
            vec_usize:           Vec::with_capacity(16),
            col_descriptors:     HashMap::with_capacity(16),
            col_entries:         Vec::with_capacity(16),
            state_offsets:       HashMap::with_capacity(16),
            component_bit_set_a: ComponentBitSet::with_capacity_for(8),
        }
    }
}
