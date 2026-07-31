use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use crate::{apis::ComponentDescriptor, chunk::column::ColumnDescriptor};

pub struct WorldTempAllocation
{
    pub vec_usize:        Vec<usize>,
    pub comp_descriptors: Vec<ComponentDescriptor>,
    pub col_descriptors:  HashMap<TypeId, ColumnDescriptor>,
    pub hashset_type_ids: HashSet<TypeId>,
}
impl WorldTempAllocation
{
    pub fn new() -> Self
    {
        Self {
            comp_descriptors: Vec::with_capacity(16),
            hashset_type_ids: HashSet::with_capacity(16),
            vec_usize:        Vec::with_capacity(16),
            col_descriptors:  HashMap::with_capacity(16),
        }
    }
}
