use std::{any::TypeId, collections::HashMap};

use crate::{apis::ComponentDescriptor, chunk::column::ColumnDescriptor};

pub struct WorldTempAllocation
{
    pub vec_usize:       Vec<usize>,
    pub vec_comp_des:    Vec<ComponentDescriptor>,
    pub col_descriptors: HashMap<TypeId, ColumnDescriptor>,
}
impl WorldTempAllocation
{
    pub fn new() -> Self
    {
        Self {
            vec_comp_des:    Vec::with_capacity(16),
            vec_usize:       Vec::with_capacity(16),
            col_descriptors: HashMap::with_capacity(16),
        }
    }
}
