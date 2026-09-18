#![allow(unused)]
use std::cell::UnsafeCell;
use std::collections::VecDeque;

use xynok_async_toolkit::mwmr_spinlock::MwmrSpinLock;
use xynok_concurrency::utils::available_cores;
use xynok_std::collection::Queue;

use crate::apis::internal::CmdBuffer;
use crate::collection::sequence_value_hash_map::SequenceValueHashMap;
use crate::entity::Entity;

type SpecSlot = Box<UnsafeCell<WorkerSpec>>;

pub struct WorkerSpecs
{
    write_lock: MwmrSpinLock<bool>,
    workers:    SequenceValueHashMap<std::thread::ThreadId, SpecSlot>,
}
pub struct WorkerSpec
{
    pub pre_allocated_entities: VecDeque<Entity>,
    pub cmd_buffer:             VecDeque<CmdBuffer>,
}
#[inline]
fn current_thread_id() -> std::thread::ThreadId
{
    std::thread::current().id()
}
impl WorkerSpecs
{
    pub fn new() -> Self
    {
        Self {
            write_lock: MwmrSpinLock::new(false),
            workers:    SequenceValueHashMap::with_capacity(available_cores()),
        }
    }
    pub fn is_registered(&self) -> bool
    {
        self.workers.contains_key(&current_thread_id())
    }
    pub fn push(&mut self, worker: WorkerSpec)
    {
        let guard = self.write_lock.write();
        let idx = self.workers.len();
        self.workers.insert(std::thread::current().id(), Box::new(UnsafeCell::new(worker)));
        drop(guard);
    }
    pub fn get_current_worker_spec(&self) -> Option<&UnsafeCell<WorkerSpec>>
    {
        self.workers.get(&current_thread_id()).map(|slot| slot.as_ref())
    }
    #[track_caller]
    pub fn flush_cmd(&mut self)
    {
        for slot in self.workers.values_mut()
        {
            let e = slot.get_mut();
            while let Some(cmd) = e.cmd_buffer.pop_front()
            {
                cmd.run();
            }
        }
    }
}
impl WorkerSpec
{
    pub fn new(entity_capacity: usize, cmd_buffer_capacity: usize) -> Self
    {
        Self {
            pre_allocated_entities: VecDeque::with_capacity(entity_capacity),
            cmd_buffer:             VecDeque::with_capacity(cmd_buffer_capacity),
        }
    }
}

#[cfg(test)]
mod tests
{
    use super::*;

    /// Issue #69
    #[test]
    fn t_spec_address_survives_vec_growth()
    {
        let mut specs = WorkerSpecs::new();
        specs.push(WorkerSpec::new(4, 4));
        let first = specs.get_current_worker_spec().unwrap().get();

        // Every push keys off the calling thread, so growing the map means registering from
        // fresh threads. Each scope joins before the next one starts, so only one borrow of
        // `specs` is live at a time.
        for _ in 0..(available_cores() * 4 + 8)
        {
            std::thread::scope(|s| {
                s.spawn(|| specs.push(WorkerSpec::new(4, 4)));
            });
        }
        assert!(specs.workers.len() > 1, "the map never grew, so the test is not exercising Vec growth");

        let after = specs.get_current_worker_spec().unwrap().get();
        assert_eq!(first, after, "spec moved during Vec growth, leaving the old reference dangling");

        // The old reference is still writable and points to the correct slot.
        unsafe { (*first).pre_allocated_entities.reserve(64) };
        assert!(unsafe { &*specs.get_current_worker_spec().unwrap().get() }.pre_allocated_entities.capacity() >= 64);
    }
}
