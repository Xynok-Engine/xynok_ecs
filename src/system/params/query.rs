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
        // Read-only path first. In a parallel group `prepare` has already registered this query,
        // so this is the branch every job takes and no job ever builds a `&mut World`.
        if let Some(query) = Query::new_prepared(world.as_ref_with_caller_lifetime(), last_run_tick)
        {
            return Ok(query);
        }

        // Only reached on a world that has not seen this query yet, or whose archetypes moved
        // since. That happens on the single-threaded path, where an exclusive borrow is fine.
        // `as_ref_mut` hands back a `&mut World` detached from this local `HeapMut`, which is what
        // lets the resulting `Query<'a, _>` outlive `init` and reach the system body
        Query::new(world.as_ref_mut(), last_run_tick)
    }

    fn prepare(world: HeapMut<World>, _last_run_tick: ChangedTick) -> Result<(), XynokEcsError>
    {
        world.as_ref_mut().prepare_query_src_access::<T>()?;
        Ok(())
    }

    fn collect_access_scope(dst: &mut AccessScopes, component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>
    {
        dst.push(T::access_scope(component_specs)?)
    }
}
