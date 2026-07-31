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
    pub arch:   Archetype,
    pub layout: ChunkLayout,
}
pub struct PairArchetypeSpecParams<'a>
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
    pub fn new(layout: ChunkLayout) -> Self
    {
        Self {
            arch:   Archetype::new(),
            layout: layout,
        }
    }
    pub fn new_from_pair(params: PairArchetypeSpecParams) -> Result<Self, XynokEcsError>
    {
        let components_des = params.temp_comp_des;
        build_component_descriptors_from(components_des, params.component_specs, params.temp_tys, params.a, params.b)?;

        let target_layout = ChunkLayout::new(ChunkLayoutParams {
            arch:                       components_des,
            component_descriptors_temp: params.component_col_descriptors_temp,
        })?;

        Ok(Self {
            arch:   Archetype::new(),
            layout: target_layout,
        })
    }

    /// treat MergeArchetypeSpecParams.b as an exclusion
    pub fn new_from_a_exclude_b_components(params: PairArchetypeSpecParams) -> Result<Self, XynokEcsError>
    {
        let components_des = params.temp_comp_des;
        build_component_descriptors_from_src_a_exclude_src_b(components_des, params.component_specs, params.temp_tys, params.a, params.b)?;

        let target_layout = ChunkLayout::new(ChunkLayoutParams {
            arch:                       components_des,
            component_descriptors_temp: params.component_col_descriptors_temp,
        })?;

        Ok(Self {
            arch:   Archetype::new(),
            layout: target_layout,
        })
    }
}
impl ArchetypeSpec
{
    pub fn contains_any_component_of(&self, other: &ArchetypeSpec) -> bool
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
    pub fn contains_all_components_of(&self, other: &ArchetypeSpec) -> bool
    {
        for e in other.layout.component_col_descriptors.keys()
        {
            if !self.layout.component_col_descriptors.contains_key(e)
            {
                return false;
            }
        }
        true
    }
}

fn build_component_descriptors_from_src_a_exclude_src_b(
    dst: &mut Vec<ComponentDescriptor>,
    component_specs: &HashMap<TypeId, ComponentSpec>,
    temp_tys: &mut HashSet<TypeId>,
    src_a: &ArchetypeSpec,
    src_b: &ArchetypeSpec,
) -> Result<(), XynokEcsError>
{
    dst.clear();
    temp_tys.clear();
    for e in src_a.layout.component_col_descriptors.keys()
    {
        if temp_tys.contains(e) || src_b.layout.component_col_descriptors.contains_key(e)
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
fn build_component_descriptors_from(
    dst: &mut Vec<ComponentDescriptor>,
    component_specs: &HashMap<TypeId, ComponentSpec>,
    temp_tys: &mut HashSet<TypeId>,
    src_a: &ArchetypeSpec,
    src_b: &ArchetypeSpec,
) -> Result<(), XynokEcsError>
{
    dst.clear();
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
