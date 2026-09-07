//! A global allocator that counts bytes and allocations on the calling thread.
//!
//! It lets the report binary measure how much memory a storage costs without any library under test
//! having to cooperate, or even know it is being watched.
//!
//! Every allocation is counted twice, once per thread and once for the whole process, because the
//! report asks two different questions:
//!
//! - Building a storage happens on one thread, and `bevy_ecs` keeps worker threads around that this
//!   crate does not control. A process-wide counter would fold their bookkeeping into whichever
//!   measurement happened to be running, so [`snapshot`] reads per-thread counters and sees only
//!   what the measuring thread did.
//! - A frame of a parallel schedule happens on every worker at once, and per-thread counters would
//!   miss all of it but the caller's share. [`process_snapshot`] reads the process-wide counters
//!   for that case. It is only meaningful around a region that joins its workers before it ends,
//!   which a schedule step does.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};

thread_local! {
    /// Bytes allocated minus bytes freed. Can go negative on a thread that frees memory another
    /// thread allocated, which is why it is signed.
    static LIVE_BYTES: Cell<isize> = const { Cell::new(0) };
    /// Bytes ever requested, never decremented.
    static TOTAL_BYTES: Cell<u64> = const { Cell::new(0) };
    static TOTAL_ALLOCS: Cell<u64> = const { Cell::new(0) };
}

/// The same three counters again, for the whole process this time. `Relaxed` is all they need:
/// nothing is published through them, and a reader that wants a settled value has to join the
/// threads that were writing to them anyway.
static PROCESS_LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);
static PROCESS_TOTAL_BYTES: AtomicU64 = AtomicU64::new(0);
static PROCESS_TOTAL_ALLOCS: AtomicU64 = AtomicU64::new(0);

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
        count_process(layout.size() as isize, layout.size() as u64);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout)
    {
        let _ = LIVE_BYTES.try_with(|c| c.set(c.get() - layout.size() as isize));
        PROCESS_LIVE_BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8
    {
        let _ = LIVE_BYTES.try_with(|c| c.set(c.get() + layout.size() as isize));
        let _ = TOTAL_BYTES.try_with(|c| c.set(c.get() + layout.size() as u64));
        let _ = TOTAL_ALLOCS.try_with(|c| c.set(c.get() + 1));
        count_process(layout.size() as isize, layout.size() as u64);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8
    {
        let _ = LIVE_BYTES.try_with(|c| c.set(c.get() - layout.size() as isize + new_size as isize));
        let _ = TOTAL_BYTES.try_with(|c| c.set(c.get() + new_size.saturating_sub(layout.size()) as u64));
        let _ = TOTAL_ALLOCS.try_with(|c| c.set(c.get() + 1));
        count_process(new_size as isize - layout.size() as isize, new_size.saturating_sub(layout.size()) as u64);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// One allocation, on the process-wide counters. Split out because all four allocator methods do
/// the same three increments with different numbers.
#[inline]
fn count_process(live_delta: isize, requested: u64)
{
    PROCESS_LIVE_BYTES.fetch_add(live_delta, Ordering::Relaxed);
    PROCESS_TOTAL_BYTES.fetch_add(requested, Ordering::Relaxed);
    PROCESS_TOTAL_ALLOCS.fetch_add(1, Ordering::Relaxed);
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

/// The same reading, taken over every thread in the process.
///
/// Only worth taking around a region that joins whatever threads it started, otherwise the two
/// snapshots catch a worker mid-flight and the difference is whatever the timing happened to be.
pub fn process_snapshot() -> Snapshot
{
    Snapshot {
        live_bytes:   PROCESS_LIVE_BYTES.load(Ordering::Relaxed),
        total_bytes:  PROCESS_TOTAL_BYTES.load(Ordering::Relaxed),
        total_allocs: PROCESS_TOTAL_ALLOCS.load(Ordering::Relaxed),
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
