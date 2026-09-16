use std::any::TypeId;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{QueryTicks, TQueryColumn, TQueryParam, TReadOnlyQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponent;
use crate::chunk::column::ColumnDescriptor;
use crate::chunk::layout::ChunkLayout;
use crate::query::access_scope::AccessScope;
use crate::query::mut_ref::{TMutPolicy, WriteStamp};
use crate::utils::component_id_for;

/// Looks up `T`'s column in an archetype the query already matched
#[inline]
#[track_caller]
pub(crate) fn column_descriptor<T: TComponent + 'static>(layout: &ChunkLayout) -> &ColumnDescriptor
{
    match layout.component_col_descriptors.get(&TypeId::of::<T::StorageType>())
    {
        Some(col_des) => col_des,
        None => panic!(
            "archetype does not carry a column for component `{}` even though it was pre-filtered to contain it",
            std::any::type_name::<T::StorageType>()
        ),
    }
}

#[inline]
fn offset_ptr(chunk_ptr: *mut u8, offset: Option<usize>) -> *mut u8
{
    match offset
    {
        Some(offset) => unsafe { chunk_ptr.add(offset) },
        None => std::ptr::null_mut(),
    }
}

impl<T: TComponent + 'static> TQueryParam for &T
{
    type QueryItem<'a> = &'a T;
    // column offset, then column pointer
    type ArchState = usize;
    type Fetch = *mut u8;
    type Shape = &'static T::StorageType;

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = AccessScope::default();
        scope.read.insert(component_id_for::<T>(component_specs));
        Ok(scope)
    }

    #[inline]
    #[track_caller]
    fn arch_state(layout: &ChunkLayout) -> usize
    {
        column_descriptor::<T>(layout).offset
    }

    #[inline]
    unsafe fn fetch_init(state: usize, chunk_ptr: *mut u8) -> *mut u8
    {
        unsafe { chunk_ptr.add(state) }
    }

    #[inline]
    unsafe fn accepts(_fetch: &*mut u8, _row: usize, _ticks: QueryTicks) -> bool
    {
        true
    }

    #[inline]
    unsafe fn fetch<'a>(fetch: &*mut u8, row: usize, _ticks: QueryTicks) -> &'a T
    {
        unsafe { &*(*fetch as *const T).add(row) }
    }
}
// SAFETY: `&T` only hands out `&'a T`, nothing can be written through it
unsafe impl<T: TComponent + 'static> TReadOnlyQueryParam for &T {}

/// One row of a `&mut T` query: `Mut<'a, T>` when `T` is `ChangeAble`, a plain `&'a mut T`
/// otherwise. See [`TMutPolicy`].
pub type MutItem<'a, T> = <<T as TComponent>::MutPolicy as TMutPolicy>::Item<'a, T>;

/// Where a `&mut T` column records its writes. The changed pointer stays null while the
/// component tracks no changes, and `NoTracking` never looks at it.
#[derive(Clone, Copy)]
pub struct MutFetch
{
    col_ptr:     *mut u8,
    changed_ptr: *mut u8,
}

impl<T: TComponent + 'static> TQueryParam for &mut T
{
    type QueryItem<'a> = MutItem<'a, T>;
    // column offset and changed region offset
    type ArchState = (usize, Option<usize>);
    type Fetch = MutFetch;
    type Shape = &'static mut T::StorageType;

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = AccessScope::default();
        scope.write.insert(component_id_for::<T>(component_specs));
        Ok(scope)
    }

    #[inline]
    #[track_caller]
    fn arch_state(layout: &ChunkLayout) -> (usize, Option<usize>)
    {
        let col_des = column_descriptor::<T>(layout);
        (col_des.offset, col_des.state_offset.changed_offset)
    }

    #[inline]
    unsafe fn fetch_init(state: (usize, Option<usize>), chunk_ptr: *mut u8) -> MutFetch
    {
        MutFetch {
            col_ptr:     unsafe { chunk_ptr.add(state.0) },
            changed_ptr: offset_ptr(chunk_ptr, state.1),
        }
    }

    #[inline]
    unsafe fn accepts(_fetch: &MutFetch, _row: usize, _ticks: QueryTicks) -> bool
    {
        true
    }

    #[inline]
    unsafe fn fetch<'a>(fetch: &MutFetch, row: usize, ticks: QueryTicks) -> MutItem<'a, T>
    {
        let stamp = WriteStamp {
            changed_ptr: fetch.changed_ptr,
            tick:        ticks.this_run,
        };
        unsafe { T::MutPolicy::make((fetch.col_ptr as *mut T).add(row), stamp, row) }
    }
}

impl<T: TComponent + 'static> TQueryColumn for &T
{
    type Component = T;
}
impl<T: TComponent + 'static> TQueryColumn for &mut T
{
    type Component = T;
}
