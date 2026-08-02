//! Fixtures shared by the integration tests under `tests/`. Each test file is compiled as its
//! own crate, so this lives at `tests/common/mod.rs` (not `tests/common.rs`) precisely so cargo
//! does not also treat it as a standalone test binary.
#![allow(unused)]

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

use xynok_ecs::{component, entity::Entity, world::testing, world::World};

// ------------------------------------------------------------------------------------------------
// Component fixtures
// ------------------------------------------------------------------------------------------------

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hp(pub u32);

#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mana(pub u32);

#[component]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pos
{
    pub x: f32,
    pub y: f32,
}

/// Zero-sized component: every row shares the same offset and `byte_size == 0`.
#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker;

/// Over-aligned component, to check that column offsets honour `align_of`.
#[component]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(align(32))]
pub struct Aligned32(pub u64);

/// Component with a non-trivial `Drop`, used to verify the drop glue is invoked
/// exactly once per stored value.
#[component]
#[derive(Debug)]
pub struct Tracked(pub u32);

pub static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
/// Serialises the drop-counter tests so they stay meaningful under a parallel test runner.
pub static DROP_TEST_LOCK: Mutex<()> = Mutex::new(());

impl Drop for Tracked
{
    fn drop(&mut self)
    {
        DROP_COUNT.fetch_add(1, Ordering::SeqCst);
    }
}

pub fn reset_drop_count()
{
    DROP_COUNT.store(0, Ordering::SeqCst);
}
pub fn drop_count() -> usize
{
    DROP_COUNT.load(Ordering::SeqCst)
}

// ------------------------------------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------------------------------------

/// Small LCG so the stress tests are pseudo-random but fully reproducible.
pub struct Rng(u64);
impl Rng
{
    pub fn new(seed: u64) -> Self
    {
        Self(seed)
    }
    pub fn next_u32(&mut self) -> u32
    {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    pub fn below(&mut self, upper: usize) -> usize
    {
        (self.next_u32() as usize) % upper
    }
}

/// Asserts that every live entity still maps to the chunk row that stores its own handle.
/// This is the core invariant that swap-remove must preserve.
pub fn assert_entity_mapping_is_consistent(w: &World, live: &[Entity])
{
    for &e in live
    {
        let stored = testing::entity_stored_at_row_of(w, e);
        let loc = testing::entity_location(w, e);
        assert_eq!(stored, e, "row {} of chunk {} should hold {} but holds {}", loc.idx_in_chunk, loc.chunk_idx, e, stored);
    }
}
