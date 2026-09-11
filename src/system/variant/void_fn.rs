use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::constants::ChangedTick;
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
        (self.func)();
        Ok(())
    }

    fn access_scope(&self, _component_specs: &mut ComponentSpecs) -> Result<AccessScopes, XynokEcsError>
    {
        Ok(AccessScopes::default())
    }

    fn prepare(&self, _world: HeapMut<World>) -> Result<(), XynokEcsError>
    {
        Ok(())
    }

    fn last_run_tick(&self) -> ChangedTick
    {
        self.last_run_tick
    }
    fn set_last_run_tick(&mut self, tick: ChangedTick)
    {
        self.last_run_tick = tick;
    }
}

impl<F: Fn() + Send + Sync + 'static> TIntoSystem<()> for F
{
    fn into_system(self) -> Result<SystemTypeStorage, XynokEcsError>
    {
        Ok(Box::new(SystemAlias {
            func:          self,
            params:        ParamAlias::<()>::default(),
            last_run_tick: 0,
        }))
    }
}
