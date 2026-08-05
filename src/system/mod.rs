use std::marker::PhantomData;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TSystem;
use crate::query::access_scope::AccessScope;

struct PhantomSystem<F, P>(pub F, pub PhantomData<P>);

unsafe impl<F: Send, P> Send for PhantomSystem<F, P> {}
unsafe impl<F: Sync, P> Sync for PhantomSystem<F, P> {}

impl<F: Fn() + 'static> TSystem for PhantomSystem<F, ()>
{
    fn run(&self, world: &mut crate::world::World)
    {
        self.0();
    }

    fn access_scope(&self) -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope::default())
    }
}
