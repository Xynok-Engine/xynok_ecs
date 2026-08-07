use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScope;
use crate::system::traits::TSystemParam;
use crate::world::World;

impl TSystemParam for ()
{
    fn access_scope(_component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope::default())
    }

    fn init(_world: HeapMut<World>) -> Result<Self, XynokEcsError>
    {
        Ok(())
    }
}
