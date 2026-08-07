use std::any::TypeId;
use std::collections::{HashMap, HashSet};

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::apis::ComponentDescriptor;
use crate::archetype::Archetype;
use crate::chunk::column::ColumnDescriptor;
use crate::chunk::layout::{ChunkLayout, ChunkLayoutParams};
use crate::collection::component_bit_set::ComponentBitSet;
use crate::collection::sequence_value_hash_map::SequenceValueHashMap;

/// The world's archetype registry, keyed by archetype id.
///
/// Values live inline in the registry's dense storage, so growing it relocates them. Nothing
/// caches their addresses: `QuerySpec` stores indices instead.
pub type ArchetypeSpecs = SequenceValueHashMap<usize, ArchetypeSpec>;

pub struct ArchetypeSpec
{
    pub arch:       Archetype,
    pub layout:     ChunkLayout,
    pub archetypes: HashMap<usize, Archetype>,
}
pub struct PairArchetypeSpecParams<'a>
{
    pub a:                              &'a ArchetypeSpec,
    pub b:                              &'a ArchetypeSpec,
    pub component_specs:                &'a ComponentSpecs,
    pub temp_comp_des:                  &'a mut Vec<ComponentDescriptor>,
    pub temp_tys:                       &'a mut HashSet<TypeId>,
    pub component_col_descriptors_temp: &'a mut HashMap<TypeId, ColumnDescriptor>,
    pub component_bit_set:              &'a mut ComponentBitSet,
}

impl ArchetypeSpec
{
    pub fn new(layout: ChunkLayout) -> Self
    {
        Self {
            arch:       Archetype::default(),
            layout:     layout,
            archetypes: HashMap::new(),
        }
    }
    pub fn new_from_pair(params: PairArchetypeSpecParams) -> Result<Self, XynokEcsError>
    {
        let components_des = params.temp_comp_des;
        build_component_descriptors_from(components_des, params.component_specs, params.temp_tys, params.a, params.b)?;

        let target_layout = ChunkLayout::new(ChunkLayoutParams {
            components:                 components_des,
            component_specs:            params.component_specs,
            component_descriptors_temp: params.component_col_descriptors_temp,
            component_bit_set_temp:     params.component_bit_set,
        })?;

        Ok(Self {
            arch:       Archetype::default(),
            layout:     target_layout,
            archetypes: HashMap::new(),
        })
    }

    /// treat MergeArchetypeSpecParams.b as an exclusion
    pub fn new_from_a_exclude_b_components(params: PairArchetypeSpecParams) -> Result<Self, XynokEcsError>
    {
        let components_des = params.temp_comp_des;
        build_component_descriptors_from_src_a_exclude_src_b(components_des, params.component_specs, params.temp_tys, params.a, params.b)?;

        let target_layout = ChunkLayout::new(ChunkLayoutParams {
            components:                 components_des,
            component_specs:            params.component_specs,
            component_descriptors_temp: params.component_col_descriptors_temp,
            component_bit_set_temp:     params.component_bit_set,
        })?;

        Ok(Self {
            arch:       Archetype::default(),
            layout:     target_layout,
            archetypes: HashMap::new(),
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
    #[allow(unused)]
    pub fn contains_any_type_id_component_of(&self, other: &[TypeId]) -> bool
    {
        for e in other
        {
            if self.layout.component_col_descriptors.contains_key(e)
            {
                return true;
            }
        }
        false
    }
    pub fn contains_all_type_id_components_of(&self, other: &ComponentBitSet) -> bool
    {
        self.layout.component_bit_set.contains_all(other)
    }
}

fn build_component_descriptors_from_src_a_exclude_src_b(
    dst: &mut Vec<ComponentDescriptor>,
    component_specs: &ComponentSpecs,
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
    component_specs: &ComponentSpecs,
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
    component_specs: &ComponentSpecs,
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
