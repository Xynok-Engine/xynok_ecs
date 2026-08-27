//! What each storage costs in memory, and whether the timed query loop allocates at all.
//!
//! ```bash
//! cargo run --release -p xynok_ecs_benches --bin memory_report
//! ```
//!
//! This is not a benchmark and it reports no timings. Criterion answers "how fast"; bytes are a
//! different question and a stopwatch is the wrong instrument for them, so they live here, measured
//! through a counting global allocator over the same workload `benches/query.rs` times.
//!
//! The table reports, per library and scenario:
//!
//! - **resident**: bytes still held once the storage is built. This is the footprint of storing
//!   those entities, and dividing by the entity count gives the per-entity cost including whatever
//!   the library rounds up to (chunk padding, table capacity growth, index tables).
//! - **allocated**: every byte the storage asked for while being built, including buffers it grew
//!   out of and freed again. Much larger than `resident` means a lot of copying during growth.
//! - **allocations**: how many times the allocator was called to get there.
//!
//! The footprint does not depend on how many components a later query reads, so those columns are
//! reported once per (layout, entity count) rather than once per query arity.
//!
//! Separately, and for every arity, the pass that `benches/query.rs` times is run inside its own
//! measured region. It must allocate nothing. If it does, that benchmark's timings include an
//! allocator call and the comparison is no longer about iteration, so the offenders are listed and
//! the process exits non-zero.
//!
//! Leaks are not checked here. `tests/memory.rs` in the parent crate already does that against
//! `World` directly, with an allocator that counts chunk-sized allocations specifically, which
//! catches more than a byte total ever could.

use std::hint::black_box;
use std::path::Path;

use serde::Serialize;
use xynok_ecs_benches::config::require_release_build;
use xynok_ecs_benches::workload::{ArchetypeLayout, ENTITY_COUNTS, QueryWorkload, count_label};
use xynok_ecs_benches::{alloc_probe, bevy, stdvec, xynok};

#[global_allocator]
static ALLOC_PROBE: alloc_probe::CountingAllocator = alloc_probe::CountingAllocator;

/// Passes to run inside the measured region. More than one so a library that allocates on the first
/// pass only (a lazily built query plan, say) still shows up.
const QUERY_PASSES: usize = 32;
/// Passes run before the measured region, so first-touch page faults and any one-time query setup
/// are not counted as steady-state allocation.
const WARMUP_PASSES: usize = 8;

#[derive(Serialize, Clone, Debug)]
struct Footprint
{
    library:          &'static str,
    archetype_layout: ArchetypeLayout,
    entity_count:     usize,
    /// Bytes still held after the storage is built. See the module notes.
    resident_bytes:   u64,
    /// Every byte requested while building, freed or not.
    allocated_bytes:  u64,
    allocations:      u64,
    /// `resident_bytes / entity_count`, the number worth quoting per entity.
    bytes_per_entity: f64,
}

#[derive(Serialize, Clone, Debug)]
struct QueryLoop
{
    library:          &'static str,
    archetype_layout: ArchetypeLayout,
    entity_count:     usize,
    component_count:  u8,
    /// Bytes allocated across [`QUERY_PASSES`] passes of the timed loop. Expected to be 0.
    bytes:            u64,
    allocations:      u64,
}

#[derive(Serialize)]
struct Report
{
    footprints:  Vec<Footprint>,
    query_loops: Vec<QueryLoop>,
}

fn measure_footprint<W: QueryWorkload>(entity_count: usize, layout: ArchetypeLayout) -> Footprint
{
    let before = alloc_probe::snapshot();
    let storage = W::setup(entity_count, layout);
    let after = alloc_probe::snapshot();

    let built = alloc_probe::delta(&before, &after);
    let resident_bytes = alloc_probe::live_delta(&before, &after).max(0) as u64;

    drop(storage);

    Footprint {
        library:          W::NAME,
        archetype_layout: layout,
        entity_count:     entity_count,
        resident_bytes:   resident_bytes,
        allocated_bytes:  built.bytes,
        allocations:      built.allocations,
        bytes_per_entity: resident_bytes as f64 / entity_count as f64,
    }
}

fn measure_query_loop<W: QueryWorkload>(entity_count: usize, layout: ArchetypeLayout) -> QueryLoop
{
    let mut storage = W::setup(entity_count, layout);
    let mut query = W::prepare_query(&mut storage);

    for _ in 0..WARMUP_PASSES
    {
        W::run_query_once(black_box(&mut storage), black_box(&mut query));
    }

    let before = alloc_probe::snapshot();
    for _ in 0..QUERY_PASSES
    {
        W::run_query_once(black_box(&mut storage), black_box(&mut query));
    }
    let measured = alloc_probe::delta(&before, &alloc_probe::snapshot());

    QueryLoop {
        library:          W::NAME,
        archetype_layout: layout,
        entity_count:     entity_count,
        component_count:  W::COMPONENT_COUNT,
        bytes:            measured.bytes,
        allocations:      measured.allocations,
    }
}

fn fmt_bytes(bytes: u64) -> String
{
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let value = bytes as f64;
    if value >= MB
    {
        format!("{:.2} MB", value / MB)
    }
    else if value >= KB
    {
        format!("{:.2} KB", value / KB)
    }
    else
    {
        format!("{bytes} B")
    }
}

fn print_table(footprints: &[Footprint])
{
    for layout in ArchetypeLayout::ALL
    {
        for &entity_count in &ENTITY_COUNTS
        {
            println!("\n=== {} | {} entities ===", layout.slug(), count_label(entity_count));
            println!(
                "{:<10} | {:>12} | {:>14} | {:>12} | {:>11}",
                "library", "resident", "bytes/entity", "allocated", "allocations"
            );
            println!("{}", "-".repeat(70));

            for footprint in footprints.iter().filter(|f| f.archetype_layout == layout && f.entity_count == entity_count)
            {
                println!(
                    "{:<10} | {:>12} | {:>14.1} | {:>12} | {:>11}",
                    footprint.library,
                    fmt_bytes(footprint.resident_bytes),
                    footprint.bytes_per_entity,
                    fmt_bytes(footprint.allocated_bytes),
                    footprint.allocations,
                );
            }
        }
    }
}

fn main()
{
    require_release_build();

    let mut footprints = Vec::new();
    let mut query_loops = Vec::new();

    for layout in ArchetypeLayout::ALL
    {
        for &entity_count in &ENTITY_COUNTS
        {
            // Arity does not change the storage, so one workload per library is enough here.
            footprints.push(measure_footprint::<xynok::Query1>(entity_count, layout));
            footprints.push(measure_footprint::<bevy::Query1>(entity_count, layout));
            footprints.push(measure_footprint::<stdvec::Query1>(entity_count, layout));

            // Arity does change what the timed loop does, so every one of them gets checked.
            query_loops.push(measure_query_loop::<xynok::Query1>(entity_count, layout));
            query_loops.push(measure_query_loop::<bevy::Query1>(entity_count, layout));
            query_loops.push(measure_query_loop::<stdvec::Query1>(entity_count, layout));
            query_loops.push(measure_query_loop::<xynok::Query2>(entity_count, layout));
            query_loops.push(measure_query_loop::<bevy::Query2>(entity_count, layout));
            query_loops.push(measure_query_loop::<stdvec::Query2>(entity_count, layout));
            query_loops.push(measure_query_loop::<xynok::Query3>(entity_count, layout));
            query_loops.push(measure_query_loop::<bevy::Query3>(entity_count, layout));
            query_loops.push(measure_query_loop::<stdvec::Query3>(entity_count, layout));
        }
    }

    print_table(&footprints);

    let offenders: Vec<&QueryLoop> = query_loops.iter().filter(|q| q.bytes > 0).collect();
    let clean = offenders.is_empty();
    if clean
    {
        println!("\nquery loop: no allocation in any of the {} scenarios checked.", query_loops.len());
    }
    else
    {
        println!(
            "\nquery loop: {} of {} scenarios allocate inside the timed pass:",
            offenders.len(),
            query_loops.len()
        );
        for offender in offenders
        {
            println!(
                "  {} | {} | {} entities | {} component(s) -> {} in {} allocation(s) over {QUERY_PASSES} passes",
                offender.library,
                offender.archetype_layout.slug(),
                count_label(offender.entity_count),
                offender.component_count,
                fmt_bytes(offender.bytes),
                offender.allocations,
            );
        }
        println!("their `benches/query.rs` timings include allocator work and are not iteration-only.");
    }

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("output").join("memory.json");
    if let Some(parent) = path.parent()
    {
        std::fs::create_dir_all(parent).expect("could not create the output directory");
    }
    let report = Report {
        footprints:  footprints,
        query_loops: query_loops,
    };
    let json = serde_json::to_string_pretty(&report).expect("Report must always be serialisable");
    std::fs::write(&path, json).expect("could not write the memory report");
    println!("wrote {}", path.display());

    if !clean
    {
        std::process::exit(1);
    }
}
