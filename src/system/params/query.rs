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
    fn init(mut world: HeapMut<World>) -> Result<Self, XynokEcsError>
    {
        Query::new(&mut world)
    }

    fn collect_access_scope(dst: &mut AccessScopes, component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>
    {
        dst.push(T::access_scope(component_specs)?)
    }
}
