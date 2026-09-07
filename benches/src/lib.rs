//! Shared benchmark workload for `xynok_ecs`, `bevy_ecs` and a plain `Vec` baseline.
//!
//! Nothing in this library measures anything. It defines the components, builds the storages and
//! performs the passes. The measuring lives in the targets that use it, and they are split that
//! way on purpose because they answer different questions with different instruments:
//!
//! - `benches/query.rs` is a [criterion] target, run by `cargo bench`. Criterion owns the timing:
//!   warm-up, iteration counts, sample collection, bootstrapped confidence intervals, outlier
//!   classification and the comparison against the previous run.
//! - `benches/parallel.rs` is a second criterion target, and the only one that involves more than
//!   one thread. It times a whole frame of a schedule running a group of non-conflicting systems,
//!   so what it compares is the two schedulers rather than the two query loops. Its workload lives
//!   in [`parallel`] and the plain `Vec` baseline sits it out, having no scheduler to speak of.
//! - `src/bin/report.rs` runs the query workload through a counting global allocator to get the
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
pub mod parallel;
pub mod report;
pub mod stdvec;
pub mod workload;
pub mod xynok;

pub use workload::{ArchetypeLayout, ENTITY_COUNTS, QueryWorkload, count_label};
