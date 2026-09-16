//! The settings `benches/query_iter.rs` runs with, shared with the report so both read the same
//! numbers.
//!
//! That benchmark walks one query three ways (`iter`, `iter_chunk`, `iter_batch`) and compares
//! each against the closest thing bevy has. It reuses the storage and the 3 component pass from
//! [`crate::workload`], so there is no workload of its own here, only which slice of it runs.

use serde::Serialize;

use crate::workload::ArchetypeLayout;

/// Five archetypes is the layout where crossing from one archetype or chunk to the next actually
/// shows up in the numbers.
pub const ITER_MODE_LAYOUT: ArchetypeLayout = ArchetypeLayout::Fragmented5;

/// Rows per batch in the `iter_batch` benchmarks, the same number on both sides. At 1k entities
/// that is a single batch, so that row mostly shows what handing work to the pool costs.
pub const ITER_BATCH_SIZE: usize = 1_024;

/// One way of walking a query.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IterMode
{
    /// One entity at a time, on one thread. bevy: `iter_mut`.
    Iter,
    /// A loop over chunks with an inner loop per chunk. bevy has no public per chunk walk, so its
    /// side is `iter_mut` again.
    IterChunk,
    /// Batches spawned on the task pool. bevy: `par_iter_mut` with the same fixed batch size.
    IterBatch,
}

impl IterMode
{
    pub const ALL: [IterMode; 3] = [IterMode::Iter, IterMode::IterChunk, IterMode::IterBatch];

    /// The middle part of the criterion id, `query_iter/<slug>/<library>/<count>`. Changing it
    /// invalidates criterion data already on disk.
    pub fn slug(self) -> &'static str
    {
        match self
        {
            IterMode::Iter => "iter",
            IterMode::IterChunk => "iter_chunk",
            IterMode::IterBatch => "iter_batch",
        }
    }

    /// What each side actually calls, for a human reading a table.
    pub fn label(self) -> &'static str
    {
        match self
        {
            IterMode::Iter => "iter (bevy: iter_mut)",
            IterMode::IterChunk => "iter_chunk (bevy: iter_mut)",
            IterMode::IterBatch => "iter_batch (bevy: par_iter_mut)",
        }
    }

    pub fn is_parallel(self) -> bool
    {
        self == IterMode::IterBatch
    }
}
