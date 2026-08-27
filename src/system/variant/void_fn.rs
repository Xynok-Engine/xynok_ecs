use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScopes;
use crate::system::traits::{ParamAlias, SystemAlias, SystemTypeStorage, TIntoSystem, TSystem};
use crate::world::World;

impl<F: Fn() + Send + Sync + 'static> TSystem for SystemAlias<F, ()>
{
    fn name(&self) -> &'static str
    {
        std::any::type_name::<F>()
    }

    fn system_type_id(&self) -> std::any::TypeId
    {
        std::any::TypeId::of::<F>()
    }

    fn run(&mut self, _world: HeapMut<World>) -> Result<(), XynokEcsError>
    {
        (self.0)();
        Ok(())
    }

    fn prepare(&self, _world: HeapMut<World>) -> Result<(), XynokEcsError>
    {
        Ok(())
    }

    fn access_scope(&self, _component_specs: &mut ComponentSpecs) -> Result<AccessScopes, XynokEcsError>
    {
        Ok(AccessScopes::default())
    }
}

impl<F: Fn() + Send + Sync + 'static> TIntoSystem<()> for F
{
    fn into_system(self) -> Result<SystemTypeStorage, XynokEcsError>
    {
        Ok(Box::new(SystemAlias(self, ParamAlias::<()>::default())))
    }
}
