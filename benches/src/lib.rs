//! Shared benchmark workload for `xynok_ecs`, `bevy_ecs` and a plain `Vec` baseline.
//!
//! Nothing in this library measures anything. It defines the components, builds the storages and
//! performs the passes. The measuring lives in the two targets that use it, and they are split
//! that way on purpose because they answer two different questions with two different instruments:
//!
//! - `benches/query.rs` is a [criterion] target, run by `cargo bench`. Criterion owns the timing:
//!   warm-up, iteration counts, sample collection, bootstrapped confidence intervals, outlier
//!   classification and the comparison against the previous run.
//! - `src/bin/report.rs` runs the same workload through a counting global allocator to get the
//!   memory numbers, reads back what criterion measured, and writes the combined report.
//!
//! Keeping the workload in one library rather than copying it into each target is what makes the
//! numbers comparable: there is exactly one definition of what "the same work" means.
//!
//! [criterion]: https://github.com/criterion-rs/criterion.rs

pub mod alloc_probe;
pub mod bevy;
pub mod config;
pub mod criterion_data;
pub mod report;
pub mod stdvec;
pub mod workload;
pub mod xynok;

pub use workload::{ArchetypeLayout, ENTITY_COUNTS, QueryWorkload, count_label};
