use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScopes;

use crate::system::traits::{ParamAlias, SystemAlias, SystemTypeStorage, TIntoSystem, TSystem, TSystemParam};

use crate::world::World;
use xynok_std::unsafe_ptr::HeapMut;

macro_rules! mutiple_param_system {
    ($($name:ident),* $(,)?) =>
    {
        impl<F $(, $name)*> TSystem for SystemAlias<F ,($($name,)*)>
        where F: Fn($($name,)*) + Send + Sync + 'static
              $(,$name: TSystemParam + 'static)*
        {
            fn name(&self) -> &'static str
            {
                std::any::type_name::<F>()
            }
            fn system_type_id(&self) -> std::any::TypeId
            {
                std::any::TypeId::of::<F>()
            }
            fn access_scope(&self, component_specs: &mut ComponentSpecs) -> Result<AccessScopes, XynokEcsError>
            {
                let mut result = AccessScopes::default();
                $($name::collect_access_scope(&mut result, component_specs)?;)*
                Ok(result)
            }
            fn run(&mut self, world: HeapMut<World>)-> Result<(), XynokEcsError>
            {
                (self.0)($($name::init(world)?,)*);
                Ok(())
            }
            fn prepare(&self, world: HeapMut<World>) -> Result<(), XynokEcsError>
            {
                $($name::prepare(world)?;)*
                Ok(())
            }
        }

        impl<F $(, $name)*> TIntoSystem<($($name,)*)> for F
        where F: Fn($($name,)*) + Send + Sync + 'static
              $(,$name: TSystemParam + 'static)*
        {
            fn into_system(self) -> Result<SystemTypeStorage, XynokEcsError>
            {
                Ok(Box::new(SystemAlias(self, ParamAlias::<($($name,)*)>::default())))
            }
        }
    };
}

#[rustfmt::skip] mutiple_param_system!(S0);
#[rustfmt::skip] mutiple_param_system!(S0, S1);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6, S7);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6, S7, S8);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6, S7, S8, S9);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, S12);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, S12, S13);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, S12, S13, S14);
#[rustfmt::skip] mutiple_param_system!(S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, S12, S13, S14, S15);
