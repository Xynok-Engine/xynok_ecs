use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::query::access_scope::AccessScope;
use crate::system::traits::{ParamAlias, SystemAlias, SystemTypeStorage, TIntoSystem, TSystem};
use crate::world::World;

impl<F: Fn() + Send + Sync + 'static> TSystem for SystemAlias<F, ()>
{
    fn name(&self) -> &'static str
    {
        std::any::type_name::<F>()
    }

    fn run(&mut self, _world: HeapMut<World>) -> Result<(), XynokEcsError>
    {
        (self.0)();
        Ok(())
    }

    fn access_scope(&self) -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope::default())
    }
}

impl<F: Fn() + Send + Sync + 'static> TIntoSystem<()> for F
{
    fn into_system(self) -> Result<SystemTypeStorage, XynokEcsError>
    {
        Ok(Box::new(SystemAlias(self, ParamAlias::<()>::default())))
    }
}
