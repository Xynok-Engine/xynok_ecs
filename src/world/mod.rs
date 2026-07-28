use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use crate::{
    apis::{
        swapped_row::{self, SwappedRow},
        TArchetype,
    },
    archetype::Archetype,
    chunk::{
        layout::{ChunkLayout, ChunkLayoutParams},
        Chunk,
    },
    entity::Entity,
    world::{arch_spec::ArchetypeSpec, entity_spec::EntitySpec, temp_allocation::WorldTempAllocation},
};
mod temp_allocation;
mod entity_spec;
mod arch_spec;
pub struct World
{
    archetypes:               HashMap<usize, ArchetypeSpec>,
    component_counter:        HashMap<TypeId, usize>,
    component_set_counter:    HashMap<Vec<usize>, usize>,
    archetype_counter:        HashMap<TypeId, usize>,
    entities:                 HashMap<Entity, EntitySpec>,
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
            entities:                 HashMap::with_capacity(16),
            archetypes:               HashMap::new(),
            component_counter:        HashMap::new(),
            archetype_counter:        HashMap::new(),
            component_set_counter:    HashMap::new(),
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

        self.entities.insert(
            new_e,
            EntitySpec {
                arch_id:         arch_id,
                chunk_idx:       arch_to_chunk_spec.chunk_idx,
                idx_in_chunk:    arch_to_chunk_spec.idx_in_chunk,
                current_version: new_e.version(),
            },
        );
        new_e
    }

    #[track_caller]
    pub fn destroy(&mut self, e: Entity)
    {
        let e_spec = match self.entities.get(&e)
        {
            Some(r) => r,
            None => panic!("{} does not exist to be destroyed !", e),
        };
        let arch = self.archetypes.get_mut(&e_spec.arch_id).unwrap();
        match arch.arch.remove_at(arch.fn_remove, &arch.layout, e_spec.chunk_idx, e_spec.chunk_idx)
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
    }
}

impl World
{
    #[track_caller]
    fn update_entity_indices(&mut self, swapped_row: SwappedRow)
    {
        let swapped_e_spec = match self.entities.get_mut(&swapped_row.e)
        {
            Some(r) => r,
            None => panic!("Swapped {} not found to update indices !", swapped_row.e),
        };
        debug_assert!(
            swapped_row.from == swapped_e_spec.idx_in_chunk,
            "{} with old idx `{}` != swapped idx `{}`",
            swapped_row.e,
            swapped_e_spec.idx_in_chunk,
            swapped_row.from
        );
        swapped_e_spec.idx_in_chunk = swapped_row.to;
    }
    fn new_entity(&self) -> Entity
    {
        Entity::new(self.entities.len(), Entity::INITIALIZE_VERSION)
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
        for comp in T::STORAGE_TYPE_IDS
        {
            if let Some(id) = self.component_counter.get(comp)
            {
                component_set.push(*id);
                continue;
            }
            let id = self.component_counter.len();
            self.component_counter.insert(*comp, id);
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
