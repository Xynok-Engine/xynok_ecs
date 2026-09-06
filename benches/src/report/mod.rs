//! The shape of the combined report, and the three ways it gets written out.
//!
//! A row is one (library, layout, arity, entity count) scenario with two halves joined onto it:
//! the timing half comes from criterion, the memory half from the counting allocator. Either half
//! can be missing, which is normal rather than an error: a filtered `cargo bench` run only produces
//! timings for the benchmarks it ran.

pub mod html;
pub mod json;
pub mod table;

use serde::Serialize;

use crate::criterion_data::{CriterionResult, Estimate};
use crate::workload::ArchetypeLayout;

/// A criterion estimate, flattened for the report.
///
/// Criterion resamples the collected samples to get these, so the interval is a real statement
/// about the measurement and not a guess from the standard deviation. Keeping the bounds means the
/// report can say when two libraries are too close to call instead of ranking noise.
#[derive(Serialize, Clone, Copy, Debug)]
pub struct Interval
{
    pub point:            f64,
    pub lower:            f64,
    pub upper:            f64,
    pub standard_error:   f64,
    pub confidence_level: f64,
}

impl From<Estimate> for Interval
{
    fn from(estimate: Estimate) -> Self
    {
        Interval {
            point:            estimate.point_estimate,
            lower:            estimate.confidence_interval.lower_bound,
            upper:            estimate.confidence_interval.upper_bound,
            standard_error:   estimate.standard_error,
            confidence_level: estimate.confidence_interval.confidence_level,
        }
    }
}

/// What criterion measured for one scenario. Every time is nanoseconds for one full pass.
#[derive(Serialize, Clone, Debug)]
pub struct Timing
{
    pub mean:                Interval,
    pub median:              Interval,
    pub std_dev:             Option<Interval>,
    pub median_abs_dev:      Option<Interval>,
    /// Criterion's linear-regression estimate of the per-iteration cost. Usually the number to
    /// quote when it exists: fitting a line through growing batches cancels the fixed overhead of
    /// entering and leaving the timed region. Absent for flat-sampled benchmarks.
    pub slope:               Option<Interval>,
    pub min_ns:              f64,
    pub max_ns:              f64,
    pub p95_ns:              f64,
    pub p99_ns:              f64,
    pub sample_count:        usize,
    pub total_iters:         f64,
    pub sampling_mode:       String,
    /// Entities per second, which is what makes different entity counts comparable.
    pub elements_per_second: Option<f64>,
    /// Mean time divided by the entity count: how long one entity costs in this scenario.
    pub ns_per_entity:       f64,
    /// Relative change of the mean since the previous run, as a fraction (`0.03` is 3% slower).
    /// `None` until a scenario has been benchmarked twice.
    pub change_mean:         Option<Interval>,
    pub change_median:       Option<Interval>,
}

impl Timing
{
    pub fn from_criterion(result: &CriterionResult, entity_count: usize) -> Self
    {
        Timing {
            mean:                result.mean.into(),
            median:              result.median.into(),
            std_dev:             result.std_dev.map(Into::into),
            median_abs_dev:      result.median_abs_dev.map(Into::into),
            slope:               result.slope.map(Into::into),
            min_ns:              result.min_ns,
            max_ns:              result.max_ns,
            p95_ns:              result.p95_ns,
            p99_ns:              result.p99_ns,
            sample_count:        result.sample_count,
            total_iters:         result.total_iters,
            sampling_mode:       result.sampling_mode.clone(),
            elements_per_second: result.elements_per_second(),
            ns_per_entity:       result.mean.point_estimate / entity_count.max(1) as f64,
            change_mean:         result.change_mean.map(Into::into),
            change_median:       result.change_median.map(Into::into),
        }
    }
}

/// What the counting allocator saw for one scenario. Criterion has no opinion on any of this.
#[derive(Serialize, Clone, Copy, Debug, Default)]
pub struct Memory
{
    /// Bytes still held once the storage is built. This is the footprint of storing those entities,
    /// including whatever the library rounds up to: chunk padding, table capacity growth, indices.
    pub resident_bytes:         u64,
    pub bytes_per_entity:       f64,
    /// Every byte the storage asked for while being built, freed again or not. Much larger than
    /// `resident_bytes` means a lot of copying while growing.
    pub allocated_bytes:        u64,
    pub allocations:            u64,
    /// Bytes allocated inside the pass criterion times, over [`QUERY_LOOP_PASSES`] passes. Anything
    /// but 0 means that benchmark's timing includes allocator work and is not iteration-only.
    pub query_loop_bytes:       u64,
    pub query_loop_allocations: u64,
    /// Bytes still live after the storage was dropped. Anything but 0 is a leak.
    pub leaked_bytes:           i64,
}

/// Passes run inside the measured region when checking the query loop for allocation. More than one
/// so a library that allocates on the first pass only (a lazily built query plan, say) still shows.
pub const QUERY_LOOP_PASSES: usize = 32;
/// Passes run before that region, so first-touch page faults and one-time query setup are not
/// counted as steady-state allocation.
pub const QUERY_LOOP_WARMUP: usize = 8;

#[derive(Serialize, Clone, Debug)]
pub struct BenchRow
{
    /// How the library is spelled in the report.
    pub library:          String,
    /// How it is spelled in criterion ids and directory names.
    pub library_id:       String,
    pub entity_count:     usize,
    pub component_count:  u8,
    pub archetype_layout: ArchetypeLayout,
    pub layout_label:     String,
    /// The criterion id this row's timing came from, so a number in the report can be traced back
    /// to the directory it was read out of.
    pub criterion_id:     String,
    /// `None` when `cargo bench` has not run this scenario yet.
    pub timing:           Option<Timing>,
    pub memory:           Memory,
}

/// Where and when the numbers were produced. Timings only mean something next to this.
#[derive(Serialize, Clone, Debug)]
pub struct Environment
{
    pub harness:               String,
    pub os:                    String,
    pub arch:                  String,
    pub available_parallelism: usize,
    /// Seconds since the unix epoch, formatted on the page rather than here.
    pub generated_at_unix:     u64,
    pub query_loop_passes:     usize,
}

impl Environment
{
    pub fn detect() -> Self
    {
        Environment {
            harness:               format!("criterion {}", CRITERION_VERSION),
            os:                    std::env::consts::OS.to_string(),
            arch:                  std::env::consts::ARCH.to_string(),
            available_parallelism: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
            generated_at_unix:     std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            query_loop_passes:     QUERY_LOOP_PASSES,
        }
    }
}

/// Kept next to the criterion dependency in `Cargo.toml`; there is no way to read a dev-dependency
/// version at runtime from a binary that does not depend on it.
pub const CRITERION_VERSION: &str = "0.8";

#[derive(Serialize, Clone, Debug)]
pub struct Report
{
    pub environment: Environment,
    pub rows:        Vec<BenchRow>,
}
