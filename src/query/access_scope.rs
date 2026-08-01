use std::any::TypeId;

use crate::{apis::identifies::XynokEcsError, world::arch_spec::ArchetypeSpec};

#[derive(Default, Clone)]
pub struct AccessScope
{
    pub read:    Vec<TypeId>,
    pub write:   Vec<TypeId>,
    pub exclude: Vec<TypeId>,
}
impl AccessScope
{
    pub fn extend(&mut self, other: AccessScope) -> Result<(), XynokEcsError>
    {
        if self.collide_with(&other)
        {
            return Err(XynokEcsError::QueryAccessScopeConflict);
        }
        self.read.extend(other.read);
        self.write.extend(other.write);
        Ok(())
    }

    pub fn is_read_only(&self) -> bool
    {
        self.write.is_empty()
    }

    pub fn belong_to(&self, arch: &ArchetypeSpec) -> bool
    {
        arch.contains_all_type_id_components_of(&self.read) && arch.contains_all_type_id_components_of(&self.write)
    }

    pub fn collide_with(&self, other: &AccessScope) -> bool
    {
        self.read.iter().any(|e| other.write.contains(e)) // read while other write
        || self.write.iter().any(|e| other.read.contains(e)) // write while other read
        || self.write.iter().any(|e| other.write.contains(e)) // both write on the same
    }
}
