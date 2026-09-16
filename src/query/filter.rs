use std::marker::PhantomData;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{QueryTicks, TQueryColumn, TQueryParam, TReadOnlyQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::{TChangeAble, TComponent, TEnableAble};
use crate::chunk::layout::ChunkLayout;
use crate::chunk::{read_bit, read_changed_tick};
use crate::query::access_scope::AccessScope;
use crate::query::variant::column_descriptor;
use crate::utils::{component_id_for, is_newer_than};

macro_rules! define_filter
{
    (
        $(#[$meta:meta])*
        $name:ident,
        component_bound: $component_bound:path,
        state_offset: $offset_field:ident,
        accepts: |$state_ptr:ident, $row:ident, $ticks:ident| $accepts:expr
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
            // the wrapped column's own state, plus the state region this filter reads
            type ArchState = (Q::ArchState, usize);
            type Fetch = (Q::Fetch, *mut u8);
            // the wrapped `Q` keeps its own shape, so `Changed<&Hp>` and `Changed<&mut Hp>` stay
            // apart just like `&Hp` and `&mut Hp` do
            type Shape = $name<Q::Shape>;

            fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
            {
                Q::access_scope(component_specs)
            }

            #[inline]
            #[track_caller]
            fn arch_state(layout: &ChunkLayout) -> Self::ArchState
            {
                let state_offset = column_descriptor::<Q::Component>(layout).state_offset.$offset_field.unwrap_or_else(|| {
                    panic!(
                        concat!("component `{}` has no state region for `", stringify!($name), "`"),
                        std::any::type_name::<<Q::Component as TComponent>::StorageType>()
                    )
                });
                (Q::arch_state(layout), state_offset)
            }

            #[inline]
            unsafe fn fetch_init(state: Self::ArchState, chunk_ptr: *mut u8) -> Self::Fetch
            {
                unsafe { (Q::fetch_init(state.0, chunk_ptr), chunk_ptr.add(state.1)) }
            }

            #[inline]
            unsafe fn accepts(fetch: &Self::Fetch, $row: usize, $ticks: QueryTicks) -> bool
            {
                let $state_ptr = fetch.1;
                $accepts
            }

            #[inline]
            unsafe fn fetch<'a>(fetch: &Self::Fetch, row: usize, ticks: QueryTicks) -> Self::QueryItem<'a>
            {
                unsafe { Q::fetch(&fetch.0, row, ticks) }
            }
        }

        #[doc = concat!("Note: ", stringify!($name), "<Q> is only considered read-only if Q itself is also read-only.")]
        unsafe impl<Q: TQueryColumn + TReadOnlyQueryParam> TReadOnlyQueryParam for $name<Q> where Q::Component: $component_bound {}
    };
}

define_filter!(
    Added,
    component_bound: TChangeAble,
    state_offset: added_offset,
    accepts: |state_ptr, row, ticks|
        is_newer_than(unsafe { read_changed_tick(state_ptr, row) }, ticks.last_run, ticks.this_run)
);

define_filter!(
    Changed,
    component_bound: TChangeAble,
    state_offset: changed_offset,
    accepts: |state_ptr, row, ticks|
        is_newer_than(unsafe { read_changed_tick(state_ptr, row) }, ticks.last_run, ticks.this_run)
);

define_filter!(
    Enabled,
    component_bound: TEnableAble,
    state_offset: enable_offset,
    accepts: |state_ptr, row, _ticks| unsafe { read_bit(state_ptr, row) }
);

define_filter!(
    Disabled,
    component_bound: TEnableAble,
    state_offset: enable_offset,
    accepts: |state_ptr, row, _ticks| !unsafe { read_bit(state_ptr, row) }
);

/// Keeps only the rows whose archetype does *not* carry `T`.
///
/// Unlike the filters above it takes the component itself, not `&T` or `&mut T`. It never reads
/// anything: the component it names is exactly the one the matched archetypes are guaranteed to
/// lack, so there is nothing to hand out. Its whole job is one bit in
/// [`AccessScope::exclude`], and `belong_to` then drops those archetypes before iteration even
/// starts. That also means it costs nothing per row.
///
/// It only makes sense as one element of a tuple, next to the columns you do want, e.g.
/// `Query<(&Hp, Without<Frozen>)>`. Used on its own, `Query<Without<Frozen>>` names no column to walk, so it yields nothing.
pub struct Without<T: TComponent + 'static>(PhantomData<fn() -> T>);

impl<T: TComponent + 'static> TQueryParam for Without<T>
{
    type QueryItem<'a> = ();
    // the component is the one the archetype is guaranteed *not* to carry, so there is no
    // descriptor to look up and nothing to point at
    type ArchState = ();
    type Fetch = ();
    type Shape = Without<T::StorageType>;

    const NAMES_NO_COLUMN: bool = true;

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = AccessScope::default();
        scope.exclude.insert(component_id_for::<T>(component_specs));
        Ok(scope)
    }

    #[inline]
    fn arch_state(_layout: &ChunkLayout) {}

    #[inline]
    unsafe fn fetch_init(_state: (), _chunk_ptr: *mut u8) {}

    #[inline]
    unsafe fn accepts(_fetch: &(), _row: usize, _ticks: QueryTicks) -> bool
    {
        // the archetype was already dropped by `exclude`, every row that gets here passes
        true
    }

    #[inline]
    unsafe fn fetch<'a>(_fetch: &(), _row: usize, _ticks: QueryTicks) -> Self::QueryItem<'a> {}
}

// SAFETY: `Without<T>` hands out `()`, there is nothing to write through
unsafe impl<T: TComponent + 'static> TReadOnlyQueryParam for Without<T> {}
