//! Reads back what `cargo bench` measured.
//!
//! Criterion writes one directory per benchmark under `target/criterion`, and the report binary
//! wants those numbers rather than a second set of its own: measuring the same thing twice with two
//! different harnesses is how a report ends up disagreeing with itself. So nothing here times
//! anything. It walks the directory, parses the JSON criterion already wrote, and hands back the
//! statistics.
//!
//! Four files per benchmark matter:
//!
//! - `new/benchmark.json`   which scenario this is, and the throughput criterion was told about
//! - `new/estimates.json`   the bootstrapped mean, median, std dev, MAD and slope, in nanoseconds
//! - `new/sample.json`      the raw samples, which is where min/max and the percentiles come from
//! - `change/estimates.json` how this run compares to the previous one, when there was one
//!
//! Criterion has no opinion on memory, so nothing about allocation appears here. That is measured
//! separately in `src/bin/report.rs` and joined onto these rows by scenario.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One estimate criterion produced, with the confidence interval it bootstrapped for it.
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Estimate
{
    pub point_estimate:      f64,
    pub standard_error:      f64,
    pub confidence_interval: ConfidenceInterval,
}

#[derive(Deserialize, Clone, Copy, Debug)]
pub struct ConfidenceInterval
{
    pub confidence_level: f64,
    pub lower_bound:      f64,
    pub upper_bound:      f64,
}

#[derive(Deserialize, Debug)]
struct Estimates
{
    mean:           Estimate,
    median:         Estimate,
    median_abs_dev: Option<Estimate>,
    std_dev:        Option<Estimate>,
    /// Only present when criterion could fit a line through the samples, which needs linear
    /// sampling. Benchmarks slow enough to fall back to flat sampling have no slope.
    slope:          Option<Estimate>,
}

#[derive(Deserialize, Debug)]
struct ChangeEstimates
{
    mean:   Estimate,
    median: Estimate,
}

#[derive(Deserialize, Debug)]
struct Sample
{
    /// `Linear` or `Flat`. Worth carrying through to the report: a flat-sampled benchmark has no
    /// slope estimate, and its samples are single iterations rather than growing batches.
    sampling_mode: String,
    iters:         Vec<f64>,
    times:         Vec<f64>,
}

#[derive(Deserialize, Debug)]
struct BenchmarkId
{
    group_id:    String,
    function_id: Option<String>,
    value_str:   Option<String>,
    throughput:  Option<Throughput>,
}

/// Criterion writes whichever variant the benchmark declared. This crate only ever declares
/// `Elements`, but all four have to be listed or the file would fail to parse the day one of the
/// other benchmarks starts reporting bytes.
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
enum Throughput
{
    Bytes(u64),
    Elements(u64),
    BytesDecimal(u64),
    Bits(u64),
}

/// Everything criterion knows about one benchmark, flattened into the shape the report wants.
#[derive(Clone, Debug)]
pub struct CriterionResult
{
    /// `query/1_archetype/2_components`, straight from criterion.
    pub group_id:       String,
    /// The library slug, matching `QueryWorkload::NAME`.
    pub function_id:    String,
    /// The entity-count label, matching `count_label`.
    pub value_str:      String,
    pub elements:       Option<u64>,
    pub sampling_mode:  String,
    /// Time for one call of the benchmarked closure, in nanoseconds, with its confidence interval.
    pub mean:           Estimate,
    pub median:         Estimate,
    pub median_abs_dev: Option<Estimate>,
    pub std_dev:        Option<Estimate>,
    pub slope:          Option<Estimate>,
    /// Per-sample times, already divided by the iteration count of that sample, so every entry is
    /// comparable to `mean` regardless of how many iterations criterion batched into it.
    pub min_ns:         f64,
    pub max_ns:         f64,
    pub p95_ns:         f64,
    pub p99_ns:         f64,
    pub sample_count:   usize,
    pub total_iters:    f64,
    /// Relative change of the mean against the previous run, as a fraction (`0.03` is 3% slower).
    /// `None` on the first run, or after the criterion directory was cleared.
    pub change_mean:    Option<Estimate>,
    pub change_median:  Option<Estimate>,
}

impl CriterionResult
{
    /// `query/1_archetype/2_components/xynok_ecs/10k`, the id criterion prints while benching.
    pub fn full_id(&self) -> String
    {
        format!("{}/{}/{}", self.group_id, self.function_id, self.value_str)
    }

    /// Entities handled per second, from the mean time and the throughput criterion was given.
    pub fn elements_per_second(&self) -> Option<f64>
    {
        let elements = self.elements? as f64;
        if self.mean.point_estimate <= 0.0
        {
            return None;
        }
        Some(elements / (self.mean.point_estimate * 1e-9))
    }
}

/// Where criterion put its output.
///
/// `CRITERION_HOME` and `CARGO_TARGET_DIR` are the two knobs criterion itself honours, so they are
/// checked first and in that order. Otherwise the workspace target directory is found by walking up
/// from this crate, which is what a plain `cargo bench` at the workspace root produces.
pub fn output_directory() -> Option<PathBuf>
{
    if let Ok(home) = std::env::var("CRITERION_HOME")
    {
        return Some(PathBuf::from(home));
    }
    if let Ok(target) = std::env::var("CARGO_TARGET_DIR")
    {
        return Some(PathBuf::from(target).join("criterion"));
    }

    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(current) = dir
    {
        let candidate = current.join("target").join("criterion");
        if candidate.is_dir()
        {
            return Some(candidate);
        }
        dir = current.parent();
    }
    None
}

/// Loads every benchmark under `root`, keyed by full id.
///
/// Directories without a readable `new/estimates.json` are skipped rather than failing the whole
/// load: criterion keeps a `report` directory alongside the data, and a run filtered with
/// `--bench query -- some_filter` leaves the untouched benchmarks' directories in place.
pub fn load_all(root: &Path) -> HashMap<String, CriterionResult>
{
    let mut results = HashMap::new();
    collect(root, &mut results);
    results
}

fn collect(dir: &Path, out: &mut HashMap<String, CriterionResult>)
{
    let Ok(entries) = fs::read_dir(dir)
    else
    {
        return;
    };

    for entry in entries.flatten()
    {
        let path = entry.path();
        if !path.is_dir()
        {
            continue;
        }
        if path.file_name().is_some_and(|name| name == "new")
        {
            if let Some(result) = load_one(&path)
            {
                out.insert(result.full_id(), result);
            }
            continue;
        }
        collect(&path, out);
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T>
{
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn load_one(new_dir: &Path) -> Option<CriterionResult>
{
    let id: BenchmarkId = read_json(&new_dir.join("benchmark.json"))?;
    let estimates: Estimates = read_json(&new_dir.join("estimates.json"))?;
    let sample: Sample = read_json(&new_dir.join("sample.json"))?;

    // `change/` sits next to `new/`, and only exists once a benchmark has run twice.
    let change: Option<ChangeEstimates> = new_dir.parent().and_then(|base| read_json(&base.join("change").join("estimates.json")));

    let mut per_iter: Vec<f64> = sample
        .times
        .iter()
        .zip(sample.iters.iter())
        .filter(|(_, iters)| **iters > 0.0)
        .map(|(time, iters)| time / iters)
        .collect();
    per_iter.sort_by(|a, b| a.partial_cmp(b).expect("criterion never writes NaN sample times"));

    let total_iters = sample.iters.iter().sum();
    let sample_count = per_iter.len();

    Some(CriterionResult {
        group_id:       id.group_id,
        function_id:    id.function_id.unwrap_or_default(),
        value_str:      id.value_str.unwrap_or_default(),
        elements:       id.throughput.and_then(|t| match t
        {
            Throughput::Elements(n) => Some(n),
            _ => None,
        }),
        sampling_mode:  sample.sampling_mode,
        mean:           estimates.mean,
        median:         estimates.median,
        median_abs_dev: estimates.median_abs_dev,
        std_dev:        estimates.std_dev,
        slope:          estimates.slope,
        min_ns:         first(&per_iter),
        max_ns:         last(&per_iter),
        p95_ns:         percentile_of_sorted(&per_iter, 95.0),
        p99_ns:         percentile_of_sorted(&per_iter, 99.0),
        sample_count:   sample_count,
        total_iters:    total_iters,
        change_mean:    change.as_ref().map(|c| c.mean),
        change_median:  change.as_ref().map(|c| c.median),
    })
}

fn first(sorted: &[f64]) -> f64
{
    sorted.first().copied().unwrap_or(0.0)
}

fn last(sorted: &[f64]) -> f64
{
    sorted.last().copied().unwrap_or(0.0)
}

/// Nearest-rank percentile of an already-sorted slice.
pub fn percentile_of_sorted(sorted: &[f64], p: f64) -> f64
{
    if sorted.is_empty()
    {
        return 0.0;
    }
    let rank = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}
