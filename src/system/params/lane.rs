use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScopes;
use crate::schedule::lane::Lane;
use crate::system::traits::TSystemParam;
use crate::world::World;

impl TSystemParam for Lane
{
    fn init(world: HeapMut<World>) -> Result<Self, XynokEcsError>
    {
        Ok(Lane { world: world })
    }

    /// Touches no component storage, so it adds nothing to the access scope.
    fn collect_access_scope(_dst: &mut AccessScopes, _component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>
    {
        Ok(())
    }
}
