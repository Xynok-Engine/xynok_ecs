use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScopes;
use crate::system::traits::TSystemParam;
use crate::world::World;

impl TSystemParam for ()
{
    /// Touches no storage, so it contributes no scope at all - an empty one would be a
    /// permanent no-op entry in every system's list
    fn collect_access_scope(_dst: &mut AccessScopes, _component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>
    {
        Ok(())
    }

    fn init(_world: HeapMut<World>) -> Result<Self, XynokEcsError>
    {
        Ok(())
    }
}
