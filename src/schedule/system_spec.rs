#![allow(unused)]
use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScopes;
use crate::system::traits::{SystemTypeStorage, TSystem};

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
    pub fn check_group_can_parallel(&self, group: &[SystemTypeStorage]) -> Result<(), XynokEcsError>
    {
        for (i, a) in group.iter().enumerate()
        {
            let a_spec = match self.get(a.as_ref())
            {
                Some(r) => r,
                None => return Err(XynokEcsError::SystemSpecIsNotRegistered(a.name())),
            };

            for b in &group[i + 1..]
            {
                let b_spec = match self.get(b.as_ref())
                {
                    Some(r) => r,
                    None => return Err(XynokEcsError::SystemSpecIsNotRegistered(b.name())),
                };

                if !a_spec.access_scopes.can_parallel_with(&b_spec.access_scopes)
                {
                    return Err(XynokEcsError::ParallelGroupConflict(a.name(), b.name()));
                }
            }
        }
        Ok(())
    }
}
