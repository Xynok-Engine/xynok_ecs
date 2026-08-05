use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::identifies::{SystemError, XynokEcsError};
use crate::apis::params::ComponentSpec;
use crate::apis::traits::TComponent;
use crate::query::access_scope::AccessScope;
use crate::world::arch_spec::ArchetypeSpec;
use crate::world::query_spec::QuerySpecAccessor;
use crate::world::unsafe_world_cell::UnsafeWorldCell;
use crate::world::World;

pub type SystemTypeStorage = Box<dyn TSystem>;
pub trait TQuerySrcAccess
{
    fn new(arch: *mut Vec<*mut ArchetypeSpec>, specs: *const HashMap<TypeId, ComponentSpec>) -> Self;
}
pub trait TQueryParam
{
    type QueryItem<'a>;
    type SrcAccess<'a>: TQuerySrcAccess;
    const TYPE_ID: TypeId;
    fn access_scope() -> Result<AccessScope, XynokEcsError>;
    #[track_caller]
    fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>;
    fn build_src_access<'a>(src_access: &QuerySpecAccessor) -> Self::SrcAccess<'a>
    {
        Self::SrcAccess::new(src_access.archetypes, src_access.component_specs)
    }
}

pub trait TQueryColumn: TQueryParam
{
    type Component: TComponent + 'static;
    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize) -> Self::QueryItem<'a>;
}

/// What a system body is allowed to return.
///
/// Two shapes are accepted: `()` for a system that cannot fail, and `Result<(), E>` for one that
/// can, with `E` being anything convertible into [`XynokError`] — which, thanks to its blanket
/// `From`, is every `std::error::Error`. That is what lets a system body use `?` on an io error
/// without the ECS knowing anything about io.
pub trait TSystemOutput
{
    fn into_system_result(self) -> Result<(), SystemError>;
}

impl TSystemOutput for ()
{
    #[inline]
    fn into_system_result(self) -> Result<(), SystemError>
    {
        Ok(())
    }
}

impl<E: Into<SystemError>> TSystemOutput for Result<(), E>
{
    #[inline]
    fn into_system_result(self) -> Result<(), SystemError>
    {
        self.map_err(Into::into)
    }
}

/// One argument of a system function.
///
/// Split in two halves on purpose:
/// - `State` is what survives between runs. A `Query`'s archetype list costs a world lookup to
///   resolve, and a system runs every frame, so resolving it once and keeping it here is the whole
///   point of the split.
/// - `Item<'w>` is what the function body actually receives, borrowed from the state for the
///   duration of a single run.
pub trait TSystemParam: Sized
{
    type State: 'static;
    type Item<'w>;

    fn init_state(world: &mut World) -> Result<Self::State, XynokEcsError>;

    /// Declared up front so conflicts (`Query<&mut Hp>` twice in one system) are caught at
    /// `into_system()` time rather than on the first run.
    fn access_scope() -> Result<AccessScope, XynokEcsError>;

    /// # Safety
    /// Every param fetched against the same cell must have had its [`AccessScope`] checked against
    /// the others first, otherwise this hands out aliasing `&mut`s into the same column.
    unsafe fn fetch<'w>(state: &'w mut Self::State, world: UnsafeWorldCell<'w>) -> Result<Self::Item<'w>, XynokEcsError>;
}

/// The bridge from a plain `fn`/closure to something the ECS can call.
///
/// `Marker` exists only to let one `Func` type carry a different impl per arity: without it, the
/// impls for `fn(A)` and `fn(A, B)` would overlap as far as coherence is concerned. It is never
/// constructed — see `FunctionSystem`'s `PhantomData<fn() -> Marker>`.
pub trait TSystemParamFunction<Marker>: Send + Sync + 'static
{
    type Param: TSystemParam;
    type Out: TSystemOutput;

    fn run(&mut self, params: <Self::Param as TSystemParam>::Item<'_>) -> Self::Out;
}

/// A system after type erasure: this is what a schedule stores and calls.
pub trait TSystem: Send + Sync + 'static
{
    fn name(&self) -> &'static str;

    /// Resolves param state against `world`. Idempotent, so a schedule may call it once up front
    /// and `run` will not redo the work.
    fn init(&mut self, world: &mut World) -> Result<(), XynokEcsError>;

    fn run(&mut self, world: &mut World) -> Result<(), SystemError>;

    /// The union of every param's scope, computed once at construction. A scheduler reads this to
    /// decide which systems may run in parallel.
    fn access_scope(&self) -> &AccessScope;
}

/// `fn` -> one system.
pub trait TIntoSystem<Marker>
{
    type System: TSystem;
    #[track_caller]
    fn into_system(self) -> Result<Self::System, XynokEcsError>;
}

/// `fn` or a tuple of them -> a list of boxed systems, so `schedule.add((a, b, c))` works.
pub trait TIntoSystems<P>
{
    #[track_caller]
    fn into_systems(self) -> Result<Vec<SystemTypeStorage>, XynokEcsError>;
}
