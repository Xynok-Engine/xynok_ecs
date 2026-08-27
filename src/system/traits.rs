#![allow(unused)]
use std::any::TypeId;
use std::marker::PhantomData;

use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::{AccessScope, AccessScopes};
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

    /// Identity of the underlying `fn`, for keying a schedule's system registry. Deliberately
    /// not called `type_id`: `Any` gives every `'static` type a method of that name, and the
    /// two would be ambiguous at the call site.
    fn system_type_id(&self) -> TypeId;

    fn run(&mut self, world: HeapMut<World>) -> Result<(), XynokEcsError>;

    /// Performs everything `run` would **write** into the `World`, on exactly one thread.
    ///
    /// Initialising a [`crate::query::Query`] is not read-only: the first time it meets a query
    /// type, the world registers components it has never seen, inserts a new `QuerySpec`, and
    /// rebuilds the query's archetype list whenever new archetypes appear. Letting that happen
    /// inside a job means two systems of the same group writing into one registry.
    ///
    /// The scheduler calls this for the whole group before spawning. After that, `init` inside a
    /// job is a table lookup, which is a read, and concurrent reads are fine.
    fn prepare(&self, world: HeapMut<World>) -> Result<(), XynokEcsError>;

    /// One entry per parameter, never merged into a single scope - [`AccessScopes`] explains
    /// what merging would throw away
    fn access_scope(&self, component_specs: &mut ComponentSpecs) -> Result<AccessScopes, XynokEcsError>;
}
pub trait TSystemParam: Sized
{
    fn init(world: HeapMut<World>) -> Result<Self, XynokEcsError>;

    /// Runs [`Self::init`]'s writing half up front, where only one thread is around. See
    /// [`TSystem::prepare`].
    ///
    /// The default builds one and throws it away: every parameter is cheap to build, and that
    /// build is precisely what performs the writes that need to be kept on one thread.
    fn prepare(world: HeapMut<World>) -> Result<(), XynokEcsError>
    {
        Self::init(world).map(|_| ())
    }

    /// Adds what this parameter accesses to `dst`, which rejects it if it conflicts with a
    /// parameter already registered. A parameter that touches no component storage adds
    /// nothing rather than an empty [`AccessScope`].
    fn collect_access_scope(dst: &mut AccessScopes, component_specs: &mut ComponentSpecs) -> Result<(), XynokEcsError>;
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
