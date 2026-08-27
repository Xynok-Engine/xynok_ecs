use std::any::TypeId;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponent;
use crate::chunk::Chunk;
use crate::query::access_scope::AccessScope;
use crate::world::arch_spec::ArchetypeSpec;
use crate::world::query_spec::QuerySpecAccessor;
pub trait TQuerySrcAccess
{
    fn new(accessor: &QuerySpecAccessor) -> Self;
}
pub trait TQueryParam
{
    type QueryItem<'a>;
    type SrcAccess<'a>: TQuerySrcAccess;
    /// A chunk's whole columns rather than one row at a time: `&[C]` for `&C`, `&mut [C]` for
    /// `&mut C`, and a tuple of slices for a tuple query.
    ///
    /// This is what [`crate::query::ChunkView`] carries, and also why a chunk is the ECS's natural
    /// unit of work: a chunk is already contiguous in memory.
    type ChunkColumns<'a>;
    const TYPE_ID: TypeId;
    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>;
    #[track_caller]
    fn next<'a>(src_access: &mut Self::SrcAccess<'a>) -> Option<Self::QueryItem<'a>>;
    fn build_src_access<'a>(src_access: &QuerySpecAccessor) -> Self::SrcAccess<'a>
    {
        Self::SrcAccess::new(src_access)
    }

    /// Builds the column slices for one chunk.
    ///
    /// # Safety
    ///
    /// `arch_spec` must carry every column this query names, and `chunk` must be one of its own
    /// chunks. A caller walking the pre-filtered archetype list in [`QuerySpecAccessor`] satisfies
    /// that for free.
    ///
    /// The caller must also guarantee that no two `&mut` slices point into the same chunk, which
    /// means no two jobs may be handed the same chunk.
    #[track_caller]
    unsafe fn chunk_columns<'a>(arch_spec: &ArchetypeSpec, chunk: &Chunk) -> Self::ChunkColumns<'a>;
}

pub trait TQueryColumn: TQueryParam
{
    type Component: TComponent + 'static;
    unsafe fn read_from<'a>(col_ptr: *mut u8, row: usize) -> Self::QueryItem<'a>;
}
