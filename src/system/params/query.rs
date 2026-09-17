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
        // Only the read-only build. Every parameter of the system was prepared before `run`, so
        // building this query never needs a `&mut World`. That matters: an earlier parameter may
        // already hold shared borrows into the world, and a `&mut World` next to them is UB.
        match Query::new_prepared(world.as_ref_with_caller_lifetime(), last_run_tick)
        {
            Some(query) => Ok(query),
            None => panic!(
                "query `{}` was not prepared before its system ran, call `TSystem::prepare` first",
                std::any::type_name::<T>()
            ),
        }
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
