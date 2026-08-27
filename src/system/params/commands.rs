use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::cmd_buffer::Commands;
use crate::query::access_scope::AccessScopes;
use crate::system::traits::TSystemParam;
use crate::world::World;

impl TSystemParam for Commands
{
    fn init(world: HeapMut<World>) -> Result<Self, XynokEcsError>
    {
        Ok(Commands { world: world })
    }

    /// Touches no component storage, so it adds nothing to the access scope. Two systems both
    /// holding `Commands` still run in parallel: each writes into its own worker slot, and no
    /// command becomes real until the synchronisation point that ends the step.
    fn collect_access_scope(_dst: &mut AccessScopes, _component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>
    {
        Ok(())
    }
}
