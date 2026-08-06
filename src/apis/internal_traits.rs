use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpec;
use crate::apis::traits::TComponent;
use crate::query::access_scope::AccessScope;
use crate::world::arch_spec::ArchetypeSpec;
use crate::world::query_spec::QuerySpecAccessor;
pub trait TQuerySrcAccess
{
    fn new(arch: *mut Vec<*mut ArchetypeSpec>, specs: *const HashMap<TypeId, ComponentSpec>) -> Self;
}
pub trait TQueryParam
{
    type QueryItem<'a>;
    type SrcAccess<'a>: TQuerySrcAccess;
    const TYPE_ID: TypeId;
    fn access_scope() -> Result<AccessScope, XynokEcsError>;
    #[track_caller]
    fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>;
    fn build_src_access<'a>(src_access: &QuerySpecAccessor) -> Self::SrcAccess<'a>
    {
        Self::SrcAccess::new(src_access.archetypes, src_access.component_specs)
    }
}

pub trait TQueryColumn: TQueryParam
{
    type Component: TComponent + 'static;
    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize) -> Self::QueryItem<'a>;
}
