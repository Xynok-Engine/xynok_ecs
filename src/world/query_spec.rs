use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::params::ComponentSpec;
use crate::query::access_scope::AccessScope;
use crate::world::arch_spec::ArchetypeSpec;

pub struct QuerySpec
{
    pub archetypes:   Vec<*mut ArchetypeSpec>,
    pub access_scope: AccessScope,
    pub version:      usize,
}
#[derive(Clone, Copy)]
pub struct QuerySpecAccessor
{
    pub archetypes:      *mut Vec<*mut ArchetypeSpec>,
    pub component_specs: *const HashMap<TypeId, ComponentSpec>,
}
impl QuerySpec
{
    pub fn as_accessor(&mut self, component_specs: *const HashMap<TypeId, ComponentSpec>) -> QuerySpecAccessor
    {
        QuerySpecAccessor {
            archetypes:      &mut self.archetypes as *mut _,
            component_specs: component_specs,
        }
    }
}
impl QuerySpecAccessor
{
    pub fn len(&self) -> usize
    {
        unsafe { (*self.archetypes).len() }
    }

    pub fn as_mut_ptr(&self) -> *mut *mut ArchetypeSpec
    {
        unsafe { (*self.archetypes).as_mut_ptr() }
    }
}
