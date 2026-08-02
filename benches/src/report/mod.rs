pub mod html;
pub mod json;
pub mod table;

use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug, Default)]
pub struct AllocStats
{
    pub bytes:       u64,
    pub allocations: u64,
}

#[derive(Serialize, Clone, Copy, Debug, Default)]
pub struct QueryTiming
{
    pub warmup_iters:   usize,
    pub measured_iters: usize,
    pub min_ns:         u128,
    pub max_ns:         u128,
    pub mean_ns:        u128,
    pub median_ns:      u128,
}

#[derive(Serialize, Clone, Debug)]
pub struct BenchResult
{
    pub library:      String,
    pub entity_count: usize,
    /// Allocation caused by building the storage (creating every entity/component).
    pub setup_alloc:  AllocStats,
    /// Allocation observed strictly inside the timed query loop. Should be ~0 for every
    /// competitor; anything else means "speed" and "allocation" leaked into the same
    /// measurement window and the timing below is not query-only anymore.
    pub query_alloc:  AllocStats,
    /// Bytes still live after the storage was dropped, relative to before setup. Non-zero means
    /// the competitor leaked memory for this run.
    pub leaked_bytes:  i64,
    pub query_timing:  QueryTiming,
}

pub fn median_ns(samples: &mut [u128]) -> u128
{
    samples.sort_unstable();
    let mid = samples.len() / 2;
    if samples.len() % 2 == 0
    {
        (samples[mid - 1] + samples[mid]) / 2
    }
    else
    {
        samples[mid]
    }
}
