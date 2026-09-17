use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::Ordering;

use xynok_std::collection::Queue;

use crate::apis::ArchetypeCfg;
use crate::apis::constants::{AtomicChangedTick, ChangedTick, DEFAULT_CHUNK_SIZE_IN_BYTE};
use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TQueryParam;
use crate::apis::params::{
    ArchetypeTakeAndRemoveComponentParams, ArchetypeTakeAndWriteComponentParams, ComponentSpec, ComponentSpecs, EntityInChunkIndices, EntityIndices, SwappedRow,
};
use crate::apis::safe_counter::SafeCounter;
use crate::apis::traits::TArchetype;
use crate::chunk::layout::{ChunkLayout, ChunkLayoutParams};
use crate::chunk::migration::MigrationPlan;
use crate::entity::Entity;
use crate::query::Query;
use crate::schedule::executor::TJobExecutor;
use crate::utils::normalize_set;
use crate::archetype::Archetype;
use crate::world::arch_spec::{ArchetypeEdge, ArchetypeKind, ArchetypeEdgeKey, ArchetypeSpec, ArchetypeSpecs, PairArchetypeSpecParams};
use crate::world::entity_spec::EntitySpec;
use crate::world::query_spec::{QuerySpec, QuerySpecAccessor, QuerySpecs};
use crate::world::temp_allocation::WorldTempAllocation;
mod temp_allocation;

/// What an archetype gets when nobody registered it: spawned through `create`, or built by
/// `add_component`/`remove_component`.
const DEFAULT_ARCHETYPE_CFG: ArchetypeCfg = ArchetypeCfg {
    chunk_size_in_byte:     DEFAULT_CHUNK_SIZE_IN_BYTE,
    allow_structure_change: true,
};

pub(crate) mod entity_spec;
pub(crate) mod arch_spec;
pub(crate) mod query_spec;

/// Read-only introspection into `World`'s private storage state, for the integration tests
/// under `tests/` (which only ever see the crate's normal public API otherwise)
#[cfg(feature = "test-util")]
pub mod testing;

pub struct World
{
    // A `QuerySpecAccessor` borrows these registries directly, and the entries inside are
    // never boxed: nothing caches their addresses, only their indices.
    archetypes:               ArchetypeSpecs,
    component_counter:        ComponentSpecs,
    query_counter:            QuerySpecs,
    component_set_counter:    HashMap<Vec<usize>, usize>,
    archetype_counter:        HashMap<TypeId, usize>,
    // Edge cache for structural changes, see `ArchetypeEdge`. `add_edges` serves both
    // `add_component` and `merge_component`, since both land on the union of the two archetypes.
    add_edges:                HashMap<ArchetypeEdgeKey, ArchetypeEdge>,
    remove_edges:             HashMap<ArchetypeEdgeKey, ArchetypeEdge>,
    entities:                 Vec<EntitySpec>,
    free_entities:            Queue<usize>,
    // Slots that ran out of versions and will never be handed out again, see `erase_entity`.
    // Kept as a plain count because nothing can be done with them, it is only here so the
    // leak is observable rather than silent.
    retired_entity_slots:     usize,
    temp_alloc:               WorldTempAllocation,
    global_archetype_version: SafeCounter,
    // atomic because a parallel system group calls `advance_tick` from multiple threads at once
    // (see `schedule::scheduler::run_system_group`); only uniqueness/monotonicity is required
    // between systems, not any particular ordering, so `Relaxed` is enough
    tick:                     AtomicChangedTick,
    // the tick as of the previous `create_query` call, i.e. the baseline that call's successor
    // measures `Added`/`Changed` against. `0` means "nobody has queried yet".
    last_query_tick:          ChangedTick,
    // where parallel work goes, see `TJobExecutor`. `None` means everything runs on the caller
    executor:                 Option<Box<dyn TJobExecutor>>,
}
impl Default for World
{
    fn default() -> Self
    {
        Self {
            entities:                 Vec::with_capacity(16),
            archetypes:               ArchetypeSpecs::new(),
            component_counter:        ComponentSpecs::new(),
            archetype_counter:        HashMap::new(),
            component_set_counter:    HashMap::new(),
            add_edges:                HashMap::new(),
            remove_edges:             HashMap::new(),
            query_counter:            QuerySpecs::new(),
            free_entities:            Queue::new(),
            retired_entity_slots:     0,
            temp_alloc:               WorldTempAllocation::new(),
            global_archetype_version: SafeCounter::new(1, usize::MAX - 1),
            // starts at 1, not 0: `0` is the sentinel both change-detection storage (never
            // touched) and a system's `last_run_tick` (never run) default to. If the world's
            // first-ever write also stamped tick `0`, `is_newer_than(0, last_run=0, this_run=1)`
            // would compare it as "not newer" (equal ages), so anything spawned before the very
            // first system run would be invisible to that system's first `Added`/`Changed` check
            tick:                     AtomicChangedTick::new(1),
            last_query_tick:          0,
            executor:                 None,
        }
    }
}
impl Drop for World
{
    fn drop(&mut self)
    {
        for arch in self.archetypes.values_mut()
        {
            arch.arch.dispose(&arch.layout);
        }
    }
}
impl World
{
    /// Hands the world a place to run parallel work, or takes it away with `None`. The previous
    /// executor is returned so you can keep it or let it drop.
    ///
    /// It takes `&mut self` on purpose: nothing can swap the executor while a system or a
    /// parallel query is still using it.
    pub fn set_executor(&mut self, executor: Option<Box<dyn TJobExecutor>>) -> Option<Box<dyn TJobExecutor>>
    {
        std::mem::replace(&mut self.executor, executor)
    }

    /// The executor parallel work goes to, if the world has one
    #[inline]
    pub fn executor(&self) -> Option<&dyn TJobExecutor>
    {
        self.executor.as_deref()
    }

    /// Creates the archetype for `T` with the given chunk size, before any entity lands in it.
    ///
    /// Registering the same component set again with the same size does nothing. A different size
    /// panics, see [`try_register_archetype`](Self::try_register_archetype).
    #[track_caller]
    pub fn register_archetype<T: TArchetype + 'static>(&mut self, cfg: ArchetypeCfg)
    {
        if let Err(e) = self.try_register_archetype::<T>(cfg)
        {
            panic!("Register Archetype `{}` Failed: {e}", std::any::type_name::<T>());
        }
    }

    /// Like [`register_archetype`](Self::register_archetype), but reports a bad `cfg` instead of
    /// panicking.
    ///
    /// An archetype that already exists (registered before, spawned through `create`, or built by
    /// `add_component`) keeps its layout. Its chunk size can't be changed afterwards: cached
    /// migration plans point at the old column offsets. So asking for a different size is an error.
    #[track_caller]
    pub fn try_register_archetype<T: TArchetype + 'static>(&mut self, cfg: ArchetypeCfg) -> Result<(), XynokEcsError>
    {
        cfg.validate()?;

        let arch_id = match self.get_archetype_id::<T>()
        {
            Some(r) => r,
            None => self.create_archetype_id::<T>(ArchetypeKind::Chunked(cfg)),
        };

        let arch_spec = self.archetypes.get(&arch_id).unwrap();
        if arch_spec.arch.is_singleton()
        {
            return Err(XynokEcsError::ArchetypeIsSingleton(arch_spec.name.clone()));
        }
        let current = arch_spec.layout.chunk_size_in_byte();
        if current != cfg.chunk_size_in_byte
        {
            return Err(XynokEcsError::ArchetypeAlreadyCreatedWithDifferentChunkSize(
                std::any::type_name::<T>(),
                current,
                cfg.chunk_size_in_byte,
            ));
        }
        if arch_spec.allow_structure_change != cfg.allow_structure_change
        {
            return Err(XynokEcsError::ArchetypeAlreadyCreatedWithDifferentStructureChange(
                std::any::type_name::<T>(),
                arch_spec.allow_structure_change,
                cfg.allow_structure_change,
            ));
        }
        Ok(())
    }
    /// Creates the singleton archetype for `T`: it holds one entity at most, its only chunk is
    /// sized to fit that one row instead of using a fixed chunk size, and structure changes are
    /// always locked (see [`ArchetypeCfg::allow_structure_change`]).
    ///
    /// Registering the same singleton again does nothing. Panics when `T` already exists as a
    /// regular archetype, see [`try_register_singleton`](Self::try_register_singleton).
    #[track_caller]
    pub fn register_singleton<T: TArchetype + 'static>(&mut self)
    {
        if let Err(e) = self.try_register_singleton::<T>()
        {
            panic!("Register Singleton `{}` Failed: {e}", std::any::type_name::<T>());
        }
    }

    /// Like [`register_singleton`](Self::register_singleton), but returns an error instead of
    /// panicking.
    ///
    /// An archetype can't switch kind once it exists, so this fails with
    /// [`XynokEcsError::ArchetypeIsNotSingleton`] when `T` (or another tuple with the same
    /// components) was already created as a regular archetype.
    #[track_caller]
    pub fn try_register_singleton<T: TArchetype + 'static>(&mut self) -> Result<(), XynokEcsError>
    {
        self.get_or_create_singleton_archetype_id::<T>().map(|_| ())
    }

    #[track_caller]
    fn get_or_create_singleton_archetype_id<T: TArchetype + 'static>(&mut self) -> Result<usize, XynokEcsError>
    {
        let arch_id = match self.get_archetype_id::<T>()
        {
            Some(r) => r,
            None => self.create_archetype_id::<T>(ArchetypeKind::Singleton),
        };
        let arch_spec = self.archetypes.get(&arch_id).unwrap();
        if !arch_spec.arch.is_singleton()
        {
            return Err(XynokEcsError::ArchetypeIsNotSingleton(arch_spec.name.clone()));
        }
        Ok(arch_id)
    }

    /// Spawns the one entity of the singleton archetype `T`, registering it first if needed.
    ///
    /// Panics when the singleton already has its entity, or when `T` exists as a regular
    /// archetype. Use [`try_create_singleton`](Self::try_create_singleton) to handle those yourself.
    /// Once the entity is destroyed, a new one can be spawned again.
    #[track_caller]
    pub fn create_singleton<T: TArchetype + 'static>(&mut self, val: T) -> Entity
    {
        match self.try_create_singleton(val)
        {
            Ok(r) => r,
            Err(e) => panic!("Create Singleton `{}` Failed: {e}", std::any::type_name::<T>()),
        }
    }

    /// The non panicking version of [`create_singleton`](Self::create_singleton).
    #[track_caller]
    pub fn try_create_singleton<T: TArchetype + 'static>(&mut self, val: T) -> Result<Entity, XynokEcsError>
    {
        let arch_id = self.get_or_create_singleton_archetype_id::<T>()?;
        // checked before `new_entity`, otherwise a failed push would leave an entity slot behind
        let arch_spec = self.archetypes.get(&arch_id).unwrap();
        if !arch_spec.arch.can_take_a_row()
        {
            return Err(XynokEcsError::SingletonAlreadyExists(arch_spec.name.clone()));
        }

        let new_e = self.new_entity()?;
        let tick = self.current_tick();
        let arch_spec = self.archetypes.get_mut(&arch_id).unwrap();
        let entity_chunk_indices = arch_spec.arch.push(&arch_spec.layout, new_e, val, tick)?;

        self.update_entity_spec(new_e, arch_id, entity_chunk_indices);
        Ok(new_e)
    }

    #[track_caller]
    pub fn create<T: TArchetype + 'static>(&mut self, val: T) -> Entity
    {
        let arch_id = self.get_or_create_archetype_id::<T>();
        // `create` onto a singleton layout that is already taken, e.g. `create_singleton(Hp)` then `create(Hp)`
        let arch_spec = self.archetypes.get(&arch_id).unwrap();
        if !arch_spec.arch.can_take_a_row()
        {
            panic!("{}", XynokEcsError::SingletonAlreadyExists(arch_spec.name.clone()));
        }

        let new_e = match self.new_entity()
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        let tick = self.current_tick();
        let arch_spec = self.archetypes.get_mut(&arch_id).unwrap();

        let entity_chunk_indices = match arch_spec.arch.push(&arch_spec.layout, new_e, val, tick)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };

        self.update_entity_spec(new_e, arch_id, entity_chunk_indices);
        new_e
    }

    pub fn exists(&self, e: Entity) -> bool
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
    /// Destroys `e` and drops every component it holds.
    ///
    /// Panics (in every build, release included) when `e` does not exist, for example when the
    /// handle is stale because the entity was already destroyed. Use [`World::try_destroy`] when
    /// a missing entity is a normal case you want to handle yourself.
    #[track_caller]
    pub fn destroy(&mut self, e: Entity)
    {
        if let Err(err) = self.try_destroy(e)
        {
            panic!("{}", err)
        }
    }

    /// The non panicking version of [`World::destroy`]: returns
    /// [`XynokEcsError::EntityDoesNotExist`] instead of panicking on a stale handle.
    pub fn try_destroy(&mut self, e: Entity) -> Result<(), XynokEcsError>
    {
        if !self.exists(e)
        {
            return Err(XynokEcsError::EntityDoesNotExist(e.idx(), e.version()));
        }

        let (arch_id, chunk_idx, idx_in_chunk) = unsafe {
            let spec = self.entities.get_unchecked(e.idx());
            (spec.arch_id(), spec.chunk_idx(), spec.idx_in_chunk())
        };
        let arch = self.archetypes.get_mut(&arch_id).unwrap();
        if let Some(swapped_row) = arch.arch.remove_at(&arch.layout, chunk_idx, idx_in_chunk)?
        {
            self.update_entity_indices(swapped_row);
        }

        self.erase_entity(e);
        Ok(())
    }

    /// Adds the components of `T` to `e`. `T` must not share any component with `e`'s current
    /// archetype, use [`World::merge_component`] for that.
    ///
    /// Panics (in every build, release included) when `e` does not exist. Use
    /// [`World::try_add_component`] when a missing entity is a normal case you want to handle
    /// yourself.
    #[track_caller]
    pub fn add_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T)
    {
        if let Err(err) = self.try_add_component(e, val)
        {
            panic!("{}", err)
        }
    }

    /// The non panicking version of [`World::add_component`]: returns
    /// [`XynokEcsError::EntityDoesNotExist`] instead of panicking on a stale handle.
    #[track_caller]
    pub fn try_add_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T) -> Result<(), XynokEcsError>
    {
        if !self.exists(e)
        {
            return Err(XynokEcsError::EntityDoesNotExist(e.idx(), e.version()));
        }

        let (a_arch_id, a_chunk_idx, a_idx_in_chunk) = unsafe {
            let e_spec = self.entities.get_unchecked(e.idx());
            (e_spec.arch_id(), e_spec.chunk_idx(), e_spec.idx_in_chunk())
        };
        // `b_arch_id` is only needed in a debug build now: it is here purely to compare the two
        // archetypes and report a misuse of the API. The main path goes straight to the edge cache.
        #[cfg(debug_assertions)]
        {
            let b_arch_id = self.get_or_create_archetype_id::<T>();
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

        let edge_key = ArchetypeEdgeKey {
            src_arch_id:       a_arch_id,
            archetype_type_id: TypeId::of::<T>(),
        };
        let target_arch_id = self.get_or_create_add_edge::<T>(edge_key);
        self.check_structure_change::<T>(e, a_arch_id, target_arch_id)?;

        let tick = self.current_tick();
        let src_idx = self.archetypes.index_of(&a_arch_id).unwrap();
        let target_idx = self.archetypes.index_of(&target_arch_id).unwrap();
        let migration = &self.add_edges.get(&edge_key).unwrap().migration;
        let [src_arch_spec, target_arch_spec] = self.archetypes.values_mut_slice().get_disjoint_mut([src_idx, target_idx]).unwrap();

        let src_e_indices = EntityIndices {
            chunk_idx:    a_chunk_idx,
            idx_in_chunk: a_idx_in_chunk,
        };
        let params = ArchetypeTakeAndWriteComponentParams {
            src_e:      src_e_indices,
            src_arch:   &mut src_arch_spec.arch,
            src_layout: &src_arch_spec.layout,
            dst_layout: &target_arch_spec.layout,
            migration:  migration,
            write_val:  val,
            tick:       tick,
        };
        let take_and_write_result = target_arch_spec.arch.take_and_write_from(params)?;
        if let Some(swapped_row) = take_and_write_result.swapped_e
        {
            self.update_entity_indices(swapped_row);
        }
        self.update_entity_spec(e, target_arch_id, take_and_write_result.new_indices_took);
        Ok(())
    }

    /// Adds the components of `T` to `e` if they are not already present, otherwise overwrites the
    /// existing values. Unlike `add_component`, this allows `T` to share components with `e`'s current
    /// archetype.
    ///
    /// Panics (in every build, release included) when `e` does not exist. Use
    /// [`World::try_merge_component`] when a missing entity is a normal case you want to handle
    /// yourself.
    #[track_caller]
    pub fn merge_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T)
    {
        if let Err(err) = self.try_merge_component(e, val)
        {
            panic!("{}", err)
        }
    }

    /// The non panicking version of [`World::merge_component`]: returns
    /// [`XynokEcsError::EntityDoesNotExist`] instead of panicking on a stale handle.
    #[track_caller]
    pub fn try_merge_component<T: TArchetype + 'static>(&mut self, e: Entity, val: T) -> Result<(), XynokEcsError>
    {
        if !self.exists(e)
        {
            return Err(XynokEcsError::EntityDoesNotExist(e.idx(), e.version()));
        }

        let (a_arch_id, a_chunk_idx, a_idx_in_chunk) = unsafe {
            let e_spec = self.entities.get_unchecked(e.idx());
            (e_spec.arch_id(), e_spec.chunk_idx(), e_spec.idx_in_chunk())
        };
        let edge_key = ArchetypeEdgeKey {
            src_arch_id:       a_arch_id,
            archetype_type_id: TypeId::of::<T>(),
        };
        let target_arch_id = self.get_or_create_add_edge::<T>(edge_key);

        let tick = self.current_tick();

        // every component of `T` is already part of `e`'s archetype: overwrite the row in place, no move needed
        if target_arch_id == a_arch_id
        {
            let arch_spec = self.archetypes.get_mut(&a_arch_id).unwrap();
            arch_spec.arch.replace_at(&arch_spec.layout, a_chunk_idx, a_idx_in_chunk, val, tick)?;
            return Ok(());
        }
        self.check_structure_change::<T>(e, a_arch_id, target_arch_id)?;

        let src_idx = self.archetypes.index_of(&a_arch_id).unwrap();
        let target_idx = self.archetypes.index_of(&target_arch_id).unwrap();
        let migration = &self.add_edges.get(&edge_key).unwrap().migration;
        let [src_arch_spec, target_arch_spec] = self.archetypes.values_mut_slice().get_disjoint_mut([src_idx, target_idx]).unwrap();

        let src_e_indices = EntityIndices {
            chunk_idx:    a_chunk_idx,
            idx_in_chunk: a_idx_in_chunk,
        };
        let params = ArchetypeTakeAndWriteComponentParams {
            src_e:      src_e_indices,
            src_arch:   &mut src_arch_spec.arch,
            src_layout: &src_arch_spec.layout,
            dst_layout: &target_arch_spec.layout,
            migration:  migration,
            write_val:  val,
            tick:       tick,
        };
        let take_and_write_result = target_arch_spec.arch.take_and_write_from(params)?;
        if let Some(swapped_row) = take_and_write_result.swapped_e
        {
            self.update_entity_indices(swapped_row);
        }
        self.update_entity_spec(e, target_arch_id, take_and_write_result.new_indices_took);
        Ok(())
    }

    /// Removes the components of `T` from `e` and hands them back.
    ///
    /// Panics (in every build, release included) when `e` does not exist. Use
    /// [`World::try_remove_component`] when a missing entity is a normal case you want to handle
    /// yourself.
    #[track_caller]
    pub fn remove_component<T: TArchetype + 'static>(&mut self, e: Entity) -> T
    {
        match self.try_remove_component::<T>(e)
        {
            Ok(r) => r,
            Err(err) => panic!("{}", err),
        }
    }

    /// The non panicking version of [`World::remove_component`]: returns
    /// [`XynokEcsError::EntityDoesNotExist`] instead of panicking on a stale handle.
    #[track_caller]
    pub fn try_remove_component<T: TArchetype + 'static>(&mut self, e: Entity) -> Result<T, XynokEcsError>
    {
        if !self.exists(e)
        {
            return Err(XynokEcsError::EntityDoesNotExist(e.idx(), e.version()));
        }
        let (a_arch_id, a_chunk_idx, a_idx_in_chunk) = unsafe {
            let e_spec = self.entities.get_unchecked(e.idx());
            (e_spec.arch_id(), e_spec.chunk_idx(), e_spec.idx_in_chunk())
        };
        // like `add_component`: only a debug build needs `b_arch_id`, to check the input
        #[cfg(debug_assertions)]
        {
            let b_arch_id = self.get_or_create_archetype_id::<T>();
            let contains_all_components = {
                let a = self.archetypes.get(&a_arch_id).unwrap();
                let b = self.archetypes.get(&b_arch_id).unwrap();
                a.contains_all_components_of(b)
            };
            if !contains_all_components
            {
                panic!(
                    "Cannot remove component `{}` from entity {}: the entity does not have this component.",
                    std::any::type_name::<T>(),
                    e
                )
            }
        }

        let edge_key = ArchetypeEdgeKey {
            src_arch_id:       a_arch_id,
            archetype_type_id: TypeId::of::<T>(),
        };
        let target_arch_id = self.get_or_create_remove_edge::<T>(edge_key);
        self.check_structure_change::<T>(e, a_arch_id, target_arch_id)?;

        let src_idx = self.archetypes.index_of(&a_arch_id).unwrap();
        let target_idx = self.archetypes.index_of(&target_arch_id).unwrap();
        let migration = &self.remove_edges.get(&edge_key).unwrap().migration;
        let [src_arch_spec, target_arch_spec] = self.archetypes.values_mut_slice().get_disjoint_mut([src_idx, target_idx]).unwrap();

        let src_e_indices = EntityIndices {
            chunk_idx:    a_chunk_idx,
            idx_in_chunk: a_idx_in_chunk,
        };
        let params = ArchetypeTakeAndRemoveComponentParams::<T> {
            src_e:      src_e_indices,
            src_arch:   &mut src_arch_spec.arch,
            src_layout: &src_arch_spec.layout,
            dst_layout: &target_arch_spec.layout,
            migration:  migration,
            phantom:    PhantomData,
        };
        let result = target_arch_spec.arch.take_and_remove_from(params)?;
        if let Some(swapped_row) = result.swapped_e
        {
            self.update_entity_indices(swapped_row);
        }
        self.update_entity_spec(e, target_arch_id, result.new_indices_took);

        Ok(result.val)
    }

    /// Builds a query over the current state of the world.
    ///
    /// The `Added` / `Changed` filters report what happened since the previous `create_query`
    /// call on this world. The first call reports everything written so far, since nothing has
    /// looked yet. The other filters (`Without`, `Enabled`, `Disabled`) read the current state
    /// and have no notion of time.
    ///
    /// The query borrows the world mutably for as long as it lives, so treat it as a short lived
    /// value: create it, iterate it, let it go. Keeping one around while the world changes does
    /// not compile, see [`Query`].
    #[track_caller]
    pub fn create_query<'a, T: TQueryParam + 'static>(&'a mut self) -> Query<'a, T>
    {
        // The baseline is the tick as it stood at the *previous* `create_query`, not at the
        // previous advance: writes made between the two calls were stamped with the tick this
        // method left behind last time, so using that tick itself as the baseline would filter
        // them right back out (`is_newer_than` treats equal ticks as "not newer").
        let last_query_tick = self.last_query_tick;
        self.last_query_tick = self.current_tick();

        // the round is closed by `create_query_since`, which every query goes through
        self.create_query_since(last_query_tick)
    }

    /// Same as [`create_query`](Self::create_query), but with an explicit baseline for the
    /// `Added` / `Changed` filters: only rows stamped *after* `last_run_tick` are yielded.
    /// Use it when you want to hold a window of your own instead of the one `create_query`
    /// keeps for you, for example when driving a world by hand or writing your own scheduler.
    ///
    /// Like `create_query`, this closes the current round of change detection on the way out,
    /// so it moves the world's tick even though the name only talks about reading.
    ///
    /// Take the baseline with [`capture_current_tick`](Self::capture_current_tick), which hands
    /// back exactly what this parameter wants.
    #[track_caller]
    pub fn create_query_since<'a, T: TQueryParam + 'static>(&'a mut self, last_run_tick: ChangedTick) -> Query<'a, T>
    {
        // Every query closes the current round of change detection, exactly like `create_query`:
        // whatever gets written from here on is stamped with a tick of its own, so the next query
        // can tell it apart from what this one is about to report.
        self.advance_tick();

        match Query::new(self, last_run_tick)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        }
    }

    /// How many entity slots have used up all of their versions and been taken out of
    /// circulation for good (see `erase_entity`).
    ///
    /// Each one costs an `EntitySpec` that will never be reused, and reaching even one means a
    /// single slot has been recycled 2^24 times. A number that keeps climbing is worth looking
    /// at: it usually means a handful of slots are being churned in a tight create/destroy
    /// loop, and pooling the entities would serve better than respawning them.
    #[inline]
    pub fn retired_entity_slot_count(&self) -> usize
    {
        self.retired_entity_slots
    }

    /// The tick as of the last call to [`advance_tick`](Self::advance_tick), i.e. the one every
    /// write from now until the next `advance_tick` will be stamped with. Change-detection
    /// storage (`added`/`changed`) starts zeroed, so `0` permanently means "never touched" and
    /// is never handed out by `advance_tick`.
    ///
    /// This is *not* the baseline to hand to [`create_query_since`](Self::create_query_since):
    /// the writes you are about to make carry this very tick, which counts as "not newer" and
    /// filters them right back out. Use [`capture_current_tick`](Self::capture_current_tick) for that.
    #[inline]
    pub fn current_tick(&self) -> ChangedTick
    {
        self.tick.load(Ordering::Relaxed)
    }

    /// Captures the current tick as a "from here on" baseline for
    /// [`create_query_since`](Self::create_query_since).
    ///
    /// This is the way to take a baseline. It returns the value the `Added` / `Changed` filters
    /// actually want, and closes the current round on the way out, so everything written after
    /// this call carries a tick of its own and gets reported. Pairing
    /// [`current_tick`](Self::current_tick) with [`advance_tick`](Self::advance_tick) by hand
    /// gives the same result, it is just easy to forget the second half.
    #[inline]
    pub fn capture_current_tick(&mut self) -> ChangedTick
    {
        let captured = self.current_tick();
        self.advance_tick();
        captured
    }

    /// Moves the world's tick forward and returns the new value. Called once per system run
    /// (see `schedule::scheduler::run_system`), the same granularity Bevy advances its
    /// `change_tick` at: every write within one system run is stamped with that one tick, and
    /// two different systems always see two different ticks even if they run back to back - even
    /// when they run concurrently in the same parallel group, since this is an atomic increment
    ///
    /// Driving a world by hand rather than through a schedule, this is how you close one round
    /// of change detection and open the next. Without it every ad-hoc write shares one tick and
    /// `Changed` can only ever answer "was this ever written", not "was it written since".
    #[inline]
    pub fn advance_tick(&self) -> ChangedTick
    {
        self.tick.fetch_add(1, Ordering::Relaxed) + 1
    }
}

impl World
{
    pub(crate) fn component_specs_mut(&mut self) -> &mut ComponentSpecs
    {
        &mut self.component_counter
    }

    /// Whether the `QuerySpec` registered for `T` writes anything, i.e. what the scheduler would
    /// see when it decides who may run beside this query. Only needed by the unit test that
    /// guards `TQueryParam::Shape` against two query shapes sharing one spec.
    #[cfg(test)]
    pub(crate) fn registered_query_is_read_only<T: TQueryParam + 'static>(&self) -> Option<bool>
    {
        let idx = self.query_counter.index_of(&T::TYPE_ID)?;
        self.query_counter.value_at(idx).map(|spec| spec.access_scope.is_read_only())
    }

    /// Registers the `QuerySpec` for `T` and refreshes its archetype list if the world has
    /// changed shape since.
    ///
    /// This is the only part that needs `&mut World`, so it runs only where a single thread is
    /// guaranteed: the scheduler's `prepare` pass, or the first touch on a single-threaded world.
    /// Once it has run, building an accessor needs nothing but `&World`.
    pub(crate) fn prepare_query_src_access<T: TQueryParam + 'static>(&mut self) -> Result<usize, XynokEcsError>
    {
        let current_global_arch_version = self.global_archetype_version.current_val();

        let query_idx = match self.query_counter.index_of(&T::TYPE_ID)
        {
            Some(idx) => idx,
            None =>
            {
                // Registers any component the world has not seen yet, so it has to run
                // before anything borrows the component registry again
                let access_scope = T::access_scope(&mut self.component_counter)?;

                let mut target_archetypes = Vec::new();
                crate::utils::build_archetype_which_contains(&self.archetypes, &mut target_archetypes, &access_scope);
                self.query_counter.insert(
                    T::TYPE_ID,
                    QuerySpec {
                        access_scope: access_scope,
                        archetypes:   target_archetypes,
                        version:      current_global_arch_version,
                    },
                )
            }
        };

        let query_spec = match self.query_counter.value_mut_at(query_idx)
        {
            Some(r) => r,
            None => panic!("query index {query_idx} vanished from the registry it was just taken from"),
        };
        if query_spec.version != current_global_arch_version
        {
            query_spec.archetypes.clear();
            crate::utils::build_archetype_which_contains(&self.archetypes, &mut query_spec.archetypes, &query_spec.access_scope);
            query_spec.version = current_global_arch_version;
        }

        Ok(query_idx)
    }

    /// The read-only fast path. Returns `None` when `T` has not been registered yet, or when its
    /// spec is stale against `global_archetype_version`, which means the caller has to go back
    /// through [`World::prepare_query_src_access`].
    ///
    /// Taking `&self` is the point: every job in a parallel group may call this at the same time,
    /// instead of each one conjuring its own `&mut World` and aliasing the others.
    pub(crate) fn query_src_access<'a, T: TQueryParam + 'static>(&'a self, last_run_tick: ChangedTick) -> Option<QuerySpecAccessor<'a>>
    {
        let query_idx = self.query_counter.index_of(&T::TYPE_ID)?;
        if self.query_counter.value_at(query_idx)?.version != self.global_archetype_version.current_val()
        {
            return None;
        }

        Some(QuerySpecAccessor {
            query_idx:     query_idx,
            queries:       &self.query_counter,
            archetypes:    &self.archetypes,
            //component_specs: &self.component_counter,
            last_run_tick: last_run_tick,
            this_run_tick: self.current_tick(),
        })
    }

    pub(crate) fn get_or_create_query_src_access<'a, T: TQueryParam + 'static>(
        &'a mut self,
        last_run_tick: ChangedTick,
    ) -> Result<QuerySpecAccessor<'a>, XynokEcsError>
    {
        self.prepare_query_src_access::<T>()?;

        // every mutation is done, so the exclusive borrow can turn into the shared one the
        // accessor keeps for `'a`
        let this: &'a World = self;
        match this.query_src_access::<T>(last_run_tick)
        {
            Some(r) => Ok(r),
            None => panic!("query spec for {} vanished right after it was prepared", std::any::type_name::<T>()),
        }
    }
}
impl World
{
    fn update_entity_spec(&mut self, e: Entity, arch_id: usize, indices: EntityInChunkIndices)
    {
        let entity_spec = unsafe { self.entities.get_unchecked_mut(e.idx()) };
        *entity_spec = EntitySpec::new(arch_id, indices.chunk_idx, indices.idx_in_chunk, e.version());
    }
    /// Marks the slot empty, and decides whether it may ever be handed out again.
    ///
    /// A handle is `(idx, version)`, and the version is the only thing telling this incarnation
    /// of the slot apart from the next one. `new_entity` hands out `version + 1`, so a slot
    /// sitting at [`Entity::MAX_VERSION`] has nothing left to distinguish itself with: reusing
    /// it would reissue a handle identical to the one just destroyed, and every stale copy of
    /// that handle would start passing `exists` again and address the new entity.
    ///
    /// So the slot is retired instead: left in `entities` but never enqueued, and `create`
    /// takes a fresh slot at the end of the vector. The cost is one `EntitySpec` leaked per
    /// retired slot, after 2^24 reuses *of that one slot*. That is the trade: a bounded leak
    /// nobody can observe going wrong, rather than an unbounded correctness hole.
    fn erase_entity(&mut self, e: Entity)
    {
        let version = unsafe {
            let e_spec = self.entities.get_unchecked_mut(e.idx());
            e_spec.errase();
            e_spec.version()
        };

        match version < Entity::MAX_VERSION
        {
            true => self.free_entities.enqueue(e.idx()),
            false => self.retired_entity_slots += 1,
        }
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
            match self.component_counter.index_of(type_id)
            {
                Some(component_id) => component_set.retain(|e| *e != component_id),
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
            match self.component_counter.index_of(type_id)
            {
                Some(component_id) => component_set.push(component_id),
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
            // `erase_entity` only enqueues a slot with a version left to spend, so the `+ 1`
            // cannot run past `MAX_VERSION` here and `Entity::new` has nothing to refuse
            debug_assert!(
                old_slot.version() < Entity::MAX_VERSION,
                "slot {free_idx} was recycled at version {} but should have been retired",
                old_slot.version()
            );
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
    fn get_or_create_archetype_id<T: TArchetype + 'static>(&mut self) -> usize
    {
        match self.get_archetype_id::<T>()
        {
            Some(r) => r,
            None => self.create_archetype_id::<T>(ArchetypeKind::Chunked(DEFAULT_ARCHETYPE_CFG)),
        }
    }

    #[track_caller]
    fn create_archetype_id<T: TArchetype + 'static>(&mut self, kind: ArchetypeKind) -> usize
    {
        // Runs once per `T`, the first time the world sees it: from here on `archetype_counter`
        // answers straight away, so the check costs nothing on the hot path.
        if let Err(e) = check_no_duplicate_component::<T>()
        {
            panic!("Create Archetype `{}` Failed: {e}", std::any::type_name::<T>());
        }

        let component_set = &mut self.temp_alloc.vec_usize;
        component_set.clear();

        // collect component id: a component's id is its index in the registry
        for des in T::COMPONENT_DESCRIPTORS
        {
            let id = self
                .component_counter
                .get_or_insert_with(des.storage_type_id, || ComponentSpec { descriptor: des.clone() });
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
                let component_set = std::mem::take(component_set);
                let arch_spec = self.create_archetype::<T>(kind);
                let arch_id = self.register_archetype_spec(&component_set, arch_spec);
                self.archetype_counter.insert(std::any::TypeId::of::<T>(), arch_id);
                // put back
                self.temp_alloc.vec_usize = component_set;
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
            state_offsets_temp:             &mut self.temp_alloc.state_offsets,
            temp_tys:                       &mut self.temp_alloc.hashset_type_ids,
            temp_comp_des:                  &mut self.temp_alloc.comp_descriptors,
            component_col_descriptors_temp: &mut self.temp_alloc.col_descriptors,
            columns_temp:                   &mut self.temp_alloc.col_entries,
            component_bit_set:              &mut self.temp_alloc.component_bit_set_a,
        };
        let new_arch = match ArchetypeSpec::new_from_a_exclude_b_components(merge_arch_params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        self.register_archetype_spec(component_set, new_arch)
    }
    fn create_archetype_id_from_set_of(&mut self, component_set: &[usize], a_arch_id: usize, b_arch_id: usize) -> usize
    {
        let merge_arch_params = PairArchetypeSpecParams {
            a:                              self.archetypes.get(&a_arch_id).unwrap(),
            b:                              self.archetypes.get(&b_arch_id).unwrap(),
            component_specs:                &self.component_counter,
            state_offsets_temp:             &mut self.temp_alloc.state_offsets,
            temp_tys:                       &mut self.temp_alloc.hashset_type_ids,
            temp_comp_des:                  &mut self.temp_alloc.comp_descriptors,
            component_col_descriptors_temp: &mut self.temp_alloc.col_descriptors,
            columns_temp:                   &mut self.temp_alloc.col_entries,
            component_bit_set:              &mut self.temp_alloc.component_bit_set_a,
        };
        let new_arch = match ArchetypeSpec::new_from_pair(merge_arch_params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        self.register_archetype_spec(component_set, new_arch)
    }
    #[track_caller]
    fn create_archetype<T: TArchetype + 'static>(&mut self, kind: ArchetypeKind) -> ArchetypeSpec
    {
        let chunk_size_in_byte = match kind
        {
            ArchetypeKind::Chunked(cfg) => cfg.chunk_size_in_byte,
            // ignored by `ChunkLayout::new_fitted`
            ArchetypeKind::Singleton => 0,
        };
        let params = ChunkLayoutParams {
            components:                 T::COMPONENT_DESCRIPTORS,
            component_specs:            &self.component_counter,
            state_offsets_temp:         &mut self.temp_alloc.state_offsets,
            component_descriptors_temp: &mut self.temp_alloc.col_descriptors,
            columns_temp:               &mut self.temp_alloc.col_entries,
            component_bit_set_temp:     &mut self.temp_alloc.component_bit_set_a,
            chunk_size_in_byte:         chunk_size_in_byte,
        };
        let layout = match kind
        {
            ArchetypeKind::Chunked(_) => ChunkLayout::new(params),
            ArchetypeKind::Singleton => ChunkLayout::new_fitted(params),
        };
        let layout = match layout
        {
            Ok(r) => r,
            Err(e) => panic!("Create Archetype `{}` Failed: {e}", std::any::type_name::<T>()),
        };
        match kind
        {
            ArchetypeKind::Chunked(cfg) => ArchetypeSpec::new(std::any::type_name::<T>().to_string(), Archetype::default(), layout, cfg.allow_structure_change),
            ArchetypeKind::Singleton => ArchetypeSpec::new(std::any::type_name::<T>().to_string(), Archetype::singleton(), layout, false),
        }
    }

    /// The one place an `arch_id` is handed out: it is always the index the spec lands on inside
    /// `archetypes`, so `build_archetype_which_contains` (which walks that registry by position)
    /// and every `arch_id` stored elsewhere can never drift apart. `component_set_counter` is
    /// only a lookup table on top of it, never a counter of its own.
    fn register_archetype_spec(&mut self, component_set: &[usize], arch_spec: ArchetypeSpec) -> usize
    {
        let arch_id = self.archetypes.len();
        self.archetypes.insert(arch_id, arch_spec);
        self.component_set_counter.insert(component_set.to_vec(), arch_id);
        self.structure_changed();
        arch_id
    }

    /// The archetype an entity lands in when `T` is added to `key.src_arch_id`, leaving the edge
    /// in the cache so the caller can pick up its `migration` right after.
    ///
    /// The first call still does all of the old work: collect both sides' component ids,
    /// `normalize_set`, hash a `Vec<usize>` to look up `component_set_counter`. Every later call
    /// with the same `(src, T)` costs one hash on `add_edges` and nothing else.
    #[track_caller]
    fn get_or_create_add_edge<T: TArchetype + 'static>(&mut self, key: ArchetypeEdgeKey) -> usize
    {
        if let Some(edge) = self.add_edges.get(&key)
        {
            return edge.dst_arch_id;
        }

        let b_arch_id = self.get_or_create_archetype_id::<T>();

        let mut component_set = std::mem::take(&mut self.temp_alloc.vec_usize);
        component_set.clear();
        self.append_archetype_component_id_of_to(key.src_arch_id, &mut component_set);
        self.append_archetype_component_id_of_to(b_arch_id, &mut component_set);
        normalize_set(&mut component_set);

        let dst_arch_id = match self.component_set_counter.get(&component_set)
        {
            Some(r) => *r,
            // create a new arch from these archetypes
            None => self.create_archetype_id_from_set_of(&component_set, key.src_arch_id, b_arch_id),
        };
        // put back
        self.temp_alloc.vec_usize = component_set;

        self.insert_edge(EdgeCacheInsert {
            is_remove:   false,
            key:         key,
            dst_arch_id: dst_arch_id,
        });
        dst_arch_id
    }

    /// The mirror of [`get_or_create_add_edge`](Self::get_or_create_add_edge) for
    /// `remove_component`: the target archetype is the difference rather than the union, and the
    /// caller reads `T`'s values out before the move.
    #[track_caller]
    fn get_or_create_remove_edge<T: TArchetype + 'static>(&mut self, key: ArchetypeEdgeKey) -> usize
    {
        if let Some(edge) = self.remove_edges.get(&key)
        {
            return edge.dst_arch_id;
        }

        let b_arch_id = self.get_or_create_archetype_id::<T>();

        let mut component_set = std::mem::take(&mut self.temp_alloc.vec_usize);
        component_set.clear();
        self.append_archetype_component_id_of_to(key.src_arch_id, &mut component_set);
        self.retain_archetype_component_id_of_to(b_arch_id, &mut component_set);
        normalize_set(&mut component_set);

        let dst_arch_id = match self.component_set_counter.get(&component_set)
        {
            Some(r) => *r,
            // create a new arch from these archetypes
            None => self.create_archetype_id_for_set_of_a_exclude_b(&component_set, key.src_arch_id, b_arch_id),
        };
        // put back
        self.temp_alloc.vec_usize = component_set;

        self.insert_edge(EdgeCacheInsert {
            is_remove:   true,
            key:         key,
            dst_arch_id: dst_arch_id,
        });
        dst_arch_id
    }

    fn insert_edge(&mut self, params: EdgeCacheInsert)
    {
        let migration = {
            let src_layout = &self.archetypes.get(&params.key.src_arch_id).unwrap().layout;
            let dst_layout = &self.archetypes.get(&params.dst_arch_id).unwrap().layout;
            MigrationPlan::new(src_layout, dst_layout)
        };
        let edge = ArchetypeEdge {
            dst_arch_id: params.dst_arch_id,
            migration:   migration,
        };
        match params.is_remove
        {
            true => self.remove_edges.insert(params.key, edge),
            false => self.add_edges.insert(params.key, edge),
        };
    }

    /// Runs before anything moves: the edge lookup right before it only fills caches.
    fn check_structure_change<T: 'static>(&self, e: Entity, src_arch_id: usize, dst_arch_id: usize) -> Result<(), XynokEcsError>
    {
        let src = self.archetypes.get(&src_arch_id).unwrap();
        let dst = self.archetypes.get(&dst_arch_id).unwrap();
        if !src.allow_structure_change || !dst.allow_structure_change
        {
            return Err(XynokEcsError::StructureChangeNotAllowed(e.idx(), e.version(), std::any::type_name::<T>()));
        }
        Ok(())
    }

    fn structure_changed(&mut self)
    {
        self.global_archetype_version.increase();
    }
}

/// Input to [`World::insert_edge`], shared by the add path and the remove path.
struct EdgeCacheInsert
{
    /// `true` puts the edge in `remove_edges`, `false` in `add_edges`.
    is_remove:   bool,
    key:         ArchetypeEdgeKey,
    dst_arch_id: usize,
}

/// Rejects an archetype that names the same component twice, e.g. `world.create((Hp(1), Hp(2)))`.
///
/// A chunk keeps exactly one column per component, so the second value would land on top of the
/// first without dropping it: a silent leak, and only one of the two survives. Catching it here
/// rather than in `ChunkLayout` matters because the duplicate is invisible by the time the layout
/// is built: `normalize_set` has already collapsed the id list, so `(Hp, Hp)` looks exactly like
/// `Hp` and may reuse that archetype without building a layout at all.
///
/// `T::COMPONENT_DESCRIPTORS` is a const slice of at most 16 entries, so the quadratic scan is
/// cheaper than reaching for a set, and it runs once per `T` for the life of the world.
fn check_no_duplicate_component<T: TArchetype>() -> Result<(), XynokEcsError>
{
    let descriptors = T::COMPONENT_DESCRIPTORS;
    for (i, des) in descriptors.iter().enumerate()
    {
        if descriptors[..i].iter().any(|earlier| earlier.storage_type_id == des.storage_type_id)
        {
            return Err(XynokEcsError::DuplicateComponentInArchetype(des.name()));
        }
    }
    Ok(())
}
