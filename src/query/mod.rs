use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;

use crate::apis::identifies::{StorageLocation, XynokEcsError};
use crate::apis::internal_traits::TQueryParam;
use crate::apis::params::ComponentSpecs;
use crate::query::access_scope::AccessScope;
use crate::query::query_iter::QueryIter;
use crate::shared::{NoSharedComponent, NoSharedFilter, TSharedComponent, TSharedComponentQueryParam, TSharedFilterParam};
use crate::world::World;
use crate::world::arch_spec::ArchetypeSpecs;
use crate::world::query_spec::{QuerySelection, QuerySpecAccessor};

pub mod query_iter;

pub(crate) mod access_scope;
mod src_access;
mod tuple;
mod variant;

/// Every entity carrying the normal components of `T`, plus per-archetype access to the shared
/// components of `S`, optionally narrowed to one shared key by the static filter `F`.
///
/// A query is not `Copy`. With `S = &mut Mesh`, two copies could each hand out a `&mut` to the
/// same payload, so every traversal borrows the query instead:
///
/// ```compile_fail
/// use xynok_ecs::{component, world::World};
/// #[component]
/// struct Hp(u32);
/// let mut world = World::default();
/// let mut query = world.create_query::<&mut Hp>();
/// let first = query.iter();
/// let second = query.iter();
/// drop((first, second));
/// ```
pub struct Query<'a, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static = NoSharedComponent, F: TSharedFilterParam = NoSharedFilter>
{
    accessor: QuerySpecAccessor<'a>,
    phantom:  PhantomData<(T, S, F)>,
}

impl<'a, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static, F: TSharedFilterParam> Query<'a, T, S, F>
{
    pub(crate) fn new(world: &mut World) -> Result<Self, XynokEcsError>
    {
        let accessor = world.get_or_create_query_src_access::<T, S, F>()?;
        Ok(Self {
            accessor: accessor,
            phantom:  PhantomData,
        })
    }

    /// Everything one query touches: the columns of `T`, the shared values of `S`, and the key
    /// `F` filters on. They all share one id space, so this is still a single row selector.
    pub(crate) fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        let mut scope = T::access_scope(component_specs)?;
        scope.extend(S::access_scope(component_specs)?)?;
        // The filter only reads the key. When `S` already writes that component the write
        // covers it, and going through `extend` would wrongly report the pair as an alias.
        if let Some(id) = F::component_id(component_specs)
            && !scope.write.contains(id)
        {
            scope.read.insert(id);
        }
        Ok(scope)
    }

    /// Rows of the normal components, across every matching archetype
    pub fn iter(&mut self) -> QueryIter<'_, T>
    {
        QueryIter::new(self.accessor.selection())
    }

    /// One item per non-empty matching archetype: its shared values, and a view of its chunks
    pub fn iter_archetype(&mut self) -> ArchetypeIter<'_, T, S>
    {
        ArchetypeIter::new(self.accessor.archetypes, self.accessor.arch_indices())
    }

    /// Only the archetypes whose value of `C` has `key`. Panics when `S` and `F` do not read or
    /// write `C`.
    #[track_caller]
    pub fn filter_shared<C: TSharedComponent>(&mut self, key: &C::Key) -> QueryView<'_, T, S>
    {
        QueryView::filtered::<C>(self.accessor, self.accessor.arch_indices(), key)
    }

    /// The archetypes split by their key of `C`, one group per distinct key. Panics when `S` and
    /// `F` do not read or write `C`.
    #[track_caller]
    pub fn group_by_shared<C: TSharedComponent>(&mut self) -> std::vec::IntoIter<(C::Key, QueryView<'_, T, S>)>
    {
        QueryView::grouped::<C>(self.accessor, self.accessor.arch_indices())
    }
}

impl<'a, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static, F: TSharedFilterParam> IntoIterator for Query<'a, T, S, F>
{
    type Item = T::QueryItem<'a>;

    type IntoIter = QueryIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter
    {
        QueryIter::new(self.accessor.selection())
    }
}

/// Part of a query's archetypes, picked by a shared key. It borrows the query it came from.
///
/// Holding its own index list is what lets a runtime filter narrow a query without touching the
/// cached `QuerySpec` other systems read.
pub struct QueryView<'q, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static>
{
    accessor:     QuerySpecAccessor<'q>,
    arch_indices: Vec<usize>,
    phantom:      PhantomData<(T, S)>,
}

impl<'q, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static> QueryView<'q, T, S>
{
    #[track_caller]
    fn filtered<C: TSharedComponent>(accessor: QuerySpecAccessor<'q>, arch_indices: &[usize], key: &C::Key) -> Self
    {
        assert_shared_declared::<C>(&accessor);
        let archetypes: &'q ArchetypeSpecs = accessor.archetypes;
        Self {
            accessor:     accessor,
            arch_indices: arch_indices
                .iter()
                .copied()
                .filter(|&arch_idx| archetypes.value_at(arch_idx).unwrap().shared.key::<C>() == Some(key))
                .collect(),
            phantom:      PhantomData,
        }
    }

    #[track_caller]
    fn grouped<C: TSharedComponent>(accessor: QuerySpecAccessor<'q>, arch_indices: &[usize]) -> std::vec::IntoIter<(C::Key, Self)>
    {
        assert_shared_declared::<C>(&accessor);
        let archetypes: &'q ArchetypeSpecs = accessor.archetypes;
        let mut groups: Vec<(C::Key, Self)> = Vec::new();
        // grouped by interned key id: equal keys always get the same id, so `C::Key` is never hashed again
        let mut group_of_key: HashMap<usize, usize> = HashMap::new();
        for &arch_idx in arch_indices
        {
            let arch = archetypes.value_at(arch_idx).unwrap();
            let Some(value) = arch.shared.value_of::<C>()
            else
            {
                continue;
            };
            if arch.arch.is_empty()
            {
                continue;
            }
            let group = *group_of_key.entry(value.key_id).or_insert_with(|| {
                let view = Self {
                    accessor:     accessor,
                    arch_indices: Vec::new(),
                    phantom:      PhantomData,
                };
                groups.push((arch.shared.key::<C>().unwrap().clone(), view));
                groups.len() - 1
            });
            groups[group].1.arch_indices.push(arch_idx);
        }
        groups.into_iter()
    }

    pub fn iter(&mut self) -> QueryIter<'_, T>
    {
        QueryIter::new(QuerySelection::archetypes(self.accessor.archetypes, &self.arch_indices))
    }

    pub fn iter_archetype(&mut self) -> ArchetypeIter<'_, T, S>
    {
        ArchetypeIter::new(self.accessor.archetypes, &self.arch_indices)
    }

    #[track_caller]
    pub fn filter_shared<C: TSharedComponent>(&mut self, key: &C::Key) -> QueryView<'_, T, S>
    {
        QueryView::filtered::<C>(self.accessor, &self.arch_indices, key)
    }

    #[track_caller]
    pub fn group_by_shared<C: TSharedComponent>(&mut self) -> std::vec::IntoIter<(C::Key, QueryView<'_, T, S>)>
    {
        QueryView::grouped::<C>(self.accessor, &self.arch_indices)
    }
}

#[track_caller]
fn assert_shared_declared<C: TSharedComponent>(accessor: &QuerySpecAccessor)
{
    let component_specs = accessor.component_specs;
    let declared = component_specs.index_of(&TypeId::of::<C>()).is_some_and(|id| {
        component_specs.value_at(id).unwrap().descriptor.storage_location == StorageLocation::Archetype && accessor.spec().access_scope.read_or_write(id)
    });
    assert!(declared, "origin query does not contain shared component `{}`", std::any::type_name::<C>());
}

pub struct ArchetypeIter<'q, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static>
{
    archetypes:   &'q ArchetypeSpecs,
    arch_indices: &'q [usize],
    phantom:      PhantomData<(T, S)>,
}

impl<'q, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static> ArchetypeIter<'q, T, S>
{
    fn new(archetypes: &'q ArchetypeSpecs, arch_indices: &'q [usize]) -> Self
    {
        Self {
            archetypes:   archetypes,
            arch_indices: arch_indices,
            phantom:      PhantomData,
        }
    }
}

impl<'q, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static> Iterator for ArchetypeIter<'q, T, S>
{
    type Item = (S::Item<'q>, ArchetypeView<'q, T>);

    fn next(&mut self) -> Option<Self::Item>
    {
        while let Some((arch_idx, rest)) = self.arch_indices.split_first()
        {
            self.arch_indices = rest;
            let arch = self.archetypes.value_at(*arch_idx).unwrap();
            if arch.arch.is_empty()
            {
                continue;
            }
            // SAFETY: the query only matched archetypes carrying every shared component of `S`.
            // Each archetype is yielded once, this iterator borrows the query, and the scheduler
            // keeps other writers of the same payloads away.
            let shared = unsafe { S::fetch(&arch.shared) };
            let view = ArchetypeView {
                archetypes: self.archetypes,
                arch_idx:   arch_idx,
                phantom:    PhantomData,
            };
            return Some((shared, view));
        }
        None
    }
}

/// The chunks of one archetype
pub struct ArchetypeView<'q, T: TQueryParam + 'static>
{
    archetypes: &'q ArchetypeSpecs,
    arch_idx:   &'q usize,
    phantom:    PhantomData<T>,
}

impl<T: TQueryParam + 'static> ArchetypeView<'_, T>
{
    pub fn iter_chunk(&mut self) -> ChunkIter<'_, T>
    {
        ChunkIter {
            archetypes: self.archetypes,
            arch_idx:   self.arch_idx,
            next_chunk: 0,
            phantom:    PhantomData,
        }
    }

    pub fn iter(&mut self) -> ChunkIter<'_, T>
    {
        self.iter_chunk()
    }
}

pub struct ChunkIter<'q, T: TQueryParam + 'static>
{
    archetypes: &'q ArchetypeSpecs,
    arch_idx:   &'q usize,
    next_chunk: usize,
    phantom:    PhantomData<T>,
}

impl<'q, T: TQueryParam + 'static> Iterator for ChunkIter<'q, T>
{
    type Item = ChunkView<'q, T>;

    fn next(&mut self) -> Option<Self::Item>
    {
        let arch = &self.archetypes.value_at(*self.arch_idx).unwrap().arch;
        while self.next_chunk < arch.chunk_count()
        {
            let chunk_idx = self.next_chunk;
            self.next_chunk += 1;
            if arch.chunk_at(chunk_idx).is_empty()
            {
                continue;
            }
            return Some(ChunkView {
                selection: QuerySelection::chunk(self.archetypes, self.arch_idx, chunk_idx),
                phantom:   PhantomData,
            });
        }
        None
    }
}

/// The rows of one chunk
pub struct ChunkView<'q, T: TQueryParam + 'static>
{
    selection: QuerySelection<'q>,
    phantom:   PhantomData<T>,
}

impl<T: TQueryParam + 'static> ChunkView<'_, T>
{
    pub fn iter(&mut self) -> QueryIter<'_, T>
    {
        QueryIter::new(self.selection)
    }
}

impl<'q, T: TQueryParam + 'static> IntoIterator for ChunkView<'q, T>
{
    type Item = T::QueryItem<'q>;

    type IntoIter = QueryIter<'q, T>;

    fn into_iter(self) -> Self::IntoIter
    {
        QueryIter::new(self.selection)
    }
}
