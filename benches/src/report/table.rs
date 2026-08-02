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

fn fmt_ns(ns: u128) -> String
{
    const US: f64 = 1_000.0;
    const MS: f64 = US * 1_000.0;
    let n = ns as f64;
    if n >= MS
    {
        format!("{:.3} ms", n / MS)
    }
    else if n >= US
    {
        format!("{:.2} us", n / US)
    }
    else
    {
        format!("{ns} ns")
    }
}

pub fn print(results: &[BenchResult])
{
    let mut entity_counts: Vec<usize> = results.iter().map(|r| r.entity_count).collect();
    entity_counts.sort_unstable();
    entity_counts.dedup();

    for entity_count in entity_counts
    {
        println!("\n=== {entity_count} entities ===");
        println!(
            "{:<10} | {:>16} | {:>18} | {:>12} | {:>12} | {:>12} | {:>12} | {:>16} | {:>14}",
            "library", "setup alloc", "setup allocations", "query min", "query mean", "query median", "query max", "query-loop alloc", "leaked bytes"
        );
        println!("{}", "-".repeat(140));
        for r in results.iter().filter(|r| r.entity_count == entity_count)
        {
            println!(
                "{:<10} | {:>16} | {:>18} | {:>12} | {:>12} | {:>12} | {:>12} | {:>16} | {:>14}",
                r.library,
                fmt_bytes(r.setup_alloc.bytes),
                r.setup_alloc.allocations,
                fmt_ns(r.query_timing.min_ns),
                fmt_ns(r.query_timing.mean_ns),
                fmt_ns(r.query_timing.median_ns),
                fmt_ns(r.query_timing.max_ns),
                fmt_bytes(r.query_alloc.bytes),
                r.leaked_bytes,
            );
        }
    }
    println!();
}
