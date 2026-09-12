use crate::apis::constants::ChangedTick;
use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{shape, TQueryColumn, TQueryParam, TQueryParamFiltered, TReadOnlyQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponent;
use crate::chunk::column::ColumnDescriptor;
use crate::query::access_scope::AccessScope;
use crate::query::mut_ref::{TMutPolicy, WriteStamp};
use crate::query::src_access::SrcAccess;
use crate::utils::component_id_for;

impl<T: TComponent + 'static> TQueryParam for &T
{
    type QueryItem<'a> = &'a T;

    type SrcAccess<'a> = SrcAccess<'a>;
    type Shape = (shape::Ref, T::StorageType);

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = AccessScope::default();
        scope.read.insert(component_id_for::<T>(component_specs));
        Ok(scope)
    }

    #[track_caller]
    fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>
    {
        src_access.next::<T>()
    }
}
// SAFETY: `&T` only hands out `&'a T`, nothing can be written through it
unsafe impl<T: TComponent + 'static> TReadOnlyQueryParam for &T {}

/// One row of a `&mut T` query: `Mut<'a, T>` when `T` is `ChangeAble`, a plain `&'a mut T`
/// otherwise. See [`TMutPolicy`].
pub type MutItem<'a, T> = <<T as TComponent>::MutPolicy as TMutPolicy>::Item<'a, T>;

impl<T: TComponent + 'static> TQueryParam for &mut T
{
    type QueryItem<'a> = MutItem<'a, T>;

    type SrcAccess<'a> = SrcAccess<'a>;
    type Shape = (shape::RefMut, T::StorageType);

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = AccessScope::default();
        scope.write.insert(component_id_for::<T>(component_specs));
        Ok(scope)
    }

    #[track_caller]
    fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>
    {
        src_access.next_mut::<T>()
    }
}

impl<T: TComponent + 'static> TQueryColumn for &T
{
    type Component = T;

    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize, _stamp: WriteStamp) -> &'a T
    {
        unsafe { &*(col_ptr as *const T).add(row) }
    }
}
impl<T: TComponent + 'static> TQueryColumn for &mut T
{
    type Component = T;

    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize, stamp: WriteStamp) -> MutItem<'a, T>
    {
        unsafe { T::MutPolicy::make((col_ptr as *mut T).add(row), stamp, row) }
    }
}

// A plain column never filters anything: `state_offset` always resolves to `None`, so `accepts`
// never dereferences its (always-null) `state_ptr`.
impl<T: TComponent + 'static> TQueryParamFiltered for &T
{
    type Component = T;

    fn state_offset(_col_des: &ColumnDescriptor) -> Option<usize>
    {
        None
    }
    unsafe fn accepts(_state_ptr: *mut u8, _row: usize, _last_run_tick: ChangedTick, _this_run_tick: ChangedTick) -> bool
    {
        true
    }
    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize, stamp: WriteStamp) -> Self::QueryItem<'a>
    {
        unsafe { <&T as TQueryColumn>::read_from(col_ptr, row, stamp) }
    }
}
impl<T: TComponent + 'static> TQueryParamFiltered for &mut T
{
    type Component = T;

    fn state_offset(_col_des: &ColumnDescriptor) -> Option<usize>
    {
        None
    }
    unsafe fn accepts(_state_ptr: *mut u8, _row: usize, _last_run_tick: ChangedTick, _this_run_tick: ChangedTick) -> bool
    {
        true
    }
    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize, stamp: WriteStamp) -> Self::QueryItem<'a>
    {
        unsafe { <&mut T as TQueryColumn>::read_from(col_ptr, row, stamp) }
    }
}
