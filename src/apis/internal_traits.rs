use std::any::TypeId;

use crate::apis::identifies::XynokEcsError;
use crate::query::access_scope::AccessScope;
use crate::world::query_spec::QuerySpecAccessor;

pub trait TQueryParam
{
    type QueryItem<'a>;
    type SrcAccess;
    const TYPE_ID: TypeId;
    fn access_scope() -> Result<AccessScope, XynokEcsError>;
    fn build_src_access(src_access: &QuerySpecAccessor) -> Self::SrcAccess;
    fn next<'a>(src_access: &mut Self::SrcAccess) -> Option<Self::QueryItem<'a>>;
}
pub trait TSystemParam {}
