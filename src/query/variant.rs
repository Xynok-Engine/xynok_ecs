use std::any::TypeId;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{TQueryColumn, TQueryParam};
use crate::apis::traits::TComponent;
use crate::query::access_scope::AccessScope;
use crate::query::src_access::SrcAccess;

impl<T: TComponent + 'static> TQueryParam for &T
{
    type QueryItem<'a> = &'a T;

    type SrcAccess<'a> = SrcAccess<'a>;

    const TYPE_ID: TypeId = TypeId::of::<T::StorageType>();

    fn access_scope() -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope {
            read:    vec![TypeId::of::<T::StorageType>()],
            write:   vec![],
            exclude: vec![],
        })
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

    fn access_scope() -> Result<AccessScope, XynokEcsError>
    {
        Ok(AccessScope {
            read:    vec![],
            write:   vec![TypeId::of::<T::StorageType>()],
            exclude: vec![],
        })
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
