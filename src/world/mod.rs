use std::{
    any::TypeId,
    collections::{HashMap, HashSet},
};

use crate::{
    apis::TArchetype,
    chunk::Chunk,
    entity::Entity,
    world::{archetype::Archetype, temp_allocation::WorldTempAllocation},
};
mod archetype;
mod temp_allocation;
pub struct World
{
    archetypes:               HashMap<usize, Archetype>,
    component_counter:        HashMap<TypeId, usize>,
    component_set_counter:    HashMap<Vec<usize>, usize>,
    archetype_counter:        HashMap<TypeId, usize>,
    temp_alloc:               WorldTempAllocation,
    global_archetype_version: usize,
}

fn normalize_set(set: &mut Vec<usize>)
{
    set.sort();
    set.dedup();
}

impl World
{
    pub fn new() -> Self
    {
        Self {
            archetypes:               HashMap::new(),
            component_counter:        HashMap::new(),
            archetype_counter:        HashMap::new(),
            component_set_counter:    HashMap::new(),
            temp_alloc:               WorldTempAllocation::new(),
            global_archetype_version: 0,
        }
    }

    pub fn register<T: TArchetype + 'static>(&mut self)
    {
        let _ = self.get_or_create_archetype_id::<T>();
    }

    pub fn create<T: TArchetype + 'static>(&mut self, val: T) -> Entity
    {
        let arch_id = self.get_or_create_archetype_id::<T>();
        todo!()
    }
}

impl World
{
    fn create_archetype<T: TArchetype + 'static>(&mut self) {}
}
impl World
{
    fn get_or_create_archetype_id<T: TArchetype + 'static>(&mut self) -> usize
    {
        match self.get_archetype_id::<T>()
        {
            Some(r) => r,
            None => self.create_archetype_id::<T>(),
        }
    }
    fn get_archetype_id<T: TArchetype + 'static>(&self) -> Option<usize>
    {
        self.archetype_counter.get(&std::any::TypeId::of::<T>()).copied()
    }

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

        let arch_id = match self.component_set_counter.get(component_set)
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
                arch_id
            }
        };
        self.structure_changed();
        arch_id
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
