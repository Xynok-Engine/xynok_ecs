use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::apis::internal_traits::TQueryParam;
use crate::query::access_scope::AccessScopes;
use crate::query::Query;
use crate::system::traits::TSystemParam;
use crate::world::World;

impl<'a, T: TQueryParam + 'static> TSystemParam for Query<'a, T>
{
    fn init(world: HeapMut<World>, last_run_tick: ChangedTick) -> Result<Self, XynokEcsError>
    {
        // `as_ref_mut` hands back a `&mut World` detached from this local `HeapMut`, which is what
        // lets the resulting `Query<'a, _>` outlive `init` and reach the system body
        Query::new(world.as_ref_mut(), last_run_tick)
    }

    fn collect_access_scope(dst: &mut AccessScopes, component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>
    {
        dst.push(T::access_scope(component_specs)?)
    }
}
