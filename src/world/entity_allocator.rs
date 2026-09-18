use std::cell::UnsafeCell;
use std::collections::VecDeque;

use crate::apis::identifies::XynokEcsError;
use crate::apis::params::{EntityInChunkIndices, SwappedRow};
use crate::entity::Entity;
use crate::world::entity_spec::EntitySpec;
use xynok_async_toolkit::mwmr_spinlock::MwmrSpinLock;

pub struct EntityAllocator
{
    // The lock only gates the worker path. Main thread work goes through `&mut self`, which is
    // already exclusive, so it reads the cells directly and pays nothing.
    write_lock:           MwmrSpinLock<bool>,
    entities:             UnsafeCell<Vec<EntitySpec>>,
    free_entities:        UnsafeCell<VecDeque<usize>>,
    // Slots that ran out of versions and will never be handed out again, see `erase_entity`.
    // Kept as a plain count because nothing can be done with them, it is only here so the
    // leak is observable rather than silent.
    retired_entity_slots: UnsafeCell<usize>,
}
// The cells are only ever touched either from the main thread through `&mut self`, or from a
// worker holding the write guard. Those two never overlap, so sharing the allocator across
// threads is fine even though `UnsafeCell` is not `Sync` on its own.
unsafe impl Sync for EntityAllocator {}

// MAIN THREAD ONLY
impl EntityAllocator
{
    pub fn new(capacity: usize) -> Self
    {
        Self {
            write_lock:           MwmrSpinLock::new(false),
            entities:             UnsafeCell::new(Vec::with_capacity(capacity)),
            free_entities:        UnsafeCell::new(VecDeque::with_capacity(capacity)),
            retired_entity_slots: UnsafeCell::new(0),
        }
    }
    #[inline]
    fn entities(&self) -> &Vec<EntitySpec>
    {
        unsafe { &*self.entities.get() }
    }
    #[inline]
    fn entities_mut(&mut self) -> &mut Vec<EntitySpec>
    {
        self.entities.get_mut()
    }
    #[inline]
    pub fn total_entities(&self) -> usize
    {
        self.entities().len()
    }
    #[inline]
    pub fn retired_entity_slots(&self) -> usize
    {
        unsafe { *self.retired_entity_slots.get() }
    }
    #[inline]
    pub fn get_unchecked(&self, idx: usize) -> &EntitySpec
    {
        unsafe { self.entities().get_unchecked(idx) }
    }
    #[inline]
    pub fn get_unchecked_mut(&mut self, idx: usize) -> &mut EntitySpec
    {
        unsafe { self.entities_mut().get_unchecked_mut(idx) }
    }
    #[inline]
    pub fn new_entity(&mut self) -> Result<Entity, XynokEcsError>
    {
        // `&mut self` is exclusive, so no guard is needed.
        unsafe { self.new_entity_unlocked() }
    }
    #[track_caller]
    #[inline]
    pub fn update_entity_indices(&mut self, swapped_row: SwappedRow)
    {
        let swapped_e_spec = match self.entities_mut().get_mut(swapped_row.e.idx())
        {
            Some(r) => r,
            None => panic!("Swapped {} not found to update indices !", swapped_row.e),
        };

        swapped_e_spec.update_idx_in_chunk(swapped_row.from, swapped_row.to);
    }
    #[track_caller]
    #[inline]
    pub fn update_entity_spec(&mut self, e: Entity, arch_id: usize, indices: EntityInChunkIndices)
    {
        let entity_spec = self.get_unchecked_mut(e.idx());
        *entity_spec = EntitySpec::new(arch_id, indices.chunk_idx, indices.idx_in_chunk, e.version());
    }

    /// Mark this entity as invalid. If its version is exceeded, the entity becomes obsolete.
    #[inline]
    pub fn erase_entity(&mut self, e: Entity)
    {
        let version = unsafe {
            let e_spec = self.entities_mut().get_unchecked_mut(e.idx());
            e_spec.errase();
            e_spec.version()
        };

        match version < Entity::MAX_VERSION
        {
            true => self.free_entities.get_mut().push_back(e.idx()),
            false => *self.retired_entity_slots.get_mut() += 1,
        }
    }
}

// WORKER THREADS
impl EntityAllocator
{
    #[inline]
    pub fn pre_allocate_entities(&self, amount: usize, dst: &mut VecDeque<Entity>) -> Result<(), XynokEcsError>
    {
        if amount < 1
        {
            return Err(XynokEcsError::PreAllocateEntityAmountMustGreaterThanZero);
        }
        let guard = self.write_lock.write();
        dst.reserve(amount);
        let mut counter = amount;
        while counter > 0
        {
            // Safe here: the write guard is the only thing allowed to hand out entities
            // concurrently, and we are holding it.
            let new_e = unsafe { self.new_entity_unlocked()? };
            dst.push_back(new_e);
            counter -= 1;
        }
        drop(guard);
        Ok(())
    }
    // Caller must own the write guard, or be the main thread holding `&mut self`.
    #[inline]
    unsafe fn new_entity_unlocked(&self) -> Result<Entity, XynokEcsError>
    {
        let entities = unsafe { &mut *self.entities.get() };
        let free_entities = unsafe { &mut *self.free_entities.get() };

        if let Some(free_idx) = free_entities.pop_front()
        {
            let old_slot = unsafe { entities.get_unchecked_mut(free_idx) };
            // `erase_entity` only enqueues a slot with a version left to spend, so the `+ 1`
            // cannot run past `MAX_VERSION` here and `Entity::new` has nothing to refuse
            debug_assert!(
                old_slot.version() < Entity::MAX_VERSION,
                "slot {free_idx} was recycled at version {} but should have been retired",
                old_slot.version()
            );
            return Entity::new(free_idx, old_slot.version() + 1);
        };
        let e = Entity::new(entities.len(), Entity::INITIALIZE_VERSION)?;
        entities.push(EntitySpec::new_empty_slot(e.version()));
        Ok(e)
    }
}
