use std::any::TypeId;

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponent;
use crate::chunk::column::ColumnDescriptor;
use crate::query::access_scope::AccessScope;
use crate::query::mut_ref::WriteStamp;
use crate::world::query_spec::QuerySpecAccessor;
pub trait TQuerySrcAccess<'a>
{
    fn new(accessor: &QuerySpecAccessor<'a>) -> Self;
}
pub trait TQueryParam
{
    type QueryItem<'a>;
    type SrcAccess<'a>: TQuerySrcAccess<'a>;

    /// A `'static` stand-in for this query's exact shape: `&Hp` is `&'static Hp`, `&mut Hp` is
    /// `&'static mut Hp`, `Changed<&Hp>` is `Changed<&'static Hp>`, and a tuple nests its
    /// elements' shapes. Lifetimes are pinned to `'static` only so the type can reach
    /// `TypeId::of`, they carry no meaning here.
    ///
    /// The world keys its query registry by `TYPE_ID`, and a `QuerySpec` carries the access
    /// scope the scheduler reads to decide what may run in parallel. Naming a shape only by the
    /// components it touches would put `&Hp` and `&mut Hp` in the same slot, so whichever
    /// registered first would hand its scope to the other: a `&mut` query inheriting a read-only
    /// scope gets scheduled alongside readers, which is `&mut` aliasing from safe code.
    type Shape: 'static;

    /// Unique per query shape, see [`TQueryParam::Shape`]
    const TYPE_ID: TypeId = TypeId::of::<Self::Shape>();

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>;
    #[track_caller]
    fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>;
    fn build_src_access<'a>(accessor: &QuerySpecAccessor<'a>) -> Self::SrcAccess<'a>
    {
        Self::SrcAccess::new(accessor)
    }
}
/// Marker for read-only queries (`&T`, or tuples where all elements are `&T`).
///
/// `Query<T>` can only be `Clone` or `Copy` if `T` implements this trait. The reason is that each copy creates a new iterator starting from the beginning. If we allowed this for `&mut T`, two copies would produce two `&mut` references pointing to the same component. Multiple `&T` references at once are perfectly fine.
///
/// # Safety
/// Only implement this for parameters where `QueryItem` does not allow writing to the component. Implementing this for a parameter containing `&mut` would lead to `&mut` aliasing within safe code.
pub unsafe trait TReadOnlyQueryParam: TQueryParam {}

pub trait TQueryColumn: TQueryParam
{
    type Component: TComponent + 'static;
    /// Hands out one row. `stamp` carries where this column records writes and the tick to
    /// record them with; a read-only column ignores it.
    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize, stamp: WriteStamp) -> Self::QueryItem<'a>;
}

/// Something a tuple query (`Query<(A, B, ...)>`) can hold one slot of: either a plain column
/// (`&T`/`&mut T`, which never filters anything) or a state filter (`Added<Q>`/`Changed<Q>`/
/// `Enabled<Q>`/`Disabled<Q>`, which additionally needs a per-chunk pointer into some state
/// region to decide whether a row passes).
///
/// A tuple shares one row cursor across every element, so filtering can't live on each element's
/// own iterator (that's what the standalone `SrcAccessAdded`/`SrcAccessChanged`/`SrcAccessEnable`
/// do for `Query<Added<Q>>` etc. used alone) - the tuple instead asks every element `accepts`
/// for the *same* row before reading any of them, skipping the row only if all agree.
pub trait TQueryParamFiltered: TQueryParam
{
    type Component: TComponent + 'static;

    /// Whether this element only selects archetypes and reads no column at all.
    ///
    /// `true` only for markers like [`Without`](crate::query::filter::Without), which do all of
    /// their work in [`TQueryParam::access_scope`] and never touch a row. The tuple iterator
    /// resolves one column pointer per element, and for those markers the column is guaranteed
    /// *not* to be there, so it has to skip them instead of looking the descriptor up. Being a
    /// const, the skipped branch disappears at compile time.
    const IS_IGNORE_FILTER: bool = false;

    /// The offset (within a chunk) of the state region this element needs to check `accepts`,
    /// resolved from the archetype's column descriptor for `Component`. `None` for a plain
    /// column: it has no state to check, so `accepts` never dereferences its `state_ptr`.
    fn state_offset(col_des: &ColumnDescriptor) -> Option<usize>;

    /// Does the row at `state_ptr` (the chunk pointer already advanced by `state_offset`, or
    /// null if `state_offset` returned `None`) pass this element's filter? Always `true` for a
    /// plain column.
    unsafe fn accepts(state_ptr: *mut u8, row: usize, last_run_tick: ChangedTick, this_run_tick: ChangedTick) -> bool;

    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize, stamp: WriteStamp) -> Self::QueryItem<'a>;
}
