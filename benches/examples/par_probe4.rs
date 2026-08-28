//! Cung du lieu ECS, nhung goi thang `pool.parallel_for` thay vi `par_for_each_chunk`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use xynok_concurrency::pool::{Config as PoolConfig, ThreadPool};
use xynok_ecs::query::Query;
use xynok_ecs_benches::workload::{ArchetypeLayout, Position, Velocity};
use xynok_ecs_benches::xynok::build_world;

struct Cols
{
    pos: *mut Position,
    vel: *const Velocity,
    len: usize,
}
unsafe impl Send for Cols {}
unsafe impl Sync for Cols {}

fn main()
{
    let threads: usize = std::env::var("PROBE_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let entities: usize = std::env::var("PROBE_ENTITIES").ok().and_then(|v| v.parse().ok()).unwrap_or(1_000_000);

    let mut world = Box::new(build_world(entities, ArchetypeLayout::Fragmented5));
    let query: Query<'static, (&'static mut Position, &'static Velocity)> = world.create_query::<(&mut Position, &Velocity)>();

    let mut cols: Vec<Cols> = Vec::new();
    query.for_each_chunk(|view| {
        let (p, v) = view.columns;
        cols.push(Cols {
            pos: p.as_mut_ptr(),
            vel: v.as_ptr(),
            len: p.len(),
        });
    });

    let pool = ThreadPool::new(PoolConfig {
        threads: threads,
        thread_name: "probe".to_string(),
        spin_rounds: std::env::var("PROBE_SPIN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(PoolConfig::default().spin_rounds),
        ..PoolConfig::default()
    });
    let participants = pool.worker_count();
    let total = cols.len();
    let batch = total.div_ceil(participants).max(1);
    println!("participants={participants} chunks={total} batch={batch}");

    let touched: Vec<AtomicUsize> = (0..participants).map(|_| AtomicUsize::new(0)).collect();
    let work = |i: usize| {
        let c = &cols[i];
        let p = unsafe { std::slice::from_raw_parts_mut(c.pos, c.len) };
        let v = unsafe { std::slice::from_raw_parts(c.vel, c.len) };
        for (p, v) in p.iter_mut().zip(v.iter())
        {
            p.x += v.x;
            p.y += v.y;
        }
    };

    let mut seq = Vec::new();
    for _ in 0..200
    {
        let t = Instant::now();
        for i in 0..total
        {
            work(i);
        }
        seq.push(t.elapsed().as_secs_f64() * 1e3);
    }
    seq.sort_by(f64::total_cmp);

    let mut rows: Vec<(f64, Vec<usize>)> = Vec::new();
    for _ in 0..400
    {
        for c in &touched
        {
            c.store(0, Ordering::Relaxed);
        }
        let t = Instant::now();
        pool.parallel_for(total, batch, |i| {
            touched[pool.worker_index()].fetch_add(1, Ordering::Relaxed);
            work(i);
        });
        rows.push((t.elapsed().as_secs_f64() * 1e3, touched.iter().map(|c| c.load(Ordering::Relaxed)).collect()));
    }
    // Chuan tren: chia deu bang std::thread, khong qua pool nao ca.
    let mut raw: Vec<f64> = Vec::new();
    let per = total.div_ceil(participants);
    for _ in 0..200
    {
        let t = Instant::now();
        std::thread::scope(|s| {
            for k in 0..participants
            {
                let work = &work;
                s.spawn(move || {
                    for i in (k * per)..((k + 1) * per).min(total)
                    {
                        work(i);
                    }
                });
            }
        });
        raw.push(t.elapsed().as_secs_f64() * 1e3);
    }
    raw.sort_by(f64::total_cmp);
    println!(
        "std::thread chia deu p50 = {:.3}ms  => speedup {:.2}x",
        raw[raw.len() / 2],
        seq[seq.len() / 2] / raw[raw.len() / 2]
    );

    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = rows.len();
    println!("tuan tu p50 = {:.3}ms", seq[seq.len() / 2]);
    println!(
        "song song p10={:.3} p50={:.3} p90={:.3} max={:.3} => speedup {:.2}x",
        rows[n / 10].0,
        rows[n / 2].0,
        rows[n * 9 / 10].0,
        rows[n - 1].0,
        seq[seq.len() / 2] / rows[n / 2].0
    );
    for (label, slice) in [("4 nhanh nhat", &rows[..4]), ("4 cham nhat", &rows[n - 4..])]
    {
        println!("{label}:");
        for (ms, counts) in slice
        {
            println!("  {ms:7.3}ms  {counts:?}");
        }
    }
}
