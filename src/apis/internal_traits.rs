use std::any::TypeId;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponent;
use crate::query::access_scope::AccessScope;
use crate::world::query_spec::QuerySpecAccessor;
pub trait TQuerySrcAccess<'a>
{
    fn new(accessor: &QuerySpecAccessor<'a>) -> Self;
}
pub trait TQueryParam
{
    type QueryItem<'a>;
    type SrcAccess<'a>: TQuerySrcAccess<'a>;
    const TYPE_ID: TypeId;
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
    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize) -> Self::QueryItem<'a>;
}
