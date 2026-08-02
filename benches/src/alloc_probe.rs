//! A global allocator that counts bytes/allocations on the current thread, so the benchmark
//! harness can measure allocation volume and leaks without any library under test having to
//! cooperate. Thread-local because `bevy_ecs` may spin up worker threads we don't control;
//! keeping the counters per-thread means only allocations made by the benchmark's own thread
//! are counted.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static LIVE_BYTES: Cell<isize> = const { Cell::new(0) };
    static TOTAL_BYTES: Cell<u64> = const { Cell::new(0) };
    static TOTAL_ALLOCS: Cell<u64> = const { Cell::new(0) };
}

pub struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8
    {
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
    live_bytes:    isize,
    total_bytes:   u64,
    total_allocs:  u64,
}

pub fn snapshot() -> Snapshot
{
    Snapshot {
        live_bytes:   LIVE_BYTES.with(|c| c.get()),
        total_bytes:  TOTAL_BYTES.with(|c| c.get()),
        total_allocs: TOTAL_ALLOCS.with(|c| c.get()),
    }
}

/// Bytes/allocations that happened strictly between `before` and `after`.
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

/// Live (still-allocated) bytes at `after` that weren't live at `before`. Anything above 0 once
/// every benchmark storage has been dropped means something leaked.
pub fn leaked_bytes(before: &Snapshot, after: &Snapshot) -> isize
{
    after.live_bytes - before.live_bytes
}
