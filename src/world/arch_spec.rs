use std::{any::TypeId, collections::HashMap};

use crate::{
    apis::{component_spec::ComponentSpec, fn_ptr::FnArchtypeRemoveEntity, identifies::XynokEcsError, traits::TArchetype, ComponentDescriptor},
    archetype::Archetype,
    chunk::{column::ColumnDescriptor, layout::ChunkLayout},
};

pub struct ArchetypeSpec
{
    pub arch:             Archetype,
    pub layout:           ChunkLayout,
    pub fn_remove_entity: Vec<FnArchtypeRemoveEntity>,
}
pub struct MergeArchetypeSpecParams<'a>
{
    a:               &'a ArchetypeSpec,
    b:               &'a ArchetypeSpec,
    component_specs: &'a HashMap<TypeId, ComponentSpec>,
    temp_comp_des:   &'a mut Vec<ComponentDescriptor>,
}

impl ArchetypeSpec
{
    pub fn new<T: TArchetype + 'static>(layout: ChunkLayout) -> Self
    {
        Self {
            arch:             Archetype::new(),
            layout:           layout,
            fn_remove_entity: vec![T::remove_at],
        }
    }
    pub fn new_from(params: MergeArchetypeSpecParams) -> Result<Self, XynokEcsError>
    {
        todo!()
    }
}

fn append_component_descriptor_to(dst: &mut Vec<ComponentDescriptor>, src: &ChunkLayout)
{
    for e in src.component_col_descriptors.values()
    {
        //dst.push(e.component_des.clone());
    }
}
