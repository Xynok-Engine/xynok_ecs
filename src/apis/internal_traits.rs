use std::any::TypeId;

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponent;
use crate::chunk::layout::ChunkLayout;
use crate::query::access_scope::AccessScope;

/// The two ticks every filter compares against: this system's own last run, and the world's
/// tick as of this run (which is also the tick a write gets stamped with).
#[derive(Clone, Copy)]
pub struct QueryTicks
{
    pub last_run: ChangedTick,
    pub this_run: ChangedTick,
}

/// A query shape (`&T`, `&mut T`, `Changed<&T>`, a tuple of those, ...).
pub trait TQueryParam
{
    type QueryItem<'a>;

    /// Offsets resolved from one archetype's layout
    type ArchState: Copy;

    /// Pointers resolved from one chunk
    type Fetch: Copy;

    /// A `'static` stand-in for this query's exact shape: `&Hp` is `&'static Hp`, `&mut Hp` is
    /// `&'static mut Hp`, `Changed<&Hp>` is `Changed<&'static Hp>`, and a tuple nests its
    /// elements' shapes. Lifetimes are pinned to `'static` only so the type can reach
    /// `TypeId::of`, they carry no meaning here.
    type Shape: 'static;

    /// Unique per query shape, see [`TQueryParam::Shape`]
    const TYPE_ID: TypeId = TypeId::of::<Self::Shape>();

    /// `true` when the shape names no column at all, i.e. [`Without`](crate::query::filter::Without)
    /// on its own. There is nothing to walk then, so every iterator is empty from the start.
    const NAMES_NO_COLUMN: bool = false;

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>;

    /// Panics if the archetype lacks a column this shape needs. That cannot happen for an
    /// archetype the query matched, since matching already checked the same thing.
    #[track_caller]
    fn arch_state(layout: &ChunkLayout) -> Self::ArchState;

    /// # Safety
    /// `chunk_ptr` must be the base pointer of a chunk built from the layout `state` came from.
    unsafe fn fetch_init(state: Self::ArchState, chunk_ptr: *mut u8) -> Self::Fetch;

    /// Does `row` pass this shape's filters? Always `true` for a plain column.
    unsafe fn accepts(fetch: &Self::Fetch, row: usize, ticks: QueryTicks) -> bool;

    /// Same as [`Self::accepts`]. On top of that, nothing else may hold a `&mut` to this row
    /// for as long as the item lives.
    unsafe fn fetch<'a>(fetch: &Self::Fetch, row: usize, ticks: QueryTicks) -> Self::QueryItem<'a>;
}

/// Marker for read-only queries (`&T`, or tuples where all elements are `&T`).
/// `Query<T>` can only be `Clone` or `Copy` if `T` implements this trait. The reason is that each copy creates a new iterator starting from the beginning. If we allowed this for `&mut T`, two copies would produce two `&mut` references pointing to the same component. Multiple `&T` references at once are perfectly fine.
pub unsafe trait TReadOnlyQueryParam: TQueryParam {}

/// A plain column, `&T` or `&mut T`. The state filters (`Added<Q>`, `Changed<Q>`, ...) only
/// wrap one of these, so they know which component's state region to look at.
pub trait TQueryColumn: TQueryParam
{
    type Component: TComponent + 'static;
}
