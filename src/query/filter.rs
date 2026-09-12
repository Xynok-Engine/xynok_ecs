use std::marker::PhantomData;

use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{shape, TQueryColumn, TQueryParam, TQueryParamFiltered, TReadOnlyQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::{TChangeAble, TEnableAble};
use crate::chunk::column::ColumnDescriptor;
use crate::chunk::{read_bit, read_changed_tick};
use crate::query::access_scope::AccessScope;
use crate::query::mut_ref::WriteStamp;
use crate::query::src_access_added::SrcAccessAdded;
use crate::query::src_access_changed::SrcAccessChanged;
use crate::query::src_access_enable::SrcAccessEnable;
use crate::utils::is_newer_than;

macro_rules! define_filter
{
    (
        $(#[$meta:meta])*
        $name:ident,
        shape: $shape:ty,
        component_bound: $component_bound:path,
        src_access: $src_access:ty,
        state_offset: $offset_field:ident,
        accepts: |$state_ptr:ident, $row:ident, $last_run_tick:ident, $this_run_tick:ident| $accepts:expr
    ) =>
    {
        $(#[$meta])*
        pub struct $name<Q: TQueryColumn>(PhantomData<Q>)
        where
            Q::Component: $component_bound;

        impl<Q: TQueryColumn> TQueryParam for $name<Q>
        where
            Q::Component: $component_bound,
        {
            type QueryItem<'a> = Q::QueryItem<'a>;
            type SrcAccess<'a> = $src_access;
            // the wrapped `Q` keeps its own shape, so `Changed<&Hp>` and `Changed<&mut Hp>` stay
            // apart just like `&Hp` and `&mut Hp` do
            type Shape = ($shape, Q::Shape);

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
    shape: shape::Added,
    component_bound: TChangeAble,
    src_access: SrcAccessAdded<'a>,
    state_offset: added_offset,
    accepts: |state_ptr, row, last_run_tick, this_run_tick|
        is_newer_than(unsafe { read_changed_tick(state_ptr, row) }, last_run_tick, this_run_tick)
);

define_filter!(
    Changed,
    shape: shape::Changed,
    component_bound: TChangeAble,
    src_access: SrcAccessChanged<'a>,
    state_offset: changed_offset,
    accepts: |state_ptr, row, last_run_tick, this_run_tick|
        is_newer_than(unsafe { read_changed_tick(state_ptr, row) }, last_run_tick, this_run_tick)
);

define_filter!(
    Enabled,
    shape: shape::Enabled,
    component_bound: TEnableAble,
    src_access: SrcAccessEnable<'a, true>,
    state_offset: enable_offset,
    accepts: |state_ptr, row, _last_run_tick, _this_run_tick| unsafe { read_bit(state_ptr, row) }
);

define_filter!(
    Disabled,
    shape: shape::Disabled,
    component_bound: TEnableAble,
    src_access: SrcAccessEnable<'a, false>,
    state_offset: enable_offset,
    accepts: |state_ptr, row, _last_run_tick, _this_run_tick| !unsafe { read_bit(state_ptr, row) }
);
