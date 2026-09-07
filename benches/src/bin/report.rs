//! Joins what `cargo bench` measured with the memory numbers criterion has no opinion about, and
//! writes the combined report.
//!
//! ```bash
//! cargo bench -p xynok_ecs_benches --bench query         # single-threaded timings
//! cargo bench -p xynok_ecs_benches --bench parallel      # multi-threaded timings
//! cargo run --release -p xynok_ecs_benches --bin report  # produces the report
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
//! The multi-threaded benchmark gets the same treatment, with one difference that matters. Its
//! frame runs on several threads at once, so per-thread counters would see only the caller's share
//! of it and the process-wide ones are used instead. And unlike the query loop, allocation inside a
//! frame is not a defect: a scheduler that hands work to other threads has jobs and queues to pay
//! for. What the report says about it is the per-frame figure, which is the difference between
//! paying that cost once and paying it every frame.
//!
//! The process exits non-zero if any query scenario allocates in the timed loop or leaks, so this
//! doubles as a check that can run in CI. The parallel rows are reported but never fail the run.

use std::collections::HashMap;
use std::hint::black_box;
use std::path::Path;

use xynok_ecs_benches::config::require_release_build;
use xynok_ecs_benches::criterion_data::{self, CriterionResult};
use xynok_ecs_benches::parallel::{self, PARALLEL_ENTITY_COUNTS, ParallelWorkload, SystemGroup};
use xynok_ecs_benches::report::{
    BenchRow, Environment, FRAME_PASSES, FRAME_WARMUP, FrameMemory, Memory, ParallelRow, QUERY_LOOP_PASSES, QUERY_LOOP_WARMUP, Report, Timing, html, json,
    table,
};
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

/// The world footprint of one parallel scenario, before any frame has run.
///
/// Measured on this thread, like the query benchmark's: building a world is single-threaded on both
/// sides, whatever the schedule does afterwards.
fn measure_parallel_world<W: ParallelWorkload>(entity_count: usize) -> (u64, f64, u64, u64)
{
    let before = alloc_probe::snapshot();
    let world = W::build_world_only(entity_count);
    let after = alloc_probe::snapshot();

    let built = alloc_probe::delta(&before, &after);
    let resident = alloc_probe::live_delta(&before, &after).max(0) as u64;
    drop(world);

    (resident, resident as f64 / entity_count.max(1) as f64, built.bytes, built.allocations)
}

/// Allocation during [`FRAME_PASSES`] frames, across every thread.
///
/// The process-wide counters are the only ones that can see a worker's share of a frame. They are
/// only readable like this because a schedule step joins its workers before it returns, so by the
/// time the second snapshot is taken there is nothing left in flight.
fn measure_frames<W: ParallelWorkload>(entity_count: usize, group: SystemGroup) -> (u64, u64)
{
    let mut runner = W::setup(entity_count, group);

    for _ in 0..FRAME_WARMUP
    {
        W::run_frame(black_box(&mut runner));
    }

    let before = alloc_probe::process_snapshot();
    for _ in 0..FRAME_PASSES
    {
        W::run_frame(black_box(&mut runner));
    }
    let measured = alloc_probe::delta(&before, &alloc_probe::process_snapshot());

    (measured.bytes, measured.allocations)
}

/// Builds one parallel row: the memory half measured here, the timing half looked up by the
/// criterion id `benches/parallel.rs` filed it under.
fn build_parallel_row<W: ParallelWorkload>(entity_count: usize, group: SystemGroup, criterion: &HashMap<String, CriterionResult>) -> ParallelRow
{
    let (resident_bytes, bytes_per_entity, allocated_bytes, allocations) = measure_parallel_world::<W>(entity_count);
    let (frame_bytes, frame_allocations) = measure_frames::<W>(entity_count, group);

    let criterion_id = format!("parallel/{}/{}/{}", group.slug(), W::NAME, count_label(entity_count));
    let entity_visits = entity_count * group.count();

    ParallelRow {
        library:       W::DISPLAY_NAME.to_string(),
        library_id:    W::NAME.to_string(),
        entity_count:  entity_count,
        system_count:  group.count(),
        system_group:  group,
        group_label:   group.label(),
        entity_visits: entity_visits,
        timing:        criterion.get(&criterion_id).map(|result| Timing::from_criterion(result, entity_visits)),
        criterion_id:  criterion_id,
        memory:        FrameMemory {
            resident_bytes:    resident_bytes,
            bytes_per_entity:  bytes_per_entity,
            allocated_bytes:   allocated_bytes,
            allocations:       allocations,
            frame_bytes:       frame_bytes,
            frame_allocations: frame_allocations,
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
        eprintln!("         run `cargo bench -p xynok_ecs_benches --bench query` and `--bench parallel` first.");
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

    let mut parallel_rows = Vec::new();
    for group in SystemGroup::ALL
    {
        for &entity_count in &PARALLEL_ENTITY_COUNTS
        {
            parallel_rows.push(build_parallel_row::<parallel::xynok::ParallelFrame>(entity_count, group, &criterion));
            parallel_rows.push(build_parallel_row::<parallel::bevy::ParallelFrame>(entity_count, group, &criterion));
        }
    }

    let report = Report {
        environment:   Environment::detect(),
        rows:          rows,
        parallel_rows: parallel_rows,
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
