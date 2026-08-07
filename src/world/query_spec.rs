use std::any::TypeId;

use crate::apis::params::ComponentSpecs;
use crate::collection::sequence_value_hash_map::SequenceValueHashMap;
use crate::query::access_scope::AccessScope;
use crate::world::arch_spec::ArchetypeSpecs;

/// The world's query registry, keyed by the query type
pub type QuerySpecs = SequenceValueHashMap<TypeId, QuerySpec>;

pub struct QuerySpec
{
    /// Indices into the world's [`ArchetypeSpecs`], not pointers into it. The registry's
    /// dense storage relocates its values whenever it grows; the indices do not change.
    pub archetypes:   Vec<usize>,
    pub access_scope: AccessScope,
    pub version:      usize,
}

/// A query's handle on the world's registries.
///
/// It stores *where to look* rather than the addresses it found, which is what lets a system
/// take several `Query` parameters at once: each parameter is initialised in turn, and
/// registering the second query can relocate the first one's `QuerySpec`. An accessor holding
/// a pointer into that spec would be left dangling before the system body even runs.
///
/// The registry pointers survive a move of the `World` itself because each registry is boxed -
/// moving the world moves the `Box`, not the storage behind it.
#[derive(Clone, Copy)]
pub struct QuerySpecAccessor
{
    pub queries:         *const QuerySpecs,
    pub query_idx:       usize,
    pub archetypes:      *const ArchetypeSpecs,
    pub component_specs: *const ComponentSpecs,
}

impl QuerySpecAccessor
{
    /// Indices of the archetypes this query currently matches.
    ///
    /// # Safety
    /// The registries this accessor points at must still be alive.
    #[inline]
    #[track_caller]
    pub unsafe fn arch_indices<'a>(&self) -> &'a [usize]
    {
        match unsafe { (*self.queries).value_at(self.query_idx) }
        {
            Some(spec) => spec.archetypes.as_slice(),
            None => panic!("query index {} is not in the world's query registry", self.query_idx),
        }
    }
}
