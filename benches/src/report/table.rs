use super::BenchResult;

fn fmt_bytes(bytes: u64) -> String
{
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB
    {
        format!("{:.2} MB", b / MB)
    }
    else if b >= KB
    {
        format!("{:.2} KB", b / KB)
    }
    else
    {
        format!("{bytes} B")
    }
}

fn fmt_ns(ns: f64) -> String
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

pub fn print(results: &[BenchResult])
{
    let mut scenarios: Vec<(usize, u8, &'static str)> = results
        .iter()
        .map(|r| (r.entity_count, r.component_count, r.archetype_layout.label()))
        .collect();
    scenarios.sort_unstable();
    scenarios.dedup();

    for (entity_count, component_count, layout_label) in scenarios
    {
        println!("\n=== {entity_count} entities | {component_count} component(s) | {layout_label} ===");
        println!(
            "{:<10} | {:>16} | {:>18} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>16} | {:>14}",
            "library",
            "setup alloc",
            "setup allocations",
            "min",
            "mean",
            "median",
            "p95",
            "p99",
            "max",
            "stddev",
            "query-loop alloc",
            "leaked bytes",
        );
        println!("{}", "-".repeat(170));
        for r in results
            .iter()
            .filter(|r| r.entity_count == entity_count && r.component_count == component_count && r.archetype_layout.label() == layout_label)
        {
            println!(
                "{:<10} | {:>16} | {:>18} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>10} | {:>16} | {:>14}",
                r.library,
                fmt_bytes(r.setup_alloc.bytes),
                r.setup_alloc.allocations,
                fmt_ns(r.query_timing.min_ns),
                fmt_ns(r.query_timing.mean_ns),
                fmt_ns(r.query_timing.median_ns),
                fmt_ns(r.query_timing.p95_ns),
                fmt_ns(r.query_timing.p99_ns),
                fmt_ns(r.query_timing.max_ns),
                fmt_ns(r.query_timing.stddev_ns),
                fmt_bytes(r.query_alloc.bytes),
                r.leaked_bytes,
            );
        }
    }
    println!();
}
