#![allow(unused)]
use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScopes;
use crate::system::traits::TSystem;

pub struct SystemSpec
{
    pub access_scopes: AccessScopes,
}

#[derive(Default)]
pub struct SystemSpecs
{
    specs: HashMap<TypeId, SystemSpec>,
}
impl SystemSpecs
{
    pub fn register(&mut self, system: &dyn TSystem, component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>
    {
        let system_type = system.system_type_id();
        if self.specs.contains_key(&system_type)
        {
            return Ok(());
        }
        let access_scopes = system.access_scope(component_specs)?;
        self.specs.insert(system_type, SystemSpec { access_scopes });
        Ok(())
    }

    pub fn get(&self, system: &dyn TSystem) -> Option<&SystemSpec>
    {
        self.specs.get(&system.system_type_id())
    }
}
