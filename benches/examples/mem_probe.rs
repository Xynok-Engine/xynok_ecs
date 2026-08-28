//! Nhu `parallel_for` trong ECS, nhung tren mot mang phang: tach anh huong cua ECS ra khoi pool.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use xynok_concurrency::pool::{Config as PoolConfig, ThreadPool};

const CHUNK_F32: usize = 4096; // 16 KiB moi chunk, dung nhu chunk cua ECS.

fn percentile(sorted: &[f64], p: f64) -> f64
{
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn main()
{
    let threads: usize = std::env::var("PROBE_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let chunks: usize = std::env::var("PROBE_CHUNKS").ok().and_then(|v| v.parse().ok()).unwrap_or(1743);
    let bpp: usize = std::env::var("PROBE_BPP").ok().and_then(|v| v.parse().ok()).unwrap_or(1);

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
    let batch = chunks.div_ceil(participants * bpp).max(1);
    println!("participants={participants} chunks={chunks} batch={batch} lots={}", chunks.div_ceil(batch));

    let data: Vec<f32> = vec![1.0; chunks * CHUNK_F32];
    let base = data.as_ptr() as usize;

    let touched: Vec<AtomicUsize> = (0..participants).map(|_| AtomicUsize::new(0)).collect();
    let first: Vec<AtomicU64> = (0..participants).map(|_| AtomicU64::new(u64::MAX)).collect();

    let mut rows: Vec<(f64, Vec<usize>)> = Vec::new();
    let mut seq: Vec<f64> = Vec::new();

    let work = |i: usize| {
        let slice = unsafe { std::slice::from_raw_parts_mut((base as *mut f32).add(i * CHUNK_F32), CHUNK_F32) };
        for v in slice.iter_mut()
        {
            *v += 1.0;
        }
    };

    for _ in 0..200
    {
        let t = Instant::now();
        for i in 0..chunks
        {
            work(i);
        }
        seq.push(t.elapsed().as_secs_f64() * 1e3);
    }
    seq.sort_by(f64::total_cmp);

    for _ in 0..400
    {
        for i in 0..participants
        {
            touched[i].store(0, Ordering::Relaxed);
            first[i].store(u64::MAX, Ordering::Relaxed);
        }
        let t = Instant::now();
        pool.parallel_for(chunks, batch, |i| {
            let idx = pool.worker_index();
            first[idx].fetch_min(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
            touched[idx].fetch_add(1, Ordering::Relaxed);
            work(i);
        });
        rows.push((t.elapsed().as_secs_f64() * 1e3, touched.iter().map(|c| c.load(Ordering::Relaxed)).collect()));
    }

    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = rows.len();
    println!("tuan tu p50 = {:.3}ms", percentile(&seq, 0.5));
    println!(
        "song song p10={:.3} p50={:.3} p90={:.3} p99={:.3} max={:.3}  => speedup p50 = {:.2}x",
        rows[(n as f64 * 0.1) as usize].0,
        rows[n / 2].0,
        rows[(n as f64 * 0.9) as usize].0,
        rows[(n as f64 * 0.99) as usize].0,
        rows[n - 1].0,
        percentile(&seq, 0.5) / rows[n / 2].0
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
