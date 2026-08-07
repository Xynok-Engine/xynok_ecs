use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TQueryParam;
use crate::query::access_scope::AccessScope;
use crate::query::Query;
use crate::system::traits::TSystemParam;
use crate::world::World;

impl<'a, T: TQueryParam + 'static> TSystemParam for Query<'a, T>
{
    fn init(mut world: HeapMut<World>) -> Result<Self, XynokEcsError>
    {
        Query::new(&mut world)
    }

    fn access_scope() -> Result<AccessScope, XynokEcsError>
    {
        T::access_scope()
    }
}
