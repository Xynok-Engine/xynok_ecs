//! Joins what `cargo bench` measured with the memory numbers criterion has no opinion about, and
//! writes the combined report.
//!
//! ```bash
//! cargo bench -p xynok_ecs_benches --bench query      # produces the timings
//! cargo run --release -p xynok_ecs_benches --bin report   # produces the report
//! ```
//!
//! `scripts/bench.sh` runs both in order, which is the usual way in.
//!
//! Nothing here times anything. Criterion answers "how fast", and re-measuring that with a second
//! harness is how a report ends up disagreeing with itself, so the timings are read straight out of
//! `target/criterion` exactly as criterion computed them. Bytes are a different question and a
//! stopwatch is the wrong instrument for them, so this binary measures those itself, through a
//! counting global allocator over the same workload the benchmark times:
//!
//! - **resident**: bytes still held once the storage is built. Divide by the entity count and you
//!   have the per-entity cost, including whatever the library rounds up to (chunk padding, table
//!   capacity growth, index tables).
//! - **allocated**: every byte the storage asked for while being built, including buffers it grew
//!   out of and freed again. Much larger than resident means a lot of copying during growth.
//! - **allocations**: how many times the allocator was called to get there.
//! - **query-loop allocation**: bytes allocated inside the pass criterion times. Must be 0. If it
//!   is not, that benchmark's timing includes allocator work and is no longer about iteration.
//! - **leaked**: bytes still live after the storage was dropped. Must be 0.
//!
//! The storage footprint does not depend on how many components a later query reads, so it is
//! measured once per (library, layout, entity count) and shared across the three arities. The query
//! loop does depend on it, so that one is measured for every arity.
//!
//! The process exits non-zero if any scenario allocates in the timed loop or leaks, so this doubles
//! as a check that can run in CI.

use std::collections::HashMap;
use std::hint::black_box;
use std::path::Path;

use xynok_ecs_benches::config::require_release_build;
use xynok_ecs_benches::criterion_data::{self, CriterionResult};
use xynok_ecs_benches::report::{BenchRow, Environment, Memory, QUERY_LOOP_PASSES, QUERY_LOOP_WARMUP, Report, Timing, html, json, table};
use xynok_ecs_benches::workload::{ArchetypeLayout, COMPONENT_COUNTS, ENTITY_COUNTS, QueryWorkload, count_label};
use xynok_ecs_benches::{alloc_probe, bevy, stdvec, xynok};

#[global_allocator]
static ALLOC_PROBE: alloc_probe::CountingAllocator = alloc_probe::CountingAllocator;

/// The footprint of one storage, which every arity of the same library shares.
#[derive(Clone, Copy, Default)]
struct Footprint
{
    resident_bytes:  u64,
    allocated_bytes: u64,
    allocations:     u64,
    leaked_bytes:    i64,
}

fn measure_footprint<W: QueryWorkload>(entity_count: usize, layout: ArchetypeLayout) -> Footprint
{
    let before = alloc_probe::snapshot();
    let storage = W::setup(entity_count, layout);
    let after_setup = alloc_probe::snapshot();

    let built = alloc_probe::delta(&before, &after_setup);
    let resident_bytes = alloc_probe::live_delta(&before, &after_setup).max(0) as u64;

    drop(storage);
    let after_drop = alloc_probe::snapshot();

    Footprint {
        resident_bytes:  resident_bytes,
        allocated_bytes: built.bytes,
        allocations:     built.allocations,
        leaked_bytes:    alloc_probe::live_delta(&before, &after_drop) as i64,
    }
}

/// Allocation inside the pass criterion times, over [`QUERY_LOOP_PASSES`] passes.
fn measure_query_loop<W: QueryWorkload>(entity_count: usize, layout: ArchetypeLayout) -> (u64, u64)
{
    let mut storage = W::setup(entity_count, layout);
    let mut query = W::prepare_query(&mut storage);

    for _ in 0..QUERY_LOOP_WARMUP
    {
        W::run_query_once(black_box(&mut storage), black_box(&mut query));
    }

    let before = alloc_probe::snapshot();
    for _ in 0..QUERY_LOOP_PASSES
    {
        W::run_query_once(black_box(&mut storage), black_box(&mut query));
    }
    let measured = alloc_probe::delta(&before, &alloc_probe::snapshot());

    (measured.bytes, measured.allocations)
}

/// Builds one row: the memory half measured here, the timing half looked up by the criterion id
/// `benches/query.rs` filed it under.
fn build_row<W: QueryWorkload>(entity_count: usize, layout: ArchetypeLayout, footprint: Footprint, criterion: &HashMap<String, CriterionResult>) -> BenchRow
{
    let (query_loop_bytes, query_loop_allocations) = measure_query_loop::<W>(entity_count, layout);

    let criterion_id = format!(
        "query/{}/{}_components/{}/{}",
        layout.slug(),
        W::COMPONENT_COUNT,
        W::NAME,
        count_label(entity_count)
    );

    BenchRow {
        library:          W::DISPLAY_NAME.to_string(),
        library_id:       W::NAME.to_string(),
        entity_count:     entity_count,
        component_count:  W::COMPONENT_COUNT,
        archetype_layout: layout,
        layout_label:     layout.label().to_string(),
        timing:           criterion.get(&criterion_id).map(|result| Timing::from_criterion(result, entity_count)),
        criterion_id:     criterion_id,
        memory:           Memory {
            resident_bytes:         footprint.resident_bytes,
            bytes_per_entity:       footprint.resident_bytes as f64 / entity_count.max(1) as f64,
            allocated_bytes:        footprint.allocated_bytes,
            allocations:            footprint.allocations,
            query_loop_bytes:       query_loop_bytes,
            query_loop_allocations: query_loop_allocations,
            leaked_bytes:           footprint.leaked_bytes,
        },
    }
}

fn main()
{
    require_release_build();

    let criterion_dir = criterion_data::output_directory();
    let criterion = match &criterion_dir
    {
        Some(dir) => criterion_data::load_all(dir),
        None => HashMap::new(),
    };

    if criterion.is_empty()
    {
        eprintln!("warning: no criterion results found. The report will have memory numbers but no timings.");
        eprintln!("         run `cargo bench -p xynok_ecs_benches --bench query` first.");
    }
    else if let Some(dir) = &criterion_dir
    {
        println!("read {} criterion result(s) from {}", criterion.len(), dir.display());
    }

    let mut rows = Vec::new();
    for layout in ArchetypeLayout::ALL
    {
        for &entity_count in &ENTITY_COUNTS
        {
            // Arity does not change the storage, so the footprint is measured once per library and
            // shared across the three arities below.
            let xynok_footprint = measure_footprint::<xynok::Query1>(entity_count, layout);
            let bevy_footprint = measure_footprint::<bevy::Query1>(entity_count, layout);
            let stdvec_footprint = measure_footprint::<stdvec::Query1>(entity_count, layout);

            for component_count in COMPONENT_COUNTS
            {
                match component_count
                {
                    1 =>
                    {
                        rows.push(build_row::<xynok::Query1>(entity_count, layout, xynok_footprint, &criterion));
                        rows.push(build_row::<bevy::Query1>(entity_count, layout, bevy_footprint, &criterion));
                        rows.push(build_row::<stdvec::Query1>(entity_count, layout, stdvec_footprint, &criterion));
                    }
                    2 =>
                    {
                        rows.push(build_row::<xynok::Query2>(entity_count, layout, xynok_footprint, &criterion));
                        rows.push(build_row::<bevy::Query2>(entity_count, layout, bevy_footprint, &criterion));
                        rows.push(build_row::<stdvec::Query2>(entity_count, layout, stdvec_footprint, &criterion));
                    }
                    _ =>
                    {
                        rows.push(build_row::<xynok::Query3>(entity_count, layout, xynok_footprint, &criterion));
                        rows.push(build_row::<bevy::Query3>(entity_count, layout, bevy_footprint, &criterion));
                        rows.push(build_row::<stdvec::Query3>(entity_count, layout, stdvec_footprint, &criterion));
                    }
                }
            }
        }
    }

    let report = Report {
        environment: Environment::detect(),
        rows:        rows,
    };

    table::print(&report);

    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("output");
    let json_path = out_dir.join("results.json");
    let html_path = out_dir.join("report.html");
    json::write(&report, &json_path).expect("could not write results.json");
    html::write(&report, &html_path).expect("could not write report.html");
    println!("wrote {}", json_path.display());
    println!("wrote {}", html_path.display());

    let unclean = report.rows.iter().any(|row| row.memory.query_loop_bytes > 0 || row.memory.leaked_bytes != 0);
    if unclean
    {
        std::process::exit(1);
    }
}
