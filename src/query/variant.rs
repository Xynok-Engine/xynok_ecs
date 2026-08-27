use std::any::TypeId;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryColumn, TQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponent;
use crate::chunk::Chunk;
use crate::query::access_scope::AccessScope;
use crate::query::src_access::{column_ptr, SrcAccess};
use crate::utils::component_id_for;
use crate::world::arch_spec::ArchetypeSpec;

impl<T: TComponent + 'static> TQueryParam for &T
{
    type QueryItem<'a> = &'a T;

    type SrcAccess<'a> = SrcAccess<'a>;

    type ChunkColumns<'a> = &'a [T];

    const TYPE_ID: TypeId = TypeId::of::<T::StorageType>();

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

    #[track_caller]
    unsafe fn chunk_columns<'a>(arch_spec: &ArchetypeSpec, chunk: &Chunk) -> &'a [T]
    {
        let base = column_ptr::<T>(arch_spec, chunk);
        unsafe { std::slice::from_raw_parts(base as *const T, chunk.len()) }
    }
}
impl<T: TComponent + 'static> TQueryParam for &mut T
{
    type QueryItem<'a> = &'a mut T;

    type SrcAccess<'a> = SrcAccess<'a>;

    type ChunkColumns<'a> = &'a mut [T];

    const TYPE_ID: TypeId = TypeId::of::<T::StorageType>();

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

    #[track_caller]
    unsafe fn chunk_columns<'a>(arch_spec: &ArchetypeSpec, chunk: &Chunk) -> &'a mut [T]
    {
        let base = column_ptr::<T>(arch_spec, chunk);
        unsafe { std::slice::from_raw_parts_mut(base as *mut T, chunk.len()) }
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
