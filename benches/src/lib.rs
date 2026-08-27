//! Shared benchmark workload for `xynok_ecs`, `bevy_ecs` and a plain `Vec` baseline.
//!
//! Nothing here measures anything. It defines the components, builds the storages and performs the
//! passes; the measuring lives in the targets that use it:
//!
//! - `benches/query.rs`     single-threaded query iteration, all three competitors, run by criterion
//! - `benches/parallel.rs`  the same pass spread across a thread pool, `xynok_ecs` against `bevy_ecs`
//! - `src/bin/memory_report.rs`  how many bytes each storage costs, and whether the timed loop
//!   allocates at all
//!
//! Keeping the workload in one library rather than copying it into each target is what makes the
//! numbers comparable: there is exactly one definition of what "the same work" means.

pub mod alloc_probe;
pub mod bevy;
pub mod config;
pub mod stdvec;
pub mod workload;
pub mod xynok;

pub use workload::{ArchetypeLayout, ENTITY_COUNTS, PARALLEL_ENTITY_COUNTS, ParallelWorkload, QueryWorkload, count_label};
