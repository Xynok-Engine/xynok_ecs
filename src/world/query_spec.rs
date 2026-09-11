use std::any::TypeId;

use crate::apis::params::ComponentSpecs;
use crate::archetype_chunk::SharedValue;
use crate::collection::sequence_value_hash_map::SequenceValueHashMap;
use crate::query::access_scope::AccessScope;
use crate::world::arch_spec::ArchetypeSpecs;

/// The world's query registry, keyed by the normal, shared and filter types of the query
pub type QuerySpecs = SequenceValueHashMap<(TypeId, TypeId, TypeId), QuerySpec>;

pub struct QuerySpec
{
    /// Indices into the world's [`ArchetypeSpecs`], not pointers into it. The registry's
    /// dense storage relocates its values whenever it grows; the indices do not change.
    pub archetypes:    Vec<usize>,
    pub access_scope:  AccessScope,
    /// The shared value a static filter asks for, resolved once when the spec is created, so
    /// `TSharedFilter::filter_key` is not called again on every frame.
    pub shared_filter: Option<SharedValue>,
    pub version:       usize,
}

/// A query's handle on the world's registries.
///
/// It stores *where to look* rather than the specific addresses it found. This design allows a system
/// to accept several `Query` parameters at once: each parameter is initialized in turn, and
/// registering the second query can relocate the first one's `QuerySpec`. An accessor holding
/// a pointer into that spec would be left dangling before the system body even runs.
///
/// The borrows are plain shared references: the `Query` that owns this accessor already carries
/// a lifetime, so the registries can be reached with a normal `&'a` instead of a raw pointer that
/// every reader has to dereference inside `unsafe`. Writing through a `&mut T` query still goes
/// through the chunk's raw pointer, so a shared borrow of the registries is all we need here.
#[derive(Clone, Copy)]
pub struct QuerySpecAccessor<'a>
{
    pub query_idx:       usize,
    pub queries:         &'a QuerySpecs,
    pub archetypes:      &'a ArchetypeSpecs,
    pub component_specs: &'a ComponentSpecs,
}

impl<'a> QuerySpecAccessor<'a>
{
    #[inline]
    #[track_caller]
    pub fn spec(&self) -> &'a QuerySpec
    {
        // copy the `&'a` out of `&self` first, so the result keeps the accessor's lifetime
        // instead of being reborrowed for the shorter life of this `&self`
        let queries: &'a QuerySpecs = self.queries;
        match queries.value_at(self.query_idx)
        {
            Some(spec) => spec,
            None => panic!("query index {} is not in the world's query registry", self.query_idx),
        }
    }

    /// Indices of the archetypes this query currently matches.
    #[inline]
    #[track_caller]
    pub fn arch_indices(&self) -> &'a [usize]
    {
        self.spec().archetypes.as_slice()
    }

    #[inline]
    #[track_caller]
    pub fn selection(&self) -> QuerySelection<'a>
    {
        QuerySelection::archetypes(self.archetypes, self.arch_indices())
    }
}

/// The rows one traversal walks: a list of archetypes, and optionally a single chunk of them.
///
/// A whole query walks every chunk of every archetype. `ChunkView` walks one chunk of one
/// archetype, which is why the range is only a start and an end instead of a filter checked
/// on every row.
#[derive(Clone, Copy)]
pub struct QuerySelection<'a>
{
    pub archetypes:   &'a ArchetypeSpecs,
    pub arch_indices: &'a [usize],
    /// Chunk the first archetype starts at. Later archetypes always start at 0.
    pub first_chunk:  usize,
    /// No archetype is read past this chunk index.
    pub chunk_end:    usize,
}

impl<'a> QuerySelection<'a>
{
    #[inline]
    pub fn archetypes(archetypes: &'a ArchetypeSpecs, arch_indices: &'a [usize]) -> Self
    {
        Self {
            archetypes:   archetypes,
            arch_indices: arch_indices,
            first_chunk:  0,
            chunk_end:    usize::MAX,
        }
    }

    #[inline]
    pub fn chunk(archetypes: &'a ArchetypeSpecs, arch_idx: &'a usize, chunk_idx: usize) -> Self
    {
        Self {
            archetypes:   archetypes,
            arch_indices: std::slice::from_ref(arch_idx),
            first_chunk:  chunk_idx,
            chunk_end:    chunk_idx + 1,
        }
    }
}
