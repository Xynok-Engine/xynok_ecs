//! Do do lech tai (straggler) va chi phi fork-join cua `par_for_each_chunk`.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use xynok_concurrency::pool::{Config as PoolConfig, ThreadPool};
use xynok_ecs::query::Query;
use xynok_ecs::world::World;
use xynok_ecs_benches::workload::{ArchetypeLayout, Position, Velocity};
use xynok_ecs_benches::xynok::build_world;

fn percentile(sorted: &[f64], p: f64) -> f64
{
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

fn main()
{
    let threads: usize = std::env::var("PROBE_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(11);
    let bpp: usize = std::env::var("PROBE_BPP").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let entities: usize = std::env::var("PROBE_ENTITIES").ok().and_then(|v| v.parse().ok()).unwrap_or(1_000_000);
    let layout = match std::env::var("PROBE_LAYOUT").as_deref()
    {
        Ok("1") => ArchetypeLayout::Single,
        _ => ArchetypeLayout::Fragmented5,
    };

    let mut world = Box::new(build_world(entities, layout));
    let query: Query<'static, (&'static mut Position, &'static Velocity)> = world.create_query::<(&mut Position, &Velocity)>();

    let pool = ThreadPool::new(PoolConfig {
        threads: threads,
        thread_name: "probe".to_string(),
        ..PoolConfig::default()
    });

    let mut chunk_count = 0usize;
    query.for_each_chunk(|_| chunk_count += 1);
    let participants = pool.worker_count();
    let jobs = participants * bpp;
    let batch = chunk_count.div_ceil(jobs.max(1)).max(1);

    println!(
        "threads={threads} participants={participants} bpp={bpp} entities={entities} layout={:?}",
        layout
    );
    println!("chunks={chunk_count} batch={batch} => {} lo", chunk_count.div_ceil(batch));

    // Chuan tuan tu.
    let mut seq = Vec::new();
    for _ in 0..200
    {
        let t = Instant::now();
        query.for_each_chunk(|view| {
            let (positions, velocities) = view.columns;
            for (p, v) in positions.iter_mut().zip(velocities.iter())
            {
                p.x += v.x;
                p.y += v.y;
            }
        });
        seq.push(t.elapsed().as_secs_f64() * 1e3);
    }
    seq.sort_by(f64::total_cmp);
    println!(
        "sequential: p50={:.3}ms p10={:.3} p90={:.3}",
        percentile(&seq, 0.5),
        percentile(&seq, 0.1),
        percentile(&seq, 0.9)
    );

    // Dem chunk theo tung nguoi tham gia, va thoi diem ket thuc cua tung nguoi.
    let per_worker: Vec<AtomicUsize> = (0..participants).map(|_| AtomicUsize::new(0)).collect();
    let last_end: Vec<AtomicU64> = (0..participants).map(|_| AtomicU64::new(0)).collect();

    let mut par = Vec::new();
    let origin = Instant::now();
    for iter in 0..500
    {
        let record = iter >= 400;
        let t = Instant::now();
        query.par_for_each_chunk(&pool, batch, |view| {
            let (positions, velocities) = view.columns;
            for (p, v) in positions.iter_mut().zip(velocities.iter())
            {
                p.x += v.x;
                p.y += v.y;
            }
            if record
            {
                let idx = pool.worker_index();
                per_worker[idx].fetch_add(1, Ordering::Relaxed);
                last_end[idx].store(origin.elapsed().as_nanos() as u64, Ordering::Relaxed);
            }
        });
        par.push(t.elapsed().as_secs_f64() * 1e3);
    }
    par.sort_by(f64::total_cmp);
    println!(
        "parallel:   p50={:.3}ms p10={:.3} p90={:.3} p99={:.3} min={:.3} max={:.3}",
        percentile(&par, 0.5),
        percentile(&par, 0.1),
        percentile(&par, 0.9),
        percentile(&par, 0.99),
        par[0],
        par[par.len() - 1]
    );
    println!("speedup p50 = {:.2}x", percentile(&seq, 0.5) / percentile(&par, 0.5));

    let counts: Vec<usize> = per_worker.iter().map(|c| c.load(Ordering::Relaxed)).collect();
    let total: usize = counts.iter().sum();
    println!(
        "chunk theo nguoi tham gia (100 vong cuoi, tong={total}, ky vong moi nguoi={}):",
        total / participants
    );
    for (i, c) in counts.iter().enumerate()
    {
        println!("  #{i:2} {c:7}  {:.1}%", *c as f64 / total as f64 * 100.0);
    }
}
