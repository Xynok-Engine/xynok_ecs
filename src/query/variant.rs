use std::any::TypeId;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryColumn, TQueryParam, TQuerySrcAccess};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponent;
use crate::query::access_scope::AccessScope;
use crate::query::src_access::SrcAccess;
use crate::utils::component_id_for;
use crate::world::query_spec::QuerySelection;

impl<T: TComponent + 'static> TQueryParam for &T
{
    type QueryItem<'a> = &'a T;

    type SrcAccess<'a> = SrcAccess<'a>;

    const TYPE_ID: TypeId = TypeId::of::<T::StorageType>();

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope: AccessScope = AccessScope::default();
        scope.read.insert(component_id_for::<T>(component_specs));
        Ok(scope)
    }

    #[track_caller]
    fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>
    {
        src_access.next::<T>()
    }
}
impl<T: TComponent + 'static> TQueryParam for &mut T
{
    type QueryItem<'a> = &'a mut T;

    type SrcAccess<'a> = SrcAccess<'a>;

    const TYPE_ID: TypeId = TypeId::of::<T::StorageType>();

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope: AccessScope = AccessScope::default();
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

    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize) -> &'a T
    {
        unsafe { &*(col_ptr as *const T).add(row) }
    }
}
impl<T: TComponent + 'static> TQueryColumn for &mut T
{
    type Component = T;

    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize) -> &'a mut T
    {
        unsafe { &mut *(col_ptr as *mut T).add(row) }
    }
}

/// Lets a query name no normal component at all, e.g. `Query<(), &Mesh>`. There is no column
/// to read, so it only counts the rows the selection covers.
pub struct UnitSrcAccess
{
    remaining: usize,
}
impl<'a> TQuerySrcAccess<'a> for UnitSrcAccess
{
    fn new(selection: QuerySelection<'a>) -> Self
    {
        let mut remaining = 0;
        for (i, arch_idx) in selection.arch_indices.iter().enumerate()
        {
            let arch = &selection.archetypes.value_at(*arch_idx).unwrap().arch;
            let first_chunk = if i == 0 { selection.first_chunk } else { 0 };
            for chunk_idx in first_chunk..arch.chunk_count().min(selection.chunk_end)
            {
                remaining += arch.chunk_at(chunk_idx).len();
            }
        }
        Self { remaining }
    }
}
impl TQueryParam for ()
{
    type QueryItem<'a> = ();
    type SrcAccess<'a> = UnitSrcAccess;
    const TYPE_ID: TypeId = TypeId::of::<()>();
    fn access_scope(_: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope::default())
    }
    fn next<'a>(src: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>
    {
        if src.remaining == 0
        {
            None
        }
        else
        {
            src.remaining -= 1;
            Some(())
        }
    }
}
