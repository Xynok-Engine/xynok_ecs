//! Vong nao cham thi ai lam bao nhieu, va nguoi tham gia thu hai bat dau tre bao lau.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use xynok_concurrency::pool::{Config as PoolConfig, ThreadPool};
use xynok_ecs::query::Query;
use xynok_ecs_benches::workload::{ArchetypeLayout, Position, Velocity};
use xynok_ecs_benches::xynok::build_world;

fn main()
{
    let threads: usize = std::env::var("PROBE_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let bpp: usize = std::env::var("PROBE_BPP").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    let entities: usize = std::env::var("PROBE_ENTITIES").ok().and_then(|v| v.parse().ok()).unwrap_or(1_000_000);

    let mut world = Box::new(build_world(entities, ArchetypeLayout::Fragmented5));
    let query: Query<'static, (&'static mut Position, &'static Velocity)> = world.create_query::<(&mut Position, &Velocity)>();

    let pool = ThreadPool::new(PoolConfig {
        threads: threads,
        thread_name: "probe".to_string(),
        ..PoolConfig::default()
    });

    let mut chunk_count = 0usize;
    query.for_each_chunk(|_| chunk_count += 1);
    let participants = pool.worker_count();
    let batch = chunk_count.div_ceil((participants * bpp).max(1)).max(1);
    println!(
        "participants={participants} chunks={chunk_count} batch={batch} lots={}",
        chunk_count.div_ceil(batch)
    );

    let counters: Vec<AtomicUsize> = (0..participants).map(|_| AtomicUsize::new(0)).collect();
    // Thoi diem nguoi tham gia `i` cham chunk dau tien cua vong nay, tinh bang nano giay tu luc bat
    // dau vong. `u64::MAX` la chua bao gio cham.
    let first_touch: Vec<AtomicU64> = (0..participants).map(|_| AtomicU64::new(u64::MAX)).collect();
    let last_touch: Vec<AtomicU64> = (0..participants).map(|_| AtomicU64::new(0)).collect();
    let mut rows: Vec<(f64, Vec<usize>, Vec<u64>)> = Vec::new();

    for _ in 0..400
    {
        for i in 0..participants
        {
            counters[i].store(0, Ordering::Relaxed);
            first_touch[i].store(u64::MAX, Ordering::Relaxed);
            last_touch[i].store(0, Ordering::Relaxed);
        }
        let t = Instant::now();
        query.par_for_each_chunk(&pool, batch, |view| {
            let idx = pool.worker_index();
            if counters[idx].load(Ordering::Relaxed) == 0
            {
                first_touch[idx].store(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
            }
            let (positions, velocities) = view.columns;
            for (p, v) in positions.iter_mut().zip(velocities.iter())
            {
                p.x += v.x;
                p.y += v.y;
            }
            counters[idx].fetch_add(1, Ordering::Relaxed);
            last_touch[idx].store(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
        });
        let ms = t.elapsed().as_secs_f64() * 1e3;
        rows.push((
            ms,
            counters.iter().map(|c| c.load(Ordering::Relaxed)).collect(),
            first_touch
                .iter()
                .zip(last_touch.iter())
                .map(|(f, l)| l.load(Ordering::Relaxed).saturating_sub(f.load(Ordering::Relaxed)))
                .collect(),
        ));
    }

    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let show = |label: &str, slice: &[(f64, Vec<usize>, Vec<u64>)]| {
        println!("{label}:  (first_touch tinh bang us, `-` la khong lam gi ca)");
        for (ms, counts, touch) in slice
        {
            let touch: Vec<String> = touch.iter().map(|t| format!("{:.1}", *t as f64 / 1000.0)).collect();
            println!("  {ms:7.3}ms  chunks={counts:?}  thoi_luong_us={touch:?}");
        }
    };
    show("6 vong nhanh nhat", &rows[..6]);
    let n = rows.len();
    show("6 vong cham nhat", &rows[n - 6..]);
    println!("  p50 = {:.3}ms", rows[n / 2].0);
    println!("counters theo nguoi tham gia:");
    for i in 0..participants
    {
        let c = pool.counters_of(i);
        println!(
            "  #{i:2} jobs={:6} steal_hit={:6} steal_miss={:8} lane_pop={:6} park={:6}",
            c.jobs_run, c.steal_hits, c.steal_misses, c.lane_pops, c.parks
        );
    }
}
