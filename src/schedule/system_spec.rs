use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScopes;
use crate::system::traits::TSystem;

/// What a schedule knows about a system, derived entirely from the system's *type*.
///
/// Everything here has to stay type-derived, because [`SystemSpecs`] hands one spec to every
/// system sharing a type. Per-instance state - when a system last ran, whether it is enabled -
/// belongs to whoever owns the system list, not here.
pub struct SystemSpec
{
    /// Written at registration and read by nothing yet - the parallel scheduler is what will
    /// pair these up through `AccessScopes::can_parallel_with`. Drop the `allow` as soon as it
    /// has a reader.
    #[allow(dead_code)]
    pub access_scopes: AccessScopes,
}

/// The schedule's system registry, keyed by system type.
///
/// A plain map rather than the [`SequenceValueHashMap`] the world uses for queries: nothing
/// hands out a pointer or index into a spec, so there is no reason to pay for stable indices.
///
/// [`SequenceValueHashMap`]: crate::collection::sequence_value_hash_map::SequenceValueHashMap
#[derive(Default)]
pub struct SystemSpecs
{
    specs: HashMap<TypeId, SystemSpec>,
}
impl SystemSpecs
{
    /// Builds the spec on first sight of a system type, and rejects a system whose parameters
    /// conflict with each other.
    ///
    /// The rejection is not about scheduling: a system's parameters all come alive before its
    /// body does, so a conflict here is two live accessors aliasing one row - unsound on a
    /// single thread just as much as on many.
    ///
    /// Building the scope registers any component the world has not seen yet, hence the mutable
    /// component registry.
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

    /// Counterpart to [`Self::register`], unused until the parallel scheduler needs to compare
    /// two systems. Drop the `allow` then.
    #[allow(dead_code)]
    pub fn get(&self, system: &dyn TSystem) -> Option<&SystemSpec>
    {
        self.specs.get(&system.system_type_id())
    }
}
