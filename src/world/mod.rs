use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use crate::{
    apis::{
        component_spec::ComponentSpec,
        swapped_row::{self, SwappedRow},
        traits::TArchetype,
    },
    archetype::Archetype,
    chunk::{
        layout::{ChunkLayout, ChunkLayoutParams},
        Chunk,
    },
    entity::Entity,
    std::queue::Queue,
    world::{arch_spec::ArchetypeSpec, entity_spec::EntitySpec, temp_allocation::WorldTempAllocation},
};

mod temp_allocation;
mod entity_spec;
mod arch_spec;

pub struct World
{
    archetypes:               HashMap<usize, ArchetypeSpec>,
    component_counter:        HashMap<TypeId, ComponentSpec>,
    component_set_counter:    HashMap<Vec<usize>, usize>,
    archetype_counter:        HashMap<TypeId, usize>,
    entities:                 Vec<EntitySpec>,
    free_entities:            Queue<usize>,
    temp_alloc:               WorldTempAllocation,
    global_archetype_version: usize,
}

fn normalize_set(set: &mut Vec<usize>)
{
    set.sort();
    set.dedup();
}

impl Default for World
{
    fn default() -> Self
    {
        Self {
            entities:                 Vec::with_capacity(16),
            archetypes:               HashMap::new(),
            component_counter:        HashMap::new(),
            archetype_counter:        HashMap::new(),
            component_set_counter:    HashMap::new(),
            free_entities:            Queue::new(),
            temp_alloc:               WorldTempAllocation::new(),
            global_archetype_version: 1usize,
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
        let new_e = self.new_entity();
        let (arch_id, arch_spec) = self.get_or_create_archetype_spec_mut::<T>();

        let arch_to_chunk_spec = match arch_spec.arch.push(&arch_spec.layout, new_e, val)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };

        let entity_spec = unsafe { self.entities.get_unchecked_mut(new_e.idx()) };
        *entity_spec = EntitySpec::new(arch_id, arch_to_chunk_spec.chunk_idx, arch_to_chunk_spec.idx_in_chunk, new_e.version());
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
        match arch
            .arch
            .remove_at(&arch.fn_remove_entity, &arch.layout, &self.component_counter, chunk_idx, idx_in_chunk)
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

        self.free_entities.enqueue(e.idx());
        unsafe {
            let e_spec = self.entities.get_unchecked_mut(e.idx());
            e_spec.errase();
        };
    }

    #[track_caller]
    pub fn add_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T)
    {
        debug_assert!(self.exists(e), "{} does not exist to add component !", e);

        let a_arch_id = unsafe { self.entities.get_unchecked(e.idx()).arch_id() };
        let b_arch_id = self.get_or_create_archetype_id::<T>();

        let mut component_set = std::mem::take(&mut self.temp_alloc.vec_usize);
        component_set.clear();
        self.append_archetype_component_id_of_to(a_arch_id, &mut component_set);
        self.append_archetype_component_id_of_to(b_arch_id, &mut component_set);
        normalize_set(&mut component_set);

        let target_arch_id = match self.component_set_counter.get(&component_set)
        {
            Some(r) => r,
            None =>
            {
                panic!(
                    "Target archetype after adding `{}` was not registered. Please register it before adding or removing any components from another archetype to become it.",
                    std::any::type_name::<T>()
                );
            }
        };
        let target_arch_spec = self.archetypes.get_mut(target_arch_id).unwrap();
        self.temp_alloc.vec_usize = component_set;
    }

    #[track_caller]
    pub fn remove_component<T: TArchetype + 'static>(&mut self, e: Entity) {}
}

impl World
{
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
    fn new_entity(&mut self) -> Entity
    {
        if let Some(free_idx) = self.free_entities.dequeue()
        {
            let old_slot = unsafe { self.entities.get_unchecked(free_idx) };
            let new_version = (old_slot.version() + 1).max(Entity::INITIALIZE_VERSION);
            return Entity::new(free_idx, new_version);
        };
        let e = Entity::new(self.entities.len(), Entity::INITIALIZE_VERSION);
        self.entities.push(EntitySpec::new_empty_slot(e.version()));
        e
    }
}
impl World
{
    fn get_archetype_id<T: TArchetype + 'static>(&self) -> Option<usize>
    {
        self.archetype_counter.get(&std::any::TypeId::of::<T>()).copied()
    }
    fn get_archetype_spec<T: TArchetype + 'static>(&self) -> Option<&ArchetypeSpec>
    {
        match self.get_archetype_id::<T>()
        {
            Some(arch_id) => self.archetypes.get(&arch_id),
            None => None,
        }
    }
    fn get_archetype_spec_mut<T: TArchetype + 'static>(&mut self) -> Option<&mut ArchetypeSpec>
    {
        match self.get_archetype_id::<T>()
        {
            Some(arch_id) => self.archetypes.get_mut(&arch_id),
            None => None,
        }
    }
}
impl World
{
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
                    id:      id,
                    fn_drop: des.fn_drop,
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

    #[track_caller]
    fn create_archetype<T: TArchetype + 'static>(&mut self, id: usize)
    {
        let params = ChunkLayoutParams {
            arch:                       T::COMPONENT_DESCRIPTORS,
            component_descriptors_temp: &mut self.temp_alloc.col_descriptors,
        };
        let layout = match ChunkLayout::new(params)
        {
            Ok(r) => r,
            Err(e) => panic!("Create Archetype `{}` Failed: {e}", std::any::type_name::<T>()),
        };
        let arch_spec = ArchetypeSpec::new::<T>(layout);
        self.archetypes.insert(id, arch_spec);
    }

    fn structure_changed(&mut self)
    {
        self.global_archetype_version += 1;
    }
}

#[cfg(test)]
mod test
{
    use std::collections::HashSet;

    use crate::world::normalize_set;

    #[test]
    fn unique_component_set()
    {
        let mut a = Vec::from([1, 2, 3]);
        let mut b = Vec::from([1, 3, 2]);
        let mut c = Vec::from([1, 3, 2, 3]);
        normalize_set(&mut a);
        normalize_set(&mut b);
        normalize_set(&mut c);
        let dict = HashSet::from([a, b, c]);
        assert!(dict.len() == 1);

        let mut d = Vec::from([1, 1, 2, 3, 2, 3]);
        assert!(!dict.contains(&d));
        normalize_set(&mut d);
        assert!(dict.contains(&d));
        println!("dict: {:?}", dict);
    }
}
