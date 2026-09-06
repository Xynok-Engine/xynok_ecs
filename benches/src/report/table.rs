//! The terminal view of the report: one comparison table per scenario, plus the two checks that
//! have a right answer rather than a ranking.

use super::{BenchRow, Report};

pub fn fmt_bytes(bytes: u64) -> String
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

pub fn fmt_ns(ns: f64) -> String
{
    const US: f64 = 1_000.0;
    const MS: f64 = US * 1_000.0;
    if ns >= MS
    {
        format!("{:.3} ms", ns / MS)
    }
    else if ns >= US
    {
        format!("{:.2} us", ns / US)
    }
    else
    {
        format!("{ns:.0} ns")
    }
}

fn fmt_throughput(elements_per_second: Option<f64>) -> String
{
    match elements_per_second
    {
        Some(eps) => format!("{:.1} Melem/s", eps / 1e6),
        None => "-".to_string(),
    }
}

/// How much slower this row is than the fastest row in its scenario. `1.00x` is the winner.
fn fmt_ratio(row: &BenchRow, fastest_ns: f64) -> String
{
    match &row.timing
    {
        Some(timing) if fastest_ns > 0.0 => format!("{:.2}x", timing.mean.point / fastest_ns),
        _ => "-".to_string(),
    }
}

fn fmt_change(row: &BenchRow) -> String
{
    match row.timing.as_ref().and_then(|t| t.change_mean)
    {
        Some(change) => format!("{:+.1}%", change.point * 100.0),
        None => "-".to_string(),
    }
}

pub fn print(report: &Report)
{
    let mut scenarios: Vec<(usize, u8, &str)> = report
        .rows
        .iter()
        .map(|row| (row.entity_count, row.component_count, row.archetype_layout.label()))
        .collect();
    scenarios.sort_unstable();
    scenarios.dedup();

    for (entity_count, component_count, layout_label) in scenarios
    {
        let rows: Vec<&BenchRow> = report
            .rows
            .iter()
            .filter(|row| row.entity_count == entity_count && row.component_count == component_count && row.archetype_layout.label() == layout_label)
            .collect();

        // The scale every ratio in this block is relative to. Only rows that actually have a
        // timing take part, so a partially benchmarked run still ranks the rows it has.
        let fastest_ns = rows
            .iter()
            .filter_map(|row| row.timing.as_ref().map(|t| t.mean.point))
            .fold(f64::INFINITY, f64::min);

        println!("\n=== {entity_count} entities | {component_count} component(s) | {layout_label} ===");
        println!(
            "{:<10} | {:>10} | {:>21} | {:>10} | {:>10} | {:>10} | {:>7} | {:>14} | {:>8} | {:>12} | {:>12} | {:>10}",
            "library", "mean", "95% CI", "median", "p95", "p99", "vs best", "throughput", "change", "resident", "alloc calls", "loop alloc",
        );
        println!("{}", "-".repeat(160));

        for row in &rows
        {
            let (mean, ci, median, p95, p99) = match &row.timing
            {
                Some(timing) => (
                    fmt_ns(timing.mean.point),
                    format!("[{} .. {}]", fmt_ns(timing.mean.lower), fmt_ns(timing.mean.upper)),
                    fmt_ns(timing.median.point),
                    fmt_ns(timing.p95_ns),
                    fmt_ns(timing.p99_ns),
                ),
                None => ("not benched".to_string(), "-".to_string(), "-".to_string(), "-".to_string(), "-".to_string()),
            };

            println!(
                "{:<10} | {:>10} | {:>21} | {:>10} | {:>10} | {:>10} | {:>7} | {:>14} | {:>8} | {:>12} | {:>12} | {:>10}",
                row.library,
                mean,
                ci,
                median,
                p95,
                p99,
                fmt_ratio(row, fastest_ns),
                fmt_throughput(row.timing.as_ref().and_then(|t| t.elements_per_second)),
                fmt_change(row),
                fmt_bytes(row.memory.resident_bytes),
                row.memory.allocations,
                fmt_bytes(row.memory.query_loop_bytes),
            );
        }
    }

    print_checks(report);
}

/// The two columns above that are not a comparison. A library either allocates in the timed loop
/// or it does not, and it either leaks or it does not, so they get called out rather than left for
/// someone to spot in a wide table.
fn print_checks(report: &Report)
{
    let allocating: Vec<&BenchRow> = report.rows.iter().filter(|row| row.memory.query_loop_bytes > 0).collect();
    let leaking: Vec<&BenchRow> = report.rows.iter().filter(|row| row.memory.leaked_bytes != 0).collect();

    println!();
    if allocating.is_empty()
    {
        println!("query loop: no allocation in any of the {} scenarios checked.", report.rows.len());
    }
    else
    {
        println!(
            "query loop: {} of {} scenarios allocate inside the timed pass:",
            allocating.len(),
            report.rows.len()
        );
        for row in allocating
        {
            println!(
                "  {} | {} | {} entities | {} component(s) -> {} in {} allocation(s) over {} passes",
                row.library,
                row.layout_label,
                row.entity_count,
                row.component_count,
                fmt_bytes(row.memory.query_loop_bytes),
                row.memory.query_loop_allocations,
                report.environment.query_loop_passes,
            );
        }
        println!("  their criterion timings include allocator work and are not iteration-only.");
    }

    if leaking.is_empty()
    {
        println!("leaks: every storage freed everything it took.");
    }
    else
    {
        println!("leaks: {} scenario(s) still hold memory after the storage was dropped:", leaking.len());
        for row in leaking
        {
            println!(
                "  {} | {} | {} entities -> {} bytes still live",
                row.library, row.layout_label, row.entity_count, row.memory.leaked_bytes
            );
        }
    }

    let missing = report.rows.iter().filter(|row| row.timing.is_none()).count();
    if missing > 0
    {
        println!("timings: {missing} scenario(s) have no criterion data. Run `cargo bench -p xynok_ecs_benches --bench query` for a full report.");
    }
    println!();
}
