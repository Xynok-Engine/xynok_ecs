//! Dung lai `parallel_for` ngay trong probe, de xem job spawn ra bat dau chay luc nao.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use xynok_concurrency::pool::{Config as PoolConfig, ThreadPool};
use xynok_ecs::query::Query;
use xynok_ecs_benches::workload::{ArchetypeLayout, Position, Velocity};
use xynok_ecs_benches::xynok::build_world;

fn main()
{
    let threads: usize = std::env::var("PROBE_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let entities: usize = std::env::var("PROBE_ENTITIES").ok().and_then(|v| v.parse().ok()).unwrap_or(1_000_000);

    let mut world = Box::new(build_world(entities, ArchetypeLayout::Fragmented5));
    let query: Query<'static, (&'static mut Position, &'static Velocity)> = world.create_query::<(&mut Position, &Velocity)>();

    let pool = ThreadPool::new(PoolConfig {
        threads: threads,
        thread_name: "probe".to_string(),
        ..PoolConfig::default()
    });

    // Danh sach chunk phang, lay mot lan.
    let mut lens: Vec<usize> = Vec::new();
    query.for_each_chunk(|v| lens.push(v.len()));
    let total = lens.len();
    let participants = pool.worker_count();
    let batch = total.div_ceil(participants).max(1);
    let lots = total.div_ceil(batch);
    println!("participants={participants} chunks={total} batch={batch} lots={lots}");

    let touched: Vec<AtomicUsize> = (0..participants).map(|_| AtomicUsize::new(0)).collect();
    // Luc job thu `k` bat dau chay, tinh tu dau vong.
    let job_start: Vec<AtomicU64> = (0..participants).map(|_| AtomicU64::new(u64::MAX)).collect();

    let mut rows = Vec::new();

    for _ in 0..400
    {
        for i in 0..participants
        {
            touched[i].store(0, Ordering::Relaxed);
            job_start[i].store(u64::MAX, Ordering::Relaxed);
        }
        let cursor = AtomicUsize::new(0);
        let t = Instant::now();

        let run = || {
            let idx = pool.worker_index();
            job_start[idx].fetch_min(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
            loop
            {
                let lot = cursor.fetch_add(1, Ordering::Relaxed);
                if lot >= lots
                {
                    break;
                }
                let start = lot * batch;
                for i in start..(start + batch).min(total)
                {
                    // Cong viec gia: dung dung do dai chunk that, chi de ton thoi gian tuong duong.
                    std::hint::black_box(lens[i]);
                    touched[idx].fetch_add(1, Ordering::Relaxed);
                }
            }
        };

        pool.scope(|s| {
            for _ in 0..participants - 1
            {
                s.spawn(run);
            }
            run();
        });

        let us = t.elapsed().as_secs_f64() * 1e6;
        rows.push((
            us,
            touched.iter().map(|c| c.load(Ordering::Relaxed)).collect::<Vec<_>>(),
            job_start.iter().map(|c| c.load(Ordering::Relaxed)).collect::<Vec<_>>(),
        ));
    }

    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let show = |label: &str, slice: &[(f64, Vec<usize>, Vec<u64>)]| {
        println!("{label}:");
        for (us, counts, start) in slice
        {
            let start: Vec<String> = start
                .iter()
                .map(|t| match *t
                {
                    u64::MAX => "-".to_string(),
                    v => format!("{:.1}", v as f64 / 1000.0),
                })
                .collect();
            println!("  {us:9.1}us  chunks={counts:?}  job_bat_dau_us={start:?}");
        }
    };
    show("6 nhanh nhat", &rows[..6]);
    let n = rows.len();
    show("6 cham nhat", &rows[n - 6..]);
    println!("p50 = {:.1}us", rows[n / 2].0);
}
