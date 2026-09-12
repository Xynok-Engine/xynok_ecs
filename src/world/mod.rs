use std::any::TypeId;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::Ordering;

use xynok_std::collection::Queue;

use crate::apis::constants::{AtomicChangedTick, ChangedTick};
use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::TQueryParam;
use crate::apis::params::{
    ArchetypeTakeAndRemoveComponentParams, ArchetypeTakeAndWriteComponentParams, ComponentSpec, ComponentSpecs, EntityInChunkIndices, EntityIndices, SwappedRow,
};
use crate::apis::safe_counter::SafeCounter;
use crate::apis::traits::TArchetype;
use crate::chunk::layout::{ChunkLayout, ChunkLayoutParams};
use crate::entity::Entity;
use crate::query::Query;
use crate::utils::normalize_set;
use crate::world::arch_spec::{ArchetypeSpec, ArchetypeSpecs, PairArchetypeSpecParams};
use crate::world::entity_spec::EntitySpec;
use crate::world::query_spec::{QuerySpec, QuerySpecAccessor, QuerySpecs};
use crate::world::temp_allocation::WorldTempAllocation;
mod temp_allocation;

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
        let tick = self.current_tick();
        let (arch_id, arch_spec) = self.get_or_create_archetype_spec_mut::<T>();

        let entity_chunk_indices = match arch_spec.arch.push(&arch_spec.layout, new_e, val, tick)
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

        let tick = self.current_tick();
        let src_idx = self.archetypes.index_of(&a_arch_id).unwrap();
        let target_idx = self.archetypes.index_of(&target_arch_id).unwrap();
        let [src_arch_spec, target_arch_spec] = self.archetypes.values_mut_slice().get_disjoint_mut([src_idx, target_idx]).unwrap();

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
            tick:            tick,
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

        let tick = self.current_tick();

        // every component of `T` is already part of `e`'s archetype: overwrite the row in place, no move needed
        if target_arch_id == a_arch_id
        {
            let arch_spec = self.archetypes.get_mut(&a_arch_id).unwrap();
            match arch_spec.arch.replace_at(&arch_spec.layout, a_chunk_idx, a_idx_in_chunk, val, tick)
            {
                Ok(_) =>
                {}
                Err(e) => panic!("{}", e),
            }
            return;
        }

        let src_idx = self.archetypes.index_of(&a_arch_id).unwrap();
        let target_idx = self.archetypes.index_of(&target_arch_id).unwrap();
        let [src_arch_spec, target_arch_spec] = self.archetypes.values_mut_slice().get_disjoint_mut([src_idx, target_idx]).unwrap();

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
            tick:            tick,
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

        let src_idx = self.archetypes.index_of(&a_arch_id).unwrap();
        let target_idx = self.archetypes.index_of(&target_arch_id).unwrap();
        let [src_arch_spec, target_arch_spec] = self.archetypes.values_mut_slice().get_disjoint_mut([src_idx, target_idx]).unwrap();

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
    ///
    /// Example usages:
    /// ```text
    /// let a = w.create(Hp(1));
    /// w.create(Hp(2));
    /// w.create_query::<Changed<&Hp>>()  // 2 rows
    /// w.create_query::<Changed<&Hp>>()  // empty
    /// w.merge_component(a, Hp(99));
    /// w.create_query::<Changed<&Hp>>()  // only 99
    /// ```
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
    ///
    /// ```
    /// use xynok_ecs::query::filter::Changed;
    /// use xynok_ecs::world::World;
    /// #[xynok_ecs::component(ChangeAble)]
    /// #[derive(Debug)]
    /// struct Hp(u32);
    ///
    /// let mut w = World::default();
    /// let a = w.create(Hp(1));
    /// w.create(Hp(2));
    ///
    /// let seen_up_to = w.capture_current_tick();
    /// w.merge_component(a, Hp(99));
    ///
    /// let changed: Vec<u32> = w
    ///     .create_query_since::<Changed<&Hp>>(seen_up_to)
    ///     .into_iter()
    ///     .map(|hp| hp.0)
    ///     .collect();
    /// assert_eq!(changed, vec![99]);
    /// ```
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
            query_idx:       query_idx,
            queries:         &self.query_counter,
            archetypes:      &self.archetypes,
            component_specs: &self.component_counter,
            last_run_tick:   last_run_tick,
            this_run_tick:   self.current_tick(),
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
            state_offsets_temp:             &mut self.temp_alloc.state_offsets,
            temp_tys:                       &mut self.temp_alloc.hashset_type_ids,
            temp_comp_des:                  &mut self.temp_alloc.comp_descriptors,
            component_col_descriptors_temp: &mut self.temp_alloc.col_descriptors,
            component_bit_set:              &mut self.temp_alloc.component_bit_set_a,
        };
        let new_arch = match ArchetypeSpec::new_from_a_exclude_b_components(merge_arch_params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        let arch_id = self.component_set_counter.len();
        self.component_set_counter.insert(component_set.to_vec(), arch_id);
        self.archetypes.insert(arch_id, new_arch);
        self.structure_changed();
        arch_id
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
            component_bit_set:              &mut self.temp_alloc.component_bit_set_a,
        };
        let new_arch = match ArchetypeSpec::new_from_pair(merge_arch_params)
        {
            Ok(r) => r,
            Err(e) => panic!("{}", e),
        };
        let arch_id = self.component_set_counter.len();
        self.component_set_counter.insert(component_set.to_vec(), arch_id);
        self.archetypes.insert(arch_id, new_arch);
        self.structure_changed();
        arch_id
    }
    #[track_caller]
    fn create_archetype<T: TArchetype + 'static>(&mut self, id: usize)
    {
        let params = ChunkLayoutParams {
            components:                 T::COMPONENT_DESCRIPTORS,
            component_specs:            &self.component_counter,
            state_offsets_temp:         &mut self.temp_alloc.state_offsets,
            component_descriptors_temp: &mut self.temp_alloc.col_descriptors,
            component_bit_set_temp:     &mut self.temp_alloc.component_bit_set_a,
        };
        let layout = match ChunkLayout::new(params)
        {
            Ok(r) => r,
            Err(e) => panic!("Create Archetype `{}` Failed: {e}", std::any::type_name::<T>()),
        };
        let arch_spec = ArchetypeSpec::new(layout);
        self.archetypes.insert(id, arch_spec);
    }

    fn structure_changed(&mut self)
    {
        self.global_archetype_version.increase();
    }
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
