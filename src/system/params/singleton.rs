use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TQueryColumn;
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TArchetype;
use crate::query::access_scope::AccessScopes;
use crate::query::singleton::Singleton;
use crate::system::traits::TSystemParam;
use crate::world::World;

impl<'a, P> TSystemParam for Singleton<'a, P>
where
    P: TQueryColumn + 'static,
    P::Component: TArchetype + 'static,
{
    fn init(world: HeapMut<World>, last_run_tick: ChangedTick) -> Result<Self, XynokEcsError>
    {
        // Read-only, for the same reason `Query`'s `init` is: the parameters before this one may
        // already hold shared borrows into the world, so a `&mut World` next to them is UB.
        Singleton::new(world.as_ref_with_caller_lifetime(), last_run_tick)
    }

    fn prepare(_world: HeapMut<World>, _last_run_tick: ChangedTick) -> Result<(), XynokEcsError>
    {
        // Nothing to warm up: a singleton reaches its archetype by type id, so there is no query
        // spec to register and no archetype list to refresh.
        Ok(())
    }

    fn collect_access_scope(dst: &mut AccessScopes, component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>
    {
        // The same scope a `Query<P>` would declare, so the scheduler keeps this parameter away
        // from anyone else touching the component, singleton or not.
        dst.push(P::access_scope(component_specs)?)
    }
}
