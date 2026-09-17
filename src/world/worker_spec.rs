#![allow(unused)]
use std::collections::VecDeque;

use xynok_async_toolkit::mwmr_spinlock::MwmrSpinLock;
use xynok_concurrency::utils::available_cores;
use xynok_std::collection::Queue;

use crate::apis::internal::CmdBuffer;
use crate::entity::Entity;

pub struct WorkerSpecs
{
    write_lock: MwmrSpinLock<bool>,
    workers:    Vec<WorkerSpec>,
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
        self.workers.push(worker);
        drop(guard);
        idx
    }
    pub fn get_spec_at(&self, idx: usize) -> Option<&WorkerSpec>
    {
        self.workers.get(idx)
    }
    pub fn get_spec_mut_at(&mut self, idx: usize) -> Option<&mut WorkerSpec>
    {
        self.workers.get_mut(idx)
    }
    #[track_caller]
    pub fn flush_cmd(&mut self)
    {
        for e in self.workers.iter_mut()
        {
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
