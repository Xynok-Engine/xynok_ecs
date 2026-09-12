#![allow(unused)]
use std::any::TypeId;
use std::marker::PhantomData;

use xynok_std::unsafe_ptr::HeapMut;

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::{AccessScope, AccessScopes};
use crate::world::World;

pub type SystemTypeStorage = Box<dyn TSystem>;

pub struct SystemAlias<F, P>
{
    pub func:          F,
    pub params:        ParamAlias<P>,
    /// The tick as of this system's previous run. Starts at `0`, the same "never touched" value
    /// change-detection storage is zeroed to, so this system's first run always treats every
    /// `Added`/`Changed` row as new.
    pub last_run_tick: ChangedTick,
}

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

    /// Warm up the system's query parameters. We must ensure all accessed data is available when the system starts running
    fn prepare(&self, world: HeapMut<World>) -> Result<(), XynokEcsError>;

    fn run(&mut self, world: HeapMut<World>) -> Result<(), XynokEcsError>;
    /// One entry per parameter, never merged into a single scope - [`AccessScopes`] explains
    /// what merging would throw away
    fn access_scope(&self, component_specs: &mut ComponentSpecs) -> Result<AccessScopes, XynokEcsError>;

    /// The world tick as of this system's previous run (`0` if it has never run). A `Changed`/
    /// `Added` query filter compares a row's tick against this to know whether it happened
    /// since *this system* last looked, independently of how often any other system runs.
    fn last_run_tick(&self) -> ChangedTick;
    fn set_last_run_tick(&mut self, tick: ChangedTick);
}
pub trait TSystemParam: Sized
{
    /// `last_run_tick` is this system's own `last_run_tick` (see [`TSystem::last_run_tick`]),
    /// threaded straight through instead of stashed in `World`: a parallel group runs several
    /// systems' `init` concurrently, so there is no single shared "current system" to read back
    /// from `World` without racing.
    fn init(world: HeapMut<World>, last_run_tick: ChangedTick) -> Result<Self, XynokEcsError>;

    /// Does whatever this parameter needs an exclusive `&mut World` for, so that a later `init`
    /// only ever reads. The scheduler calls this on its own thread before a parallel group
    /// starts, which is what keeps the jobs from aliasing the world.
    fn prepare(world: HeapMut<World>, last_run_tick: ChangedTick) -> Result<(), XynokEcsError>;

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
