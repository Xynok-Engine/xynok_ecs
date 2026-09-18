#![allow(unused)]
use std::cell::UnsafeCell;
use std::collections::VecDeque;

use xynok_async_toolkit::mwmr_spinlock::MwmrSpinLock;
use xynok_concurrency::utils::available_cores;
use xynok_std::collection::Queue;

use crate::apis::internal::CmdBuffer;
use crate::entity::Entity;

type SpecSlot = Box<UnsafeCell<WorkerSpec>>;

pub struct WorkerSpecs
{
    write_lock: MwmrSpinLock<bool>,
    workers:    Vec<SpecSlot>,
}
pub struct WorkerSpec
{
    pub pre_allocated_entities: VecDeque<Entity>,
    pub cmd_buffer:             VecDeque<CmdBuffer>,
}
impl WorkerSpecs
{
    pub fn new() -> Self
    {
        Self {
            write_lock: MwmrSpinLock::new(false),
            workers:    Vec::with_capacity(available_cores()),
        }
    }
    pub fn push(&mut self, worker: WorkerSpec) -> usize
    {
        let guard = self.write_lock.write();
        let idx = self.workers.len();
        self.workers.push(Box::new(UnsafeCell::new(worker)));
        drop(guard);
        idx
    }
    pub fn get_spec_at(&self, idx: usize) -> Option<&WorkerSpec>
    {
        let guard = self.write_lock.read();
        let ptr = self.workers.get(idx).map(|slot| slot.get());
        drop(guard);
        ptr.map(|p| unsafe { &*p })
    }
    pub fn get_spec_mut_at(&mut self, idx: usize) -> Option<&mut WorkerSpec>
    {
        let guard = self.write_lock.read();
        let ptr = self.workers.get(idx).map(|slot| slot.get());
        drop(guard);
        ptr.map(|p| unsafe { &mut *p })
    }
    #[track_caller]
    pub fn flush_cmd(&mut self)
    {
        for slot in self.workers.iter_mut()
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
        let idx = specs.push(WorkerSpec::new(4, 4));
        let first: *mut WorkerSpec = specs.get_spec_mut_at(idx).unwrap();

        // Push beyond the initial capacity to ensure the Vec reallocates at least once.
        for _ in 0..(available_cores() * 4 + 8)
        {
            specs.push(WorkerSpec::new(4, 4));
        }

        let after: *mut WorkerSpec = specs.get_spec_mut_at(idx).unwrap();
        assert_eq!(first, after, "spec moved during Vec growth, leaving the old reference dangling");

        // The old reference is still writable and points to the correct slot.
        unsafe { (*first).pre_allocated_entities.reserve(64) };
        assert!(specs.get_spec_at(idx).unwrap().pre_allocated_entities.capacity() >= 64);
    }
}
