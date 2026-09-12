use std::marker::PhantomData;

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryColumn, TQueryParam, TQueryParamFiltered, TQuerySrcAccess, TReadOnlyQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::{TChangeAble, TComponent, TEnableAble};
use crate::chunk::column::ColumnDescriptor;
use crate::chunk::{read_bit, read_changed_tick};
use crate::query::access_scope::AccessScope;
use crate::query::mut_ref::WriteStamp;
use crate::query::src_access_added::SrcAccessAdded;
use crate::query::src_access_changed::SrcAccessChanged;
use crate::query::src_access_enable::SrcAccessEnable;
use crate::utils::{component_id_for, is_newer_than};
use crate::world::query_spec::QuerySpecAccessor;

macro_rules! define_filter
{
    (
        $(#[$meta:meta])*
        $name:ident,
        component_bound: $component_bound:path,
        src_access: $src_access:ty,
        state_offset: $offset_field:ident,
        accepts: |$state_ptr:ident, $row:ident, $last_run_tick:ident, $this_run_tick:ident| $accepts:expr
    ) =>
    {
        $(#[$meta])*
        /// `Q` is unbounded on purpose: besides the real query param, this also gets
        /// instantiated with [`TQueryParam::Shape`] types, which are plain markers and implement
        /// nothing. The real bounds live on the impls below.
        pub struct $name<Q>(PhantomData<fn() -> Q>);

        impl<Q: TQueryColumn> TQueryParam for $name<Q>
        where
            Q::Component: $component_bound,
        {
            type QueryItem<'a> = Q::QueryItem<'a>;
            type SrcAccess<'a> = $src_access;
            // the wrapped `Q` keeps its own shape, so `Changed<&Hp>` and `Changed<&mut Hp>` stay
            // apart just like `&Hp` and `&mut Hp` do
            type Shape = $name<Q::Shape>;

            fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
            {
                Q::access_scope(component_specs)
            }

            #[track_caller]
            fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>
            {
                src_access.next::<Q>()
            }
        }

        #[doc = concat!("Note: ", stringify!($name), "<Q> is only considered read-only if Q itself is also read-only.")]
        unsafe impl<Q: TQueryColumn + TReadOnlyQueryParam> TReadOnlyQueryParam for $name<Q> where Q::Component: $component_bound {}

        impl<Q: TQueryColumn> TQueryParamFiltered for $name<Q>
        where
            Q::Component: $component_bound,
        {
            type Component = Q::Component;

            fn state_offset(col_des: &ColumnDescriptor) -> Option<usize>
            {
                col_des.state_offset.$offset_field
            }

            unsafe fn accepts(
                $state_ptr: *mut u8,
                $row: usize,
                $last_run_tick: ChangedTick,
                $this_run_tick: ChangedTick,
            ) -> bool
            {
                $accepts
            }

            unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize, stamp: WriteStamp) -> Self::QueryItem<'a>
            {
                unsafe { Q::read_from(col_ptr, row, stamp) }
            }
        }
    };
}

define_filter!(
    Added,
    component_bound: TChangeAble,
    src_access: SrcAccessAdded<'a>,
    state_offset: added_offset,
    accepts: |state_ptr, row, last_run_tick, this_run_tick|
        is_newer_than(unsafe { read_changed_tick(state_ptr, row) }, last_run_tick, this_run_tick)
);

define_filter!(
    Changed,
    component_bound: TChangeAble,
    src_access: SrcAccessChanged<'a>,
    state_offset: changed_offset,
    accepts: |state_ptr, row, last_run_tick, this_run_tick|
        is_newer_than(unsafe { read_changed_tick(state_ptr, row) }, last_run_tick, this_run_tick)
);

define_filter!(
    Enabled,
    component_bound: TEnableAble,
    src_access: SrcAccessEnable<'a, true>,
    state_offset: enable_offset,
    accepts: |state_ptr, row, _last_run_tick, _this_run_tick| unsafe { read_bit(state_ptr, row) }
);

define_filter!(
    Disabled,
    component_bound: TEnableAble,
    src_access: SrcAccessEnable<'a, false>,
    state_offset: enable_offset,
    accepts: |state_ptr, row, _last_run_tick, _this_run_tick| !unsafe { read_bit(state_ptr, row) }
);

/// Keeps only the rows whose archetype does *not* carry `T`.
///
/// Unlike the filters above it takes the component itself, not `&T` or `&mut T`. It never reads
/// anything: the component it names is exactly the one the matched archetypes are guaranteed to
/// lack, so there is nothing to hand out. Its whole job is one bit in
/// [`AccessScope::exclude`], and `belong_to` then drops those archetypes before iteration even
/// starts. That also means it costs nothing per row.
///
/// It only makes sense as one element of a tuple, next to the columns you do want:
///
/// ```
/// # use xynok_ecs::query::Query;
/// # use xynok_ecs::query::filter::Without;
/// # #[xynok_ecs::component]
/// # struct Hp(u32);
/// # #[xynok_ecs::component]
/// # struct Frozen(u32);
/// fn thaw(q: Query<(&Hp, Without<Frozen>)>)
/// {
///     for (hp, _) in q
///     {
///         let _ = hp;
///     }
/// }
/// ```
///
/// Used on its own, `Query<Without<Frozen>>` names no column to walk, so it yields nothing.
pub struct Without<T: TComponent + 'static>(PhantomData<fn() -> T>);

/// The source access of a query that selects rows but names no column to read, i.e.
/// [`Without`] standing alone. It has nothing to walk, so it is empty from the start.
pub struct SrcAccessEmpty;

impl<'a> TQuerySrcAccess<'a> for SrcAccessEmpty
{
    fn new(_accessor: &QuerySpecAccessor<'a>) -> Self
    {
        Self
    }
}

impl<T: TComponent + 'static> TQueryParam for Without<T>
{
    type QueryItem<'a> = ();
    type SrcAccess<'a> = SrcAccessEmpty;
    type Shape = Without<T::StorageType>;

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = AccessScope::default();
        scope.exclude.insert(component_id_for::<T>(component_specs));
        Ok(scope)
    }

    fn next<'a>(_src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>
    {
        None
    }
}

// SAFETY: `Without<T>` hands out `()`, there is nothing to write through
unsafe impl<T: TComponent + 'static> TReadOnlyQueryParam for Without<T> {}

impl<T: TComponent + 'static> TQueryParamFiltered for Without<T>
{
    // never looked up, `IS_IGNORE_FILTER` keeps the tuple iterator away from it
    type Component = T;

    const IS_IGNORE_FILTER: bool = true;

    fn state_offset(_col_des: &ColumnDescriptor) -> Option<usize>
    {
        None
    }

    unsafe fn accepts(_state_ptr: *mut u8, _row: usize, _last_run_tick: ChangedTick, _this_run_tick: ChangedTick) -> bool
    {
        // the archetype was already dropped by `exclude`, every row that gets here passes
        true
    }

    unsafe fn read_from<'a>(_col_ptr: *mut u8, _row: usize, _stamp: WriteStamp) -> Self::QueryItem<'a> {}
}
