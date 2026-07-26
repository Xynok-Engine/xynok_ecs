use std::{any::TypeId, collections::HashMap};

use crate::chunk::column::ColumnDescriptor;

pub struct WorldTempAllocation
{
    pub vec_usize:       Vec<usize>,
    pub col_descriptors: HashMap<TypeId, ColumnDescriptor>,
}
impl WorldTempAllocation
{
    pub fn new() -> Self
    {
        Self {
            vec_usize:       Vec::with_capacity(64),
            col_descriptors: HashMap::with_capacity(16),
        }
    }
}
