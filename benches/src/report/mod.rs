pub mod html;
pub mod json;
pub mod table;

use serde::Serialize;

use crate::common::ArchetypeLayout;

#[derive(Serialize, Clone, Copy, Debug, Default)]
pub struct AllocStats
{
    pub bytes:       u64,
    pub allocations: u64,
}

#[derive(Serialize, Clone, Copy, Debug, Default)]
pub struct QueryTiming
{
    pub warmup_iters:     usize,
    /// Number of timed samples collected; each sample is itself the average of `batch_size`
    /// back-to-back calls (see `batch_size`).
    pub measured_samples: usize,
    /// How many `run_query_once` calls were batched into a single timed sample, chosen so each
    /// sample covers at least `MIN_SAMPLE_NANOS` of wall time. 1 for queries already slower than
    /// that threshold.
    pub batch_size:       usize,
    pub min_ns:           f64,
    pub max_ns:           f64,
    pub mean_ns:          f64,
    pub median_ns:        f64,
    pub p95_ns:           f64,
    pub p99_ns:           f64,
    pub stddev_ns:        f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct BenchResult
{
    pub library:          String,
    pub entity_count:     usize,
    /// Number of components read/written by the timed query (1, 2, or 3).
    pub component_count:  u8,
    pub archetype_layout: ArchetypeLayout,
    /// Allocation caused by building the storage (creating every entity/component).
    pub setup_alloc:      AllocStats,
    /// Allocation observed strictly inside the timed query loop. Should be ~0 for every
    /// competitor; anything else means "speed" and "allocation" leaked into the same
    /// measurement window and the timing below is not query-only anymore.
    pub query_alloc:      AllocStats,
    /// Bytes still live after the storage was dropped, relative to before setup. Non-zero means
    /// the competitor leaked memory for this run.
    pub leaked_bytes:     i64,
    pub query_timing:     QueryTiming,
}

/// Returns the value at the given percentile (0.0..=100.0) of an already-sorted, non-empty
/// slice, using nearest-rank interpolation.
pub fn percentile_of_sorted(sorted_samples: &[f64], p: f64) -> f64
{
    let rank = ((p / 100.0) * (sorted_samples.len() - 1) as f64).round() as usize;
    sorted_samples[rank.min(sorted_samples.len() - 1)]
}

pub fn mean_ns(samples: &[f64]) -> f64
{
    samples.iter().sum::<f64>() / samples.len() as f64
}

/// Sample standard deviation (Bessel's correction); 0 for fewer than 2 samples.
pub fn stddev_ns(samples: &[f64], mean: f64) -> f64
{
    if samples.len() < 2
    {
        return 0.0;
    }
    let variance = samples.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / (samples.len() - 1) as f64;
    variance.sqrt()
}
