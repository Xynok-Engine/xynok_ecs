use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::apis::internal_traits::TQueryParam;
use crate::query::access_scope::AccessScopes;
use crate::query::Query;
use crate::system::traits::TSystemParam;
use crate::world::World;

impl<'a, T: TQueryParam + 'static> TSystemParam for Query<'a, T>
{
    /// Read-only: `&World`, not `&mut World`. This runs inside a job, and several jobs building a
    /// `&mut` over the same world is a data race even when none of them writes anything.
    ///
    /// The writing half lives in [`Self::prepare`], which runs first, on one thread.
    fn init(world: HeapMut<World>) -> Result<Self, XynokEcsError>
    {
        let world: &World = world.as_ref_with_caller_lifetime();
        match Query::from_prepared(world)
        {
            Some(query) => Ok(query),
            None => Err(XynokEcsError::QueryIsNotPrepared(std::any::type_name::<T>())),
        }
    }

    fn prepare(mut world: HeapMut<World>) -> Result<(), XynokEcsError>
    {
        Query::<T>::new(&mut world).map(|_| ())
    }

    fn collect_access_scope(dst: &mut AccessScopes, component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>
    {
        dst.push(T::access_scope(component_specs)?)
    }
}
