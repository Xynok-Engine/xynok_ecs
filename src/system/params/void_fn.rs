use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::query::access_scope::AccessScope;
use crate::system::traits::TSystemParam;
use crate::world::World;

impl TSystemParam for ()
{
    fn access_scope() -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope::default())
    }

    fn init(_world: HeapMut<World>) -> Result<Self, XynokEcsError>
    {
        Ok(())
    }
}
