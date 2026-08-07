use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use crate::apis::ComponentDescriptor;
use crate::chunk::column::ColumnDescriptor;
use crate::collection::component_bit_set::ComponentBitSet;

pub struct WorldTempAllocation
{
    pub vec_usize:           Vec<usize>,
    pub comp_descriptors:    Vec<ComponentDescriptor>,
    pub col_descriptors:     HashMap<TypeId, ColumnDescriptor>,
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
            component_bit_set_a: ComponentBitSet::with_capacity_for(8),
        }
    }
}
