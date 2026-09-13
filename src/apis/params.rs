use std::any::TypeId;
use std::marker::PhantomData;

use crate::apis::constants::ChangedTick;
use crate::apis::traits::TArchetype;
use crate::apis::ComponentDescriptor;
use crate::archetype::Archetype;
use crate::chunk::layout::ChunkLayout;
use crate::chunk::migration::MigrationPlan;
use crate::chunk::Chunk;
use crate::collection::sequence_value_hash_map::SequenceValueHashMap;
use crate::entity::Entity;

/// The world's component registry. A component's id is its index in here, so
/// `index_of(type_id)` is what turns a `TypeId` into a `ComponentBitSet` bit.
pub type ComponentSpecs = SequenceValueHashMap<TypeId, ComponentSpec>;

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct SwappedRow
{
    pub e:    Entity,
    pub from: usize,
    pub to:   usize,
}
pub struct ComponentSpec
{
    pub descriptor: ComponentDescriptor,
}
#[derive(Debug, Clone, Copy)]
pub struct EntityInChunkIndices
{
    pub chunk_idx:    usize,
    pub idx_in_chunk: usize,
}
pub struct EntityIndices
{
    pub chunk_idx:    usize,
    pub idx_in_chunk: usize,
}

pub struct ArchetypeTakeAndWriteComponentParams<'a, T: TArchetype + 'static>
{
    pub src_e:      EntityIndices,
    pub src_arch:   &'a mut Archetype,
    pub src_layout: &'a ChunkLayout,
    pub dst_layout: &'a ChunkLayout,
    /// What to do with each column on the way over, taken from `World`'s edge cache.
    pub migration:  &'a MigrationPlan,
    pub write_val:  T,
    pub tick:       ChangedTick,
}
pub struct ArchetypeTakeAndRemoveComponentParams<'a, T: TArchetype + 'static>
{
    pub src_e:      EntityIndices,
    pub src_arch:   &'a mut Archetype,
    pub src_layout: &'a ChunkLayout,
    pub dst_layout: &'a ChunkLayout,
    /// What to do with each column on the way over, taken from `World`'s edge cache.
    pub migration:  &'a MigrationPlan,
    pub phantom:    PhantomData<T>,
}
pub struct ResultTakeAndWrite
{
    pub new_indices_took: EntityInChunkIndices,
    pub swapped_e:        Option<SwappedRow>,
}
pub struct ResultTakeAndRemove<T: TArchetype + 'static>
{
    pub new_indices_took: EntityInChunkIndices,
    pub swapped_e:        Option<SwappedRow>,
    pub val:              T,
}
pub struct ChunkTakeComponentParams<'a>
{
    pub from:       usize,
    pub to:         usize,
    pub src_chunk:  &'a mut Chunk,
    pub src_layout: &'a ChunkLayout,
    pub dst_layout: &'a ChunkLayout,
    /// What to do with every column of `src_layout`, computed once for this pair of archetypes.
    /// Components the caller is about to overwrite (`merge_component`'s `T`) are already marked
    /// as dropped in place inside the plan, so nothing here has to scan a list of type ids.
    pub migration:  &'a MigrationPlan,
}
