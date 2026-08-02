//! Verifies `World` actually releases the chunk memory it allocates. Needs its own
//! `#[global_allocator]`, so it lives in its own binary, separate from the other integration
//! tests under `tests/`.
mod common;

use common::*;
use xynok_ecs::world::World;

/// Counts live chunk-sized allocations so a leaked `Chunk` buffer is observable from a test.
/// The counter is thread-local, which keeps it accurate under the parallel test runner: every
/// test runs on its own thread and only sees its own allocations.
mod alloc_probe
{
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };

    use xynok_ecs::apis::constants::CHUNK_SIZE_IN_BYTE;

    thread_local! {
        static LIVE_CHUNKS: Cell<isize> = const { Cell::new(0) };
    }

    pub struct ChunkCountingAllocator;

    unsafe impl GlobalAlloc for ChunkCountingAllocator
    {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8
        {
            if layout.size() == CHUNK_SIZE_IN_BYTE
            {
                // `try_with` because TLS is unavailable while a thread is being torn down
                let _ = LIVE_CHUNKS.try_with(|live| live.set(live.get() + 1));
            }
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout)
        {
            if layout.size() == CHUNK_SIZE_IN_BYTE
            {
                let _ = LIVE_CHUNKS.try_with(|live| live.set(live.get() - 1));
            }
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    pub fn live_chunks() -> isize
    {
        LIVE_CHUNKS.with(|live| live.get())
    }
}

#[global_allocator]
static CHUNK_ALLOC_PROBE: alloc_probe::ChunkCountingAllocator = alloc_probe::ChunkCountingAllocator;

#[test]
fn t_dropping_the_world_releases_its_chunk_memory()
{
    let before = alloc_probe::live_chunks();

    {
        let mut w = World::default();
        for i in 0..8u32
        {
            w.create(Hp(i));
        }
        assert!(alloc_probe::live_chunks() > before, "creating entities must allocate at least one chunk");
    }

    assert_eq!(
        alloc_probe::live_chunks(),
        before,
        "dropping the world must release every chunk buffer it allocated"
    );
}

/// Archetype migrations allocate a chunk in the destination archetype; those must be released too.
#[test]
fn t_dropping_the_world_releases_chunks_created_by_migrations()
{
    let before = alloc_probe::live_chunks();

    {
        let mut w = World::default();
        let e = w.create(Hp(1));
        w.add_component(e, Mana(2));
        w.add_component(e, Pos { x: 0.0, y: 0.0 });
        let _ = w.remove_component::<Mana>(e);
    }

    assert_eq!(
        alloc_probe::live_chunks(),
        before,
        "every archetype's chunks must be released, not just the first"
    );
}
