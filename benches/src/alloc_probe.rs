//! A global allocator that counts bytes and allocations on the calling thread.
//!
//! It lets `memory_report` measure how much memory a storage costs without any library under test
//! having to cooperate, or even know it is being watched.
//!
//! The counters are thread-local on purpose. `bevy_ecs` keeps worker threads around that this crate
//! does not control, and a process-wide counter would fold their bookkeeping into whichever
//! measurement happened to be running. Per-thread counters see only the allocations made by the
//! thread doing the measuring, which is exactly the storage being built in front of it.
//!
//! The flip side is that this cannot measure anything allocated on a worker thread, so it is only
//! wired up for the single-threaded workloads in `memory_report`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Bytes allocated minus bytes freed. Can go negative on a thread that frees memory another
    /// thread allocated, which is why it is signed.
    static LIVE_BYTES: Cell<isize> = const { Cell::new(0) };
    /// Bytes ever requested, never decremented.
    static TOTAL_BYTES: Cell<u64> = const { Cell::new(0) };
    static TOTAL_ALLOCS: Cell<u64> = const { Cell::new(0) };
}

pub struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8
    {
        // `try_with` throughout: thread-local storage is gone while a thread is being torn down, and
        // an allocation during teardown must not panic.
        let _ = LIVE_BYTES.try_with(|c| c.set(c.get() + layout.size() as isize));
        let _ = TOTAL_BYTES.try_with(|c| c.set(c.get() + layout.size() as u64));
        let _ = TOTAL_ALLOCS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout)
    {
        let _ = LIVE_BYTES.try_with(|c| c.set(c.get() - layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8
    {
        let _ = LIVE_BYTES.try_with(|c| c.set(c.get() + layout.size() as isize));
        let _ = TOTAL_BYTES.try_with(|c| c.set(c.get() + layout.size() as u64));
        let _ = TOTAL_ALLOCS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8
    {
        let _ = LIVE_BYTES.try_with(|c| c.set(c.get() - layout.size() as isize + new_size as isize));
        let _ = TOTAL_BYTES.try_with(|c| c.set(c.get() + new_size.saturating_sub(layout.size()) as u64));
        let _ = TOTAL_ALLOCS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Snapshot
{
    live_bytes:   isize,
    total_bytes:  u64,
    total_allocs: u64,
}

pub fn snapshot() -> Snapshot
{
    Snapshot {
        live_bytes:   LIVE_BYTES.with(|c| c.get()),
        total_bytes:  TOTAL_BYTES.with(|c| c.get()),
        total_allocs: TOTAL_ALLOCS.with(|c| c.get()),
    }
}

/// Bytes and allocations requested strictly between two snapshots, whether or not they were freed
/// again.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllocDelta
{
    pub bytes:       u64,
    pub allocations: u64,
}

pub fn delta(before: &Snapshot, after: &Snapshot) -> AllocDelta
{
    AllocDelta {
        bytes:       after.total_bytes.saturating_sub(before.total_bytes),
        allocations: after.total_allocs.saturating_sub(before.total_allocs),
    }
}

/// How much more memory is held at `after` than was held at `before`.
///
/// Taken around "build the storage" this is its resident footprint. Taken around "build it and drop
/// it again" anything other than 0 is a leak.
pub fn live_delta(before: &Snapshot, after: &Snapshot) -> isize
{
    after.live_bytes - before.live_bytes
}
