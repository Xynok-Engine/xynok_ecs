use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TQueryParam;
use crate::apis::params::{
    ArchetypeTakeAndRemoveComponentParams, ArchetypeTakeAndWriteComponentParams, ComponentSpec, EntityInChunkIndices, EntityIndices, SwappedRow,
};
use crate::apis::safe_counter::SafeCounter;
use crate::apis::traits::TArchetype;
use crate::chunk::layout::{ChunkLayout, ChunkLayoutParams};
use crate::entity::Entity;
use crate::query::Query;
use crate::std::queue::Queue;
use crate::utils::normalize_set;
use crate::world::arch_spec::{ArchetypeSpec, PairArchetypeSpecParams};
use crate::world::entity_spec::EntitySpec;
use crate::world::query_spec::{QuerySpec, QuerySpecAccessor};
use crate::world::temp_allocation::WorldTempAllocation;
use crate::world::unsafe_world_cell::UnsafeWorldCell;
mod temp_allocation;
pub(crate) mod entity_spec;
pub(crate) mod arch_spec;
pub(crate) mod query_spec;
pub(crate) mod unsafe_world_cell;

/// Read-only introspection into `World`'s private storage state, for the integration tests
/// under `tests/` (which only ever see the crate's normal public API otherwise)
#[cfg(feature = "test-util")]
pub mod testing;

pub struct World
{
    archetypes:               HashMap<usize, Box<ArchetypeSpec>>,
    #[allow(clippy::box_collection)]
    component_counter:        Box<HashMap<TypeId, ComponentSpec>>,
    component_set_counter:    HashMap<Vec<usize>, usize>,
    archetype_counter:        HashMap<TypeId, usize>,
    query_counter:            HashMap<TypeId, Box<QuerySpec>>,
    entities:                 Vec<EntitySpec>,
    free_entities:            Queue<usize>,
    temp_alloc:               WorldTempAllocation,
    global_archetype_version: SafeCounter,
}
impl Default for World
{
    fn default() -> Self
    {
        Self {
            entities:                 Vec::with_capacity(16),
            archetypes:               HashMap::new(),
            component_counter:        Box::new(HashMap::new()),
            archetype_counter:        HashMap::new(),
            component_set_counter:    HashMap::new(),
            query_counter:            HashMap::new(),
            free_entities:            Queue::new(),
            temp_alloc:               WorldTempAllocation::new(),
            global_archetype_version: SafeCounter::new(1, usize::MAX - 1),
        }
    }
}
impl Drop for World
{
    fn drop(&mut self)
    {
        for arch in self.archetypes.values_mut()
        {
            arch.arch.dispose(&arch.layout, &self.component_counter);
        }
    }
}
impl World
{
    #[track_caller]
    pub fn register_archetype<T: TArchetype + 'static>(&mut self)
    {
        let _ = self.get_or_create_archetype_id::<T>();
    }
    #[track_caller]
    pub fn create<T: TArchetype + 'static>(&mut self, val: T) -> Entity
    {
        let new_e = match self.new_entity()
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        let (arch_id, arch_spec) = self.get_or_create_archetype_spec_mut::<T>();

        let entity_chunk_indices = match arch_spec.arch.push(&arch_spec.layout, new_e, val)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };

        self.update_entity_spec(new_e, arch_id, entity_chunk_indices);
        new_e
    }

    pub fn exists(&mut self, e: Entity) -> bool
    {
        match e.idx() < self.entities.len()
        {
            false => false,
            true =>
            {
                let spec = unsafe { self.entities.get_unchecked(e.idx()) };
                match spec.has_value()
                {
                    false => false,
                    true => spec.version() == e.version(),
                }
            }
        }
    }
    #[track_caller]
    pub fn destroy(&mut self, e: Entity)
    {
        debug_assert!(self.exists(e), "{} does not exist to be destroyed !", e);

        let (arch_id, chunk_idx, idx_in_chunk) = unsafe {
            let spec = self.entities.get_unchecked(e.idx());
            (spec.arch_id(), spec.chunk_idx(), spec.idx_in_chunk())
        };
        let arch = self.archetypes.get_mut(&arch_id).unwrap();
        match arch.arch.remove_at(&arch.layout, &self.component_counter, chunk_idx, idx_in_chunk)
        {
            Ok(r) =>
            {
                if let Some(swapped_row) = r
                {
                    self.update_entity_indices(swapped_row);
                }
            }
            Err(e) => panic!("{}", e),
        };

        self.erase_entity(e);
    }

    #[track_caller]
    pub fn add_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T)
    {
        debug_assert!(self.exists(e), "{} does not exist to add component !", e);

        let (a_arch_id, a_chunk_idx, a_idx_in_chunk) = unsafe {
            let e_spec = self.entities.get_unchecked(e.idx());
            (e_spec.arch_id(), e_spec.chunk_idx(), e_spec.idx_in_chunk())
        };
        let b_arch_id = self.get_or_create_archetype_id::<T>();

        #[cfg(debug_assertions)]
        {
            let has_any_component_duplicated = {
                let a = self.archetypes.get(&a_arch_id).unwrap();
                let b = self.archetypes.get(&b_arch_id).unwrap();
                a.contains_any_component_of(b)
            };
            if has_any_component_duplicated
            {
                panic!(
                    "Cannot add component `{}` for entity {}. A component with this type already exists. 
                    When using add_component(), you can only add components that are not already present. 
                    If you want to add a duplicate component, use merge_component() instead.",
                    std::any::type_name::<T>(),
                    e
                )
            }
        }

        let mut component_set = std::mem::take(&mut self.temp_alloc.vec_usize);
        component_set.clear();

        self.append_archetype_component_id_of_to(a_arch_id, &mut component_set);
        self.append_archetype_component_id_of_to(b_arch_id, &mut component_set);

        normalize_set(&mut component_set);

        let target_arch_id = match self.component_set_counter.get(&component_set)
        {
            Some(r) => *r,
            // create a new arch from these archetypes
            None => self.create_archetype_id_from_set_of(&component_set, a_arch_id, b_arch_id),
        };
        // put back
        self.temp_alloc.vec_usize = component_set;

        let [src_arch_spec, target_arch_spec] = self.archetypes.get_disjoint_mut([&a_arch_id, &target_arch_id]);
        let (src_arch_spec, target_arch_spec) = (src_arch_spec.unwrap(), target_arch_spec.unwrap());

        let src_e_indices = EntityIndices {
            chunk_idx:    a_chunk_idx,
            idx_in_chunk: a_idx_in_chunk,
        };
        let params = ArchetypeTakeAndWriteComponentParams {
            src_e:           src_e_indices,
            src_arch:        &mut src_arch_spec.arch,
            src_layout:      &src_arch_spec.layout,
            dst_layout:      &target_arch_spec.layout,
            component_specs: &self.component_counter,
            write_val:       val,
        };
        let take_and_write_result = match target_arch_spec.arch.take_and_write_from(params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        if let Some(swapped_row) = take_and_write_result.swapped_e
        {
            self.update_entity_indices(swapped_row);
        }
        self.update_entity_spec(e, target_arch_id, take_and_write_result.new_indices_took);
    }

    /// Adds the components of `T` to `e` if they are not already present, otherwise overwrites the
    /// existing values. Unlike `add_component`, this allows `T` to share components with `e`'s current
    /// archetype.
    #[track_caller]
    pub fn merge_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T)
    {
        debug_assert!(self.exists(e), "{} does not exist to merge component !", e);

        let (a_arch_id, a_chunk_idx, a_idx_in_chunk) = unsafe {
            let e_spec = self.entities.get_unchecked(e.idx());
            (e_spec.arch_id(), e_spec.chunk_idx(), e_spec.idx_in_chunk())
        };
        let b_arch_id = self.get_or_create_archetype_id::<T>();

        let mut component_set = std::mem::take(&mut self.temp_alloc.vec_usize);
        component_set.clear();

        self.append_archetype_component_id_of_to(a_arch_id, &mut component_set);
        self.append_archetype_component_id_of_to(b_arch_id, &mut component_set);

        normalize_set(&mut component_set);

        let target_arch_id = match self.component_set_counter.get(&component_set)
        {
            Some(r) => *r,
            // create a new arch from these archetypes
            None => self.create_archetype_id_from_set_of(&component_set, a_arch_id, b_arch_id),
        };
        // put back
        self.temp_alloc.vec_usize = component_set;

        // every component of `T` is already part of `e`'s archetype: overwrite the row in place, no move needed
        if target_arch_id == a_arch_id
        {
            let arch_spec = self.archetypes.get_mut(&a_arch_id).unwrap();
            match arch_spec.arch.replace_at(&arch_spec.layout, a_chunk_idx, a_idx_in_chunk, val)
            {
                Ok(_) =>
                {}
                Err(e) => panic!("{}", e),
            }
            return;
        }

        let [src_arch_spec, target_arch_spec] = self.archetypes.get_disjoint_mut([&a_arch_id, &target_arch_id]);
        let (src_arch_spec, target_arch_spec) = (src_arch_spec.unwrap(), target_arch_spec.unwrap());

        let src_e_indices = EntityIndices {
            chunk_idx:    a_chunk_idx,
            idx_in_chunk: a_idx_in_chunk,
        };
        let params = ArchetypeTakeAndWriteComponentParams {
            src_e:           src_e_indices,
            src_arch:        &mut src_arch_spec.arch,
            src_layout:      &src_arch_spec.layout,
            dst_layout:      &target_arch_spec.layout,
            component_specs: &self.component_counter,
            write_val:       val,
        };
        let take_and_write_result = match target_arch_spec.arch.take_and_write_from(params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        if let Some(swapped_row) = take_and_write_result.swapped_e
        {
            self.update_entity_indices(swapped_row);
        }
        self.update_entity_spec(e, target_arch_id, take_and_write_result.new_indices_took);
    }

    #[track_caller]
    pub fn remove_component<T: TArchetype + 'static>(&mut self, e: Entity) -> T
    {
        debug_assert!(self.exists(e), "{} does not exist to remove component {}", e, std::any::type_name::<T>());
        let (a_arch_id, a_chunk_idx, a_idx_in_chunk) = unsafe {
            let e_spec = self.entities.get_unchecked(e.idx());
            (e_spec.arch_id(), e_spec.chunk_idx(), e_spec.idx_in_chunk())
        };
        let b_arch_id = self.get_or_create_archetype_id::<T>();

        #[cfg(debug_assertions)]
        {
            let contains_all_components = {
                let a = self.archetypes.get(&a_arch_id).unwrap();
                let b = self.archetypes.get(&b_arch_id).unwrap();
                a.contains_all_components_of(b)
            };
            if !contains_all_components
            {
                panic!(
                    "Cannot remove component `{}` for entity {}. Missing component to remove.",
                    std::any::type_name::<T>(),
                    e
                )
            }
        }
        let mut component_set = std::mem::take(&mut self.temp_alloc.vec_usize);
        component_set.clear();
        self.append_archetype_component_id_of_to(a_arch_id, &mut component_set);
        self.retain_archetype_component_id_of_to(b_arch_id, &mut component_set);
        normalize_set(&mut component_set);
        let target_arch_id = match self.component_set_counter.get(&component_set)
        {
            Some(r) => *r,
            // create a new arch from these archetypes
            None => self.create_archetype_id_for_set_of_a_exclude_b(&component_set, a_arch_id, b_arch_id),
        };
        // put back
        self.temp_alloc.vec_usize = component_set;

        let [src_arch_spec, target_arch_spec] = self.archetypes.get_disjoint_mut([&a_arch_id, &target_arch_id]);
        let (src_arch_spec, target_arch_spec) = (src_arch_spec.unwrap(), target_arch_spec.unwrap());

        let src_e_indices = EntityIndices {
            chunk_idx:    a_chunk_idx,
            idx_in_chunk: a_idx_in_chunk,
        };
        let params = ArchetypeTakeAndRemoveComponentParams::<T> {
            src_e:           src_e_indices,
            src_arch:        &mut src_arch_spec.arch,
            src_layout:      &src_arch_spec.layout,
            dst_layout:      &target_arch_spec.layout,
            component_specs: &self.component_counter,
            phantom:         PhantomData,
        };
        let result = match target_arch_spec.arch.take_and_remove_from(params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        if let Some(swapped_row) = result.swapped_e
        {
            self.update_entity_indices(swapped_row);
        }
        self.update_entity_spec(e, target_arch_id, result.new_indices_took);

        result.val
    }

    #[track_caller]
    pub fn create_query<T: TQueryParam + 'static>(&mut self) -> Query<T>
    {
        match Query::new(self)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        }
    }
}

impl World
{
    /// Bumped by `structure_changed()` whenever a new archetype appears. A cached query result is
    /// only valid for the version it was built at.
    pub(crate) fn archetype_version(&self) -> usize
    {
        self.global_archetype_version.current_val()
    }

    /// Hands this world to system params, which need overlapping views that borrowck cannot check.
    /// See [`UnsafeWorldCell`] for what replaces the compiler's guarantee.
    pub(crate) fn as_unsafe_cell(&mut self) -> UnsafeWorldCell<'_>
    {
        UnsafeWorldCell::new(self)
    }

    pub(crate) fn get_or_create_query_src_access<T: TQueryParam + 'static>(&mut self) -> Result<QuerySpecAccessor, XynokEcsError>
    {
        let current_global_arch_version = self.global_archetype_version.current_val();
        let component_specs = self.component_counter.as_ref() as *const _;

        if let Some(query_spec) = self.query_counter.get_mut(&T::TYPE_ID)
        {
            if current_global_arch_version == query_spec.version
            {
                return Ok(query_spec.as_accessor(component_specs));
            }

            query_spec.archetypes.clear();
            crate::utils::build_archetype_which_contains(&mut self.archetypes, &mut query_spec.archetypes, &query_spec.access_scope);
            query_spec.version = current_global_arch_version;
            return Ok(query_spec.as_accessor(component_specs));
        }
        let mut target_archetypes: Vec<*mut ArchetypeSpec> = Vec::new();
        let access_scope = T::access_scope()?;
        crate::utils::build_archetype_which_contains(&mut self.archetypes, &mut target_archetypes, &access_scope);
        let spec = QuerySpec {
            access_scope: access_scope,
            archetypes:   target_archetypes,
            version:      current_global_arch_version,
        };

        let query_spec = self.query_counter.entry(T::TYPE_ID).or_insert(Box::new(spec));
        Ok(query_spec.as_accessor(component_specs))
    }
}
impl World
{
    fn update_entity_spec(&mut self, e: Entity, arch_id: usize, indices: EntityInChunkIndices)
    {
        let entity_spec = unsafe { self.entities.get_unchecked_mut(e.idx()) };
        *entity_spec = EntitySpec::new(arch_id, indices.chunk_idx, indices.idx_in_chunk, e.version());
    }
    fn erase_entity(&mut self, e: Entity)
    {
        self.free_entities.enqueue(e.idx());
        unsafe {
            let e_spec = self.entities.get_unchecked_mut(e.idx());
            e_spec.errase();
        };
    }
    #[track_caller]
    fn retain_archetype_component_id_of_to(&self, arch_id: usize, component_set: &mut Vec<usize>)
    {
        let arch_spec = match self.archetypes.get(&arch_id)
        {
            Some(r) => r,
            None => panic!("Archetype `{arch_id}` not found to retain component id !"),
        };

        for type_id in arch_spec.layout.component_col_descriptors.keys()
        {
            match self.component_counter.get(type_id)
            {
                Some(component_spec) => component_set.retain(|e| *e != component_spec.id),
                None => panic!("Archetype `{arch_id}` has an unregistered component to check id !"),
            }
        }
    }

    #[track_caller]
    fn append_archetype_component_id_of_to(&self, arch_id: usize, component_set: &mut Vec<usize>)
    {
        let arch_spec = match self.archetypes.get(&arch_id)
        {
            Some(r) => r,
            None => panic!("Archetype `{arch_id}` not found to collect component id !"),
        };

        for type_id in arch_spec.layout.component_col_descriptors.keys()
        {
            match self.component_counter.get(type_id)
            {
                Some(component_spec) => component_set.push(component_spec.id),
                None => panic!("Archetype `{arch_id}` has an unregistered component to check id !"),
            }
        }
    }
    #[track_caller]
    fn update_entity_indices(&mut self, swapped_row: SwappedRow)
    {
        let swapped_e_spec = match self.entities.get_mut(swapped_row.e.idx())
        {
            Some(r) => r,
            None => panic!("Swapped {} not found to update indices !", swapped_row.e),
        };

        swapped_e_spec.update_idx_in_chunk(swapped_row.from, swapped_row.to);
    }

    fn new_entity(&mut self) -> Result<Entity, XynokEcsError>
    {
        if let Some(free_idx) = self.free_entities.dequeue()
        {
            let old_slot = unsafe { self.entities.get_unchecked_mut(free_idx) };
            return Entity::new(free_idx, old_slot.version() + 1);
        };
        let e = Entity::new(self.entities.len(), Entity::INITIALIZE_VERSION)?;
        self.entities.push(EntitySpec::new_empty_slot(e.version()));
        Ok(e)
    }

    fn get_archetype_id<T: TArchetype + 'static>(&self) -> Option<usize>
    {
        if let Some(spec) = self.archetype_counter.get(&std::any::TypeId::of::<T>())
        {
            return Some(*spec);
        }
        None
    }

    #[track_caller]
    fn get_or_create_archetype_spec_mut<T: TArchetype + 'static>(&mut self) -> (usize, &mut ArchetypeSpec)
    {
        let arch_id = self.get_or_create_archetype_id::<T>();
        (arch_id, self.archetypes.get_mut(&arch_id).unwrap())
    }
    #[track_caller]
    fn get_or_create_archetype_id<T: TArchetype + 'static>(&mut self) -> usize
    {
        match self.get_archetype_id::<T>()
        {
            Some(r) => r,
            None => self.create_archetype_id::<T>(),
        }
    }

    #[track_caller]
    fn create_archetype_id<T: TArchetype + 'static>(&mut self) -> usize
    {
        let component_set = &mut self.temp_alloc.vec_usize;
        component_set.clear();

        // collect component id
        for des in T::COMPONENT_DESCRIPTORS
        {
            if let Some(spec) = self.component_counter.get(&des.storage_type_id)
            {
                component_set.push(spec.id);
                continue;
            }
            let id = self.component_counter.len();

            self.component_counter.insert(
                des.storage_type_id,
                ComponentSpec {
                    id:         id,
                    descriptor: des.clone(),
                },
            );
            component_set.push(id);
        }
        normalize_set(component_set);

        match self.component_set_counter.get(component_set)
        {
            Some(r) =>
            {
                let arch_id = *r;
                self.archetype_counter.insert(std::any::TypeId::of::<T>(), arch_id);
                arch_id
            }
            None =>
            {
                let arch_id = self.component_set_counter.len();
                self.component_set_counter.insert(component_set.clone(), arch_id);
                self.archetype_counter.insert(std::any::TypeId::of::<T>(), arch_id);
                self.create_archetype::<T>(arch_id);
                self.structure_changed();
                arch_id
            }
        }
    }
    fn create_archetype_id_for_set_of_a_exclude_b(&mut self, component_set: &[usize], a_arch_id: usize, b_arch_id: usize) -> usize
    {
        let merge_arch_params = PairArchetypeSpecParams {
            a:                              self.archetypes.get(&a_arch_id).unwrap(),
            b:                              self.archetypes.get(&b_arch_id).unwrap(),
            component_specs:                &self.component_counter,
            temp_tys:                       &mut self.temp_alloc.hashset_type_ids,
            temp_comp_des:                  &mut self.temp_alloc.comp_descriptors,
            component_col_descriptors_temp: &mut self.temp_alloc.col_descriptors,
        };
        let new_arch = match ArchetypeSpec::new_from_a_exclude_b_components(merge_arch_params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        let arch_id = self.component_set_counter.len();
        self.component_set_counter.insert(component_set.to_vec(), arch_id);
        self.archetypes.insert(arch_id, Box::new(new_arch));
        self.structure_changed();
        arch_id
    }
    fn create_archetype_id_from_set_of(&mut self, component_set: &[usize], a_arch_id: usize, b_arch_id: usize) -> usize
    {
        let merge_arch_params = PairArchetypeSpecParams {
            a:                              self.archetypes.get(&a_arch_id).unwrap(),
            b:                              self.archetypes.get(&b_arch_id).unwrap(),
            component_specs:                &self.component_counter,
            temp_tys:                       &mut self.temp_alloc.hashset_type_ids,
            temp_comp_des:                  &mut self.temp_alloc.comp_descriptors,
            component_col_descriptors_temp: &mut self.temp_alloc.col_descriptors,
        };
        let new_arch = match ArchetypeSpec::new_from_pair(merge_arch_params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        let arch_id = self.component_set_counter.len();
        self.component_set_counter.insert(component_set.to_vec(), arch_id);
        self.archetypes.insert(arch_id, Box::new(new_arch));
        self.structure_changed();
        arch_id
    }
    #[track_caller]
    fn create_archetype<T: TArchetype + 'static>(&mut self, id: usize)
    {
        let params = ChunkLayoutParams {
            components:                 T::COMPONENT_DESCRIPTORS,
            component_descriptors_temp: &mut self.temp_alloc.col_descriptors,
        };
        let layout = match ChunkLayout::new(params)
        {
            Ok(r) => r,
            Err(e) => panic!("Create Archetype `{}` Failed: {e}", std::any::type_name::<T>()),
        };
        let arch_spec = ArchetypeSpec::new(layout);
        self.archetypes.insert(id, Box::new(arch_spec));
    }

    fn structure_changed(&mut self)
    {
        self.global_archetype_version.increase();
    }
}
