use std::any::TypeId;

use crate::apis::constants::ChangedTick;
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
    /// The tick as of this *system's* previous run (not any other system's), copied from
    /// `World::current_system_last_run` when this accessor was built. `Changed`/`Added` query
    /// filters compare a row's tick against this.
    pub last_run_tick:   ChangedTick,
}

impl<'a> QuerySpecAccessor<'a>
{
    /// Indices of the archetypes this query currently matches.
    #[inline]
    #[track_caller]
    pub fn arch_indices(&self) -> &'a [usize]
    {
        // copy the `&'a` out of `&self` first, so the slice keeps the accessor's lifetime
        // instead of being reborrowed for the shorter life of this `&self`
        let queries: &'a QuerySpecs = self.queries;
        match queries.value_at(self.query_idx)
        {
            Some(spec) => spec.archetypes.as_slice(),
            None => panic!("query index {} is not in the world's query registry", self.query_idx),
        }
    }
}
