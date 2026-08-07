#![allow(unused)]
use std::marker::PhantomData;

use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScope;
use crate::world::World;

pub type SystemTypeStorage = Box<dyn TSystem>;

pub struct SystemAlias<F, P>(pub F, pub ParamAlias<P>);

/// Marker for a system's parameter set. We use `fn() -> P` instead of `P` because
/// type aliases don't actually hold parameters—they are constructed and consumed
/// inside `run`. Using `PhantomData<P>` would cause the system to inherit the
/// parameter's auto-traits, so a parameter containing a raw pointer (`Query`)
/// would break the `TSystem: Send + Sync` bound.
/// src:
/// - https://github.com/rust-lang/nomicon/issues/320
/// - https://users.rust-lang.org/t/phantomdata-t-vs-phantomdata-fn-t-t-what-about-send-and-sync/73782
pub struct ParamAlias<P>(pub PhantomData<fn() -> P>);

impl<P> Default for ParamAlias<P>
{
    fn default() -> Self
    {
        Self(PhantomData)
    }
}

/// A system after type erasure: this is what a schedule stores and calls
pub trait TSystem: Send + Sync + 'static
{
    fn name(&self) -> &'static str;

    fn run(&mut self, world: HeapMut<World>) -> Result<(), XynokEcsError>;

    fn access_scope(&self, component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>;
}
pub trait TSystemParam: Sized
{
    fn init(world: HeapMut<World>) -> Result<Self, XynokEcsError>;
    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>;
}

/// `fn` -> one system. `Marker` is param
pub trait TIntoSystem<Marker>
{
    #[track_caller]
    fn into_system(self) -> Result<SystemTypeStorage, XynokEcsError>;
}

/// `fn` or a tuple of them -> a list of boxed systems, so `schedule.add((a, b, c))` works.
pub trait TIntoSystems<P>
{
    #[track_caller]
    fn into_systems(self) -> Result<Vec<SystemTypeStorage>, XynokEcsError>;
}
