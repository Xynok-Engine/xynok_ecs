//! Structural operations on shared components.
//!
//! An archetype is identified by its normal component set plus the shared values its entities
//! use. Archetypes without shared values are still found through `component_set_counter`.
//! Shared variants are found through `shared_archetype_counter`, keyed by the same component
//! set and the sorted list of their [`SharedValue`]s.
//!
//! The first time the world sees a key, it gets a small id per shared type. Archetype lookup then
//! hashes plain integers, and never needs to compare user keys through function pointers.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TQueryParam;
use crate::apis::params::{ArchetypeTakeAndWriteComponentParams, EntityIndices};
use crate::archetype_chunk::{ArchetypeChunk, SharedValue};
use crate::entity::Entity;
use crate::query::Query;
use crate::shared::{TSharedComponent, TSharedComponentQueryParam, TSharedFilter};
use crate::world::World;
use crate::world::arch_spec::ArchetypeSpec;

/// One `HashMap<C::Key, usize>` per shared type `C`, stored under `TypeId::of::<C>()`. Ids are
/// handed out in order and never reused, so a table grows with the number of distinct keys the
/// world has seen, not with the number of live ones.
pub(crate) type SharedKeyTables = HashMap<TypeId, Box<dyn Any + Send + Sync>>;

/// The normal component set, then the shared values sorted by component id
pub(crate) type SharedArchetypeKey = (Vec<usize>, Vec<SharedValue>);

/// Where an entity lands after a shared change
enum Destination
{
    Existing(usize),
    /// No archetype has this identity yet
    Missing(SharedArchetypeKey),
}

impl World
{
    #[track_caller]
    pub fn create_shared_query<'a, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static>(&'a mut self) -> Query<'a, T, S>
    {
        match Query::new(self)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        }
    }

    #[track_caller]
    pub fn create_filtered_query<'a, T: TQueryParam + 'static, S: TSharedComponentQueryParam + 'static, F: TSharedFilter>(
        &'a mut self,
    ) -> Query<'a, T, S, F>
    {
        match Query::new(self)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        }
    }

    /// Adds the shared component `value` to `e`.
    ///
    /// The destination is the archetype with `e`'s components, its other shared values and the
    /// key of `value`. When it does not exist yet, it is created with `value`. When it already
    /// exists it already has a value for that key, so this fails with `SharedDestinationExists`
    /// and drops `value`: join it with [`Self::add_shared_component_by_key`] instead.
    pub fn add_shared_component<C: TSharedComponent>(&mut self, e: Entity, value: C) -> Result<(), XynokEcsError>
    {
        let src = self.arch_id_without::<C>(e)?;
        let key = value.shared_key();
        let new_value = self.shared_value_of::<C>(&key);
        let identity = match self.destination_of(src, new_value.component_id, Some(new_value))
        {
            Destination::Existing(_) => return Err(XynokEcsError::SharedDestinationExists),
            Destination::Missing(r) => r,
        };
        // built before `e` leaves `src`: leaving may empty `src` and drop the values cloned here
        let shared = ArchetypeChunk::rebuild::<C>(&self.archetypes.get(&src).unwrap().shared, Some((new_value, key, value)));
        let dst = self.insert_shared_archetype(src, identity, shared);
        self.move_entity_to(e, src, dst);
        Ok(())
    }

    /// Adds the shared component `C` to `e` by moving it into the archetype that already holds
    /// `key` for `e`'s components and other shared values.
    ///
    /// Fails with `UnresolvedSharedKey` when that archetype does not exist, because there is no
    /// value to give it. Create it with [`Self::add_shared_component`].
    pub fn add_shared_component_by_key<C: TSharedComponent>(&mut self, e: Entity, key: C::Key) -> Result<(), XynokEcsError>
    {
        let src = self.arch_id_without::<C>(e)?;
        let dst = self.existing_destination_of::<C>(src, &key)?;
        self.move_entity_to(e, src, dst);
        Ok(())
    }

    /// Removes the shared component `C` from `e`. Its other shared values stay.
    pub fn remove_shared_component<C: TSharedComponent>(&mut self, e: Entity) -> Result<(), XynokEcsError>
    {
        let src = self.arch_id_with::<C>(e)?;
        let component_id = self.archetypes.get(&src).unwrap().shared.value_of::<C>().unwrap().component_id;
        let dst = match self.destination_of(src, component_id, None)
        {
            Destination::Existing(r) => r,
            Destination::Missing(identity) =>
            {
                let shared = ArchetypeChunk::rebuild::<C>(&self.archetypes.get(&src).unwrap().shared, None);
                self.insert_shared_archetype(src, identity, shared)
            }
        };
        self.move_entity_to(e, src, dst);
        Ok(())
    }

    /// Moves `e` to the archetype that holds `key` for `C` and keeps everything else the same.
    /// Setting the key `e` already has does nothing.
    ///
    /// Fails with `UnresolvedSharedKey` when that archetype does not exist, because there is no
    /// value to give it. Remove `C` and add the new value with [`Self::add_shared_component`].
    pub fn set_shared_component_by_key<C: TSharedComponent>(&mut self, e: Entity, key: C::Key) -> Result<(), XynokEcsError>
    {
        let src = self.arch_id_with::<C>(e)?;
        if self.archetypes.get(&src).unwrap().shared.key::<C>() == Some(&key)
        {
            return Ok(());
        }
        let dst = self.existing_destination_of::<C>(src, &key)?;
        self.move_entity_to(e, src, dst);
        Ok(())
    }

    /// Replaces the value of `C` that `e` uses. Every entity of `e`'s archetype sees the new
    /// value. Archetypes with the same key but other components keep their own.
    ///
    /// Fails with `SharedKeyMismatch` when `value` gives another key than the one the archetype
    /// was created with, and leaves the current value alone. Move to another key with
    /// [`Self::set_shared_component_by_key`], or skip the check with
    /// [`Self::override_shared_component`].
    pub fn set_shared_component<C: TSharedComponent>(&mut self, e: Entity, value: C) -> Result<(), XynokEcsError>
    {
        let src = self.arch_id_with::<C>(e)?;
        let (key, _) = self.archetypes.get(&src).unwrap().shared.entry_of::<C>().unwrap();
        // SAFETY: the entry lives as long as the archetype, and `&mut self` rules out any view writing it
        if unsafe { *key != value.shared_key() }
        {
            return Err(XynokEcsError::SharedKeyMismatch);
        }
        self.write_shared_value(src, value);
        Ok(())
    }

    /// Like [`Self::set_shared_component`], without the key check.
    ///
    /// The archetype keeps the key it was created with. The `*_by_key` methods, `filter_shared`
    /// and `group_by_shared` still find it through that key, even when `value` gives another one.
    pub fn override_shared_component<C: TSharedComponent>(&mut self, e: Entity, value: C) -> Result<(), XynokEcsError>
    {
        let src = self.arch_id_with::<C>(e)?;
        self.write_shared_value(src, value);
        Ok(())
    }

    /// The value of `C` that `e` uses, or `None` when `e` does not exist or does not carry `C`
    pub fn get_shared_component<C: TSharedComponent>(&self, e: Entity) -> Option<&C>
    {
        if !self.exists(e)
        {
            return None;
        }
        let (_, value) = self.archetypes.get(&self.entities[e.idx()].arch_id()).unwrap().shared.entry_of::<C>()?;
        // SAFETY: the returned borrow keeps the world shared, so nothing can write the value meanwhile
        Some(unsafe { &*value })
    }

    pub fn has_shared_component<C: TSharedComponent>(&self, e: Entity) -> bool
    {
        self.get_shared_component::<C>(e).is_some()
    }

    /// The shared value `key` stands for, interning the key and registering `C` on first sight
    pub(crate) fn shared_value_of<C: TSharedComponent>(&mut self, key: &C::Key) -> SharedValue
    {
        let component_id = self.component_counter.register_shared::<C>();
        let table = self
            .shared_keys
            .entry(TypeId::of::<C>())
            .or_insert_with(|| Box::new(HashMap::<C::Key, usize>::new()));
        let ids = match table.downcast_mut::<HashMap<C::Key, usize>>()
        {
            Some(r) => r,
            None => panic!("shared key table of `{}` is stored under the wrong type", std::any::type_name::<C>()),
        };
        let next_id = ids.len();
        let key_id = match ids.get(key)
        {
            Some(id) => *id,
            None =>
            {
                ids.insert(key.clone(), next_id);
                next_id
            }
        };
        SharedValue {
            component_id: component_id,
            key_id:       key_id,
        }
    }

    /// The shared value `key` stands for, without interning it. `None` when the world has never
    /// seen the key, in which case no archetype can hold it either.
    fn interned_value_of<C: TSharedComponent>(&self, key: &C::Key) -> Option<SharedValue>
    {
        let ids = self.shared_keys.get(&TypeId::of::<C>())?.downcast_ref::<HashMap<C::Key, usize>>()?;
        Some(SharedValue {
            component_id: self.component_counter.index_of(&TypeId::of::<C>())?,
            key_id:       *ids.get(key)?,
        })
    }

    /// Where `src` leads once its value of `component_id` is replaced by `new_value`, or removed
    /// when `new_value` is `None`
    fn destination_of(&self, src: usize, component_id: usize, new_value: Option<SharedValue>) -> Destination
    {
        let source = self.archetypes.get(&src).unwrap();
        let components: Vec<usize> = source.layout.component_bit_set.iter().collect();
        let mut values: Vec<SharedValue> = source.shared.values().filter(|value| value.component_id != component_id).collect();
        values.extend(new_value);
        values.sort_unstable();

        if values.is_empty()
        {
            // without shared values the entity goes back to the plain archetype of its
            // components, which always exists: every shared variant was made from one
            return match self.component_set_counter.get(&components)
            {
                Some(r) => Destination::Existing(*r),
                None => panic!("shared archetype `{src}` has no plain archetype to return to"),
            };
        }
        let identity: SharedArchetypeKey = (components, values);
        match self.shared_archetype_counter.get(&identity)
        {
            Some(r) => Destination::Existing(*r),
            None => Destination::Missing(identity),
        }
    }

    /// The archetype `src` leads to with its value of `C` replaced by `key`, which must exist
    fn existing_destination_of<C: TSharedComponent>(&self, src: usize, key: &C::Key) -> Result<usize, XynokEcsError>
    {
        let Some(new_value) = self.interned_value_of::<C>(key)
        else
        {
            return Err(XynokEcsError::UnresolvedSharedKey);
        };
        match self.destination_of(src, new_value.component_id, Some(new_value))
        {
            Destination::Existing(r) => Ok(r),
            Destination::Missing(_) => Err(XynokEcsError::UnresolvedSharedKey),
        }
    }

    fn write_shared_value<C: TSharedComponent>(&mut self, arch_id: usize, value: C)
    {
        let (_, slot) = self.archetypes.get(&arch_id).unwrap().shared.entry_of::<C>().unwrap();
        // SAFETY: `&mut self` rules out any view of the value. The new value is in place before
        // the old one is dropped, so a panicking drop still leaves a live value in the slot.
        let old = unsafe { std::ptr::replace(slot, value) };
        drop(old);
    }

    /// The archetype with `base`'s components and `src`'s shared values.
    ///
    /// Called when a normal component is added to or removed from an entity: `base` is where the
    /// new component set lives, and the entity must keep its shared keys on the way there.
    pub(super) fn with_shared_values_of(&mut self, src: usize, base: usize) -> usize
    {
        let source = &self.archetypes.get(&src).unwrap().shared;
        if source.is_empty()
        {
            return base;
        }
        let identity: SharedArchetypeKey = (
            self.archetypes.get(&base).unwrap().layout.component_bit_set.iter().collect(),
            source.values().collect(),
        );
        if let Some(arch_id) = self.shared_archetype_counter.get(&identity)
        {
            return *arch_id;
        }
        // the new archetype starts with its own copies, so later edits on either side stay apart
        let shared = source.clone();
        self.insert_shared_archetype(base, identity, shared)
    }

    /// Registers a shared variant with the layout of `layout_of`. An empty variant with the same
    /// components is reused when one is waiting, so changing keys over and over does not keep
    /// growing the archetype registry.
    fn insert_shared_archetype(&mut self, layout_of: usize, identity: SharedArchetypeKey, shared: ArchetypeChunk) -> usize
    {
        let reused = self.free_shared_archetypes.get_mut(&identity.0).and_then(Vec::pop);
        let arch_id = match reused
        {
            Some(arch_id) =>
            {
                self.archetypes.get_mut(&arch_id).unwrap().shared = shared;
                arch_id
            }
            None =>
            {
                let mut spec = ArchetypeSpec::new(self.archetypes.get(&layout_of).unwrap().layout.clone());
                spec.shared = shared;
                let arch_id = self.archetypes.len();
                self.archetypes.insert(arch_id, spec);
                arch_id
            }
        };
        self.shared_archetype_counter.insert(identity, arch_id);
        self.structure_changed();
        arch_id
    }

    fn move_entity_to(&mut self, e: Entity, src: usize, dst: usize)
    {
        if src == dst
        {
            return;
        }
        let e_spec = &self.entities[e.idx()];
        let src_e_indices = EntityIndices {
            chunk_idx:    e_spec.chunk_idx(),
            idx_in_chunk: e_spec.idx_in_chunk(),
        };
        let src_idx = self.archetypes.index_of(&src).unwrap();
        let dst_idx = self.archetypes.index_of(&dst).unwrap();
        let [src_arch_spec, dst_arch_spec] = self.archetypes.values_mut_slice().get_disjoint_mut([src_idx, dst_idx]).unwrap();
        let params = ArchetypeTakeAndWriteComponentParams {
            src_e:           src_e_indices,
            src_arch:        &mut src_arch_spec.arch,
            src_layout:      &src_arch_spec.layout,
            dst_layout:      &dst_arch_spec.layout,
            component_specs: &self.component_counter,
            write_val:       (),
        };
        let result = match dst_arch_spec.arch.take_and_write_from(params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        if let Some(swapped_row) = result.swapped_e
        {
            self.update_entity_indices(swapped_row);
        }
        self.update_entity_spec(e, dst, result.new_indices_took);
        self.release_if_empty(src);
    }

    /// Drops the shared values of `arch_id` once its last entity has left, and parks the
    /// archetype for reuse. A shared value lives exactly as long as some entity uses it, the
    /// same way an `Rc` does.
    pub(super) fn release_if_empty(&mut self, arch_id: usize)
    {
        let spec = self.archetypes.get_mut(&arch_id).unwrap();
        if spec.shared.is_empty() || !spec.arch.is_empty()
        {
            return;
        }
        let shared = std::mem::take(&mut spec.shared);
        let components: Vec<usize> = spec.layout.component_bit_set.iter().collect();
        self.shared_archetype_counter.remove(&(components.clone(), shared.values().collect::<Vec<_>>()));
        self.free_shared_archetypes.entry(components).or_default().push(arch_id);
        drop(shared);
        self.structure_changed();
    }

    /// The archetype of `e`, which must carry `C`
    fn arch_id_with<C: TSharedComponent>(&self, e: Entity) -> Result<usize, XynokEcsError>
    {
        let arch_id = self.arch_id_of(e)?;
        match self.archetypes.get(&arch_id).unwrap().shared.value_of::<C>()
        {
            Some(_) => Ok(arch_id),
            None => Err(XynokEcsError::SharedComponentDoesNotExist),
        }
    }

    /// The archetype of `e`, which must not carry `C` yet
    fn arch_id_without<C: TSharedComponent>(&self, e: Entity) -> Result<usize, XynokEcsError>
    {
        let arch_id = self.arch_id_of(e)?;
        match self.archetypes.get(&arch_id).unwrap().shared.value_of::<C>()
        {
            Some(_) => Err(XynokEcsError::SharedComponentAlreadyExists),
            None => Ok(arch_id),
        }
    }

    fn arch_id_of(&self, e: Entity) -> Result<usize, XynokEcsError>
    {
        if !self.exists(e)
        {
            return Err(XynokEcsError::EntityDoesNotExist);
        }
        Ok(self.entities[e.idx()].arch_id())
    }
}

#[cfg(test)]
mod test
{
    use std::any::TypeId;

    use crate::apis::identifies::{StorageLocation, XynokEcsError};
    use crate::shared::TSharedComponent;
    use crate::world::World;

    #[crate::component]
    struct Hp(u32);
    #[crate::component]
    struct Mana(u32);

    #[derive(Clone)]
    struct Mesh(u32);
    impl TSharedComponent for Mesh
    {
        type Key = u32;
        fn shared_key(&self) -> u32
        {
            self.0
        }
    }
    #[derive(Clone)]
    struct Material(u32);
    impl TSharedComponent for Material
    {
        type Key = u32;
        fn shared_key(&self) -> u32
        {
            self.0
        }
    }

    #[test]
    fn normal_and_shared_components_share_one_id_space()
    {
        let mut world = World::default();
        let e = world.create(Hp(10));
        // a query registers a shared type before any value of it exists
        assert_eq!(world.create_shared_query::<&Hp, &Mesh>().iter().count(), 0);
        world.create(Mana(20));
        assert_eq!(world.component_counter.index_of(&TypeId::of::<Hp>()), Some(0));
        assert_eq!(world.component_counter.index_of(&TypeId::of::<Mesh>()), Some(1));
        assert_eq!(world.component_counter.index_of(&TypeId::of::<Mana>()), Some(2));

        world.add_shared_component(e, Mesh(7)).unwrap();
        world.add_component(e, Mana(30));
        assert_eq!(world.component_counter.len(), 3);
        assert_eq!(
            world.component_counter.get(&TypeId::of::<Mesh>()).unwrap().descriptor.storage_location,
            StorageLocation::Archetype
        );

        let arch = world.archetypes.get(&world.entities[e.idx()].arch_id()).unwrap();
        assert_eq!(arch.layout.component_bit_set.iter().collect::<Vec<_>>(), [0, 2]);
        assert_eq!(arch.shared.component_bit_set().iter().collect::<Vec<_>>(), [1]);
        assert_eq!(
            world
                .create_shared_query::<(&Hp, &Mana), &Mesh>()
                .iter()
                .map(|(hp, mana)| (hp.0, mana.0))
                .collect::<Vec<_>>(),
            [(10, 30)]
        );
    }

    #[crate::component]
    #[derive(Clone)]
    struct Ambiguous(u32);
    impl TSharedComponent for Ambiguous
    {
        type Key = u32;
        fn shared_key(&self) -> u32
        {
            self.0
        }
    }

    #[test]
    fn a_type_cannot_be_both_normal_and_shared()
    {
        let mut normal_first = World::default();
        normal_first.create(Ambiguous(1));
        let shared_after = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            normal_first.create_shared_query::<(), &Ambiguous>();
        }));
        assert!(shared_after.is_err());

        let mut shared_first = World::default();
        let e = shared_first.create(());
        shared_first.add_shared_component(e, Ambiguous(1)).unwrap();
        let normal_after = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            shared_first.create_query::<&Ambiguous>();
        }));
        assert!(normal_after.is_err());
    }

    #[test]
    fn keys_are_interned_once_per_type()
    {
        let mut world = World::default();
        assert_eq!(world.interned_value_of::<Mesh>(&7), None);
        let mesh_7 = world.shared_value_of::<Mesh>(&7);
        assert_eq!(world.shared_value_of::<Mesh>(&7), mesh_7);
        assert_eq!(world.interned_value_of::<Mesh>(&7), Some(mesh_7));
        let mesh_8 = world.shared_value_of::<Mesh>(&8);
        assert_eq!(mesh_8.component_id, mesh_7.component_id);
        assert_ne!(mesh_8.key_id, mesh_7.key_id);

        let material_7 = world.shared_value_of::<Material>(&7);
        assert_eq!(material_7.key_id, 0, "every shared type counts its keys on its own");
        assert_ne!(material_7.component_id, mesh_7.component_id);
    }

    #[test]
    fn looking_up_an_unknown_key_does_not_intern_it()
    {
        let mut world = World::default();
        let e = world.create(Hp(1));
        world.add_shared_component(e, Mesh(7)).unwrap();
        let f = world.create(Hp(2));
        assert_eq!(world.add_shared_component_by_key::<Mesh>(f, 8), Err(XynokEcsError::UnresolvedSharedKey));
        assert_eq!(world.set_shared_component_by_key::<Mesh>(e, 9), Err(XynokEcsError::UnresolvedSharedKey));
        assert_eq!(world.interned_value_of::<Mesh>(&8), None);
        assert_eq!(world.interned_value_of::<Mesh>(&9), None);
    }

    #[test]
    fn emptied_shared_archetypes_are_unregistered_and_reused()
    {
        let mut world = World::default();
        let e = world.create(Hp(1));
        let f = world.create(Hp(2));
        world.add_shared_component(e, Mesh(7)).unwrap();
        world.add_shared_component_by_key::<Mesh>(f, 7).unwrap();

        world.remove_shared_component::<Mesh>(e).unwrap();
        world.add_shared_component(e, Mesh(8)).unwrap();
        assert_eq!(world.archetypes.len(), 3, "plain Hp, Hp + Mesh(7), Hp + Mesh(8)");
        assert_eq!(world.shared_archetype_counter.len(), 2);

        world.set_shared_component_by_key::<Mesh>(e, 7).unwrap();
        assert_eq!(world.shared_archetype_counter.len(), 1, "Mesh(8) is empty, so it no longer resolves");
        assert!(world.set_shared_component_by_key::<Mesh>(e, 8).is_err());

        world.remove_shared_component::<Mesh>(e).unwrap();
        world.add_shared_component(e, Mesh(9)).unwrap();
        assert_eq!(world.archetypes.len(), 3, "Mesh(9) takes over the slot Mesh(8) left");
        assert_eq!(world.shared_archetype_counter.len(), 2);
    }
}
