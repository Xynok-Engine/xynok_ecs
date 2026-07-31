use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use crate::{
    apis::{fn_ptr::FnArchtypeRemoveEntity, identifies::XynokEcsError, params::ComponentSpec, traits::TArchetype, ComponentDescriptor},
    archetype::Archetype,
    chunk::{
        column::ColumnDescriptor,
        layout::{self, ChunkLayout, ChunkLayoutParams},
    },
};

pub struct ArchetypeSpec
{
    pub arch:             Archetype,
    pub layout:           ChunkLayout,
    pub fn_remove_entity: Vec<FnArchtypeRemoveEntity>,
}
pub struct MergeArchetypeSpecParams<'a>
{
    pub a:                              &'a ArchetypeSpec,
    pub b:                              &'a ArchetypeSpec,
    pub component_specs:                &'a HashMap<TypeId, ComponentSpec>,
    pub temp_comp_des:                  &'a mut Vec<ComponentDescriptor>,
    pub temp_tys:                       &'a mut HashSet<TypeId>,
    pub component_col_descriptors_temp: &'a mut HashMap<TypeId, ColumnDescriptor>,
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
        let components_des = params.temp_comp_des;
        components_des.clear();
        build_component_descriptors_from(components_des, params.component_specs, params.temp_tys, params.a, params.b)?;

        let target_layout = ChunkLayout::new(ChunkLayoutParams {
            arch:                       components_des,
            component_descriptors_temp: params.component_col_descriptors_temp,
        })?;

        let mut fns_remove: Vec<FnArchtypeRemoveEntity> = Vec::new();
        fns_remove.extend(&params.a.fn_remove_entity);
        fns_remove.extend(&params.b.fn_remove_entity);

        Ok(Self {
            arch:             Archetype::new(),
            layout:           target_layout,
            fn_remove_entity: fns_remove,
        })
    }
}
impl ArchetypeSpec
{
    pub fn contains_component_of(&self, other: &ArchetypeSpec) -> bool
    {
        for e in self.layout.component_col_descriptors.keys()
        {
            if other.layout.component_col_descriptors.contains_key(e)
            {
                return true;
            }
        }
        false
    }
}
fn build_component_descriptors_from(
    dst: &mut Vec<ComponentDescriptor>,
    component_specs: &HashMap<TypeId, ComponentSpec>,
    temp_tys: &mut HashSet<TypeId>,
    src_a: &ArchetypeSpec,
    src_b: &ArchetypeSpec,
) -> Result<(), XynokEcsError>
{
    temp_tys.clear();
    append_component_descriptor_to(dst, temp_tys, component_specs, src_a)?;
    append_component_descriptor_to(dst, temp_tys, component_specs, src_b)?;
    Ok(())
}
fn append_component_descriptor_to(
    dst: &mut Vec<ComponentDescriptor>,
    temp_tys: &mut HashSet<TypeId>,
    component_specs: &HashMap<TypeId, ComponentSpec>,
    src: &ArchetypeSpec,
) -> Result<(), XynokEcsError>
{
    for e in src.layout.component_col_descriptors.keys()
    {
        if temp_tys.contains(e)
        {
            continue;
        }
        match component_specs.get(e)
        {
            Some(comp_des) =>
            {
                dst.push(comp_des.descriptor.clone());
                temp_tys.insert(*e);
            }
            None => return Err(XynokEcsError::ComponentSpecIsNotRegistered),
        }
    }
    Ok(())
}
