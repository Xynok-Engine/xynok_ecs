//! Job spawn tu host bao lau thi toi tay worker? Host ban hay ranh co doi khong?

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use xynok_concurrency::pool::{Config as PoolConfig, ThreadPool};

fn percentile(sorted: &[f64], p: f64) -> f64
{
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn main()
{
    let threads: usize = std::env::var("PROBE_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let pool = ThreadPool::new(PoolConfig {
        threads: threads,
        thread_name: "probe".to_string(),
        ..PoolConfig::default()
    });
    let participants = pool.worker_count();
    println!("participants={participants}");

    // Bo nho de host quay tren do, dung co de ep ra ngoai cache.
    let mut buffer = vec![0f32; 8 * 1024 * 1024];

    let job_us: u128 = std::env::var("PROBE_JOB_US").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    for host_busy_us in [0u128, 200, 1000]
    {
        let starts: Vec<AtomicU64> = (0..participants).map(|_| AtomicU64::new(u64::MAX)).collect();
        let mut lat: Vec<f64> = Vec::new();
        let mut never = 0usize;
        let mut hist = vec![0usize; threads + 1];

        for _ in 0..400
        {
            for s in &starts
            {
                s.store(u64::MAX, Ordering::Relaxed);
            }
            let t = Instant::now();
            pool.scope(|s| {
                for _ in 0..threads
                {
                    s.spawn(|| {
                        starts[pool.worker_index()].fetch_min(t.elapsed().as_nanos() as u64, Ordering::Relaxed);
                        // Giu cho nguoi lam viec nay ban, dung nhu mot lo cua `parallel_for` that.
                        let t2 = Instant::now();
                        while t2.elapsed().as_micros() < job_us
                        {
                            std::hint::spin_loop();
                        }
                    });
                }
                if host_busy_us > 0
                {
                    let mut i = 0usize;
                    while t.elapsed().as_micros() < host_busy_us
                    {
                        for _ in 0..4096
                        {
                            buffer[i & (8 * 1024 * 1024 - 1)] += 1.0;
                            i += 977;
                        }
                    }
                }
            });

            // Do tre cua nguoi cuoi cung trong so worker (khong tinh o cua host).
            let mut worst = 0f64;
            let mut missing = false;
            for i in 0..threads
            {
                match starts[i].load(Ordering::Relaxed)
                {
                    u64::MAX => missing = true,
                    v => worst = worst.max(v as f64 / 1000.0),
                }
            }
            let ran = (0..threads).filter(|i| starts[*i].load(Ordering::Relaxed) != u64::MAX).count();
            hist[ran] += 1;
            match missing
            {
                true => never += 1,
                false => lat.push(worst),
            }
        }

        println!("  so worker that su chay (0..={threads}): {hist:?}");
        lat.sort_by(f64::total_cmp);
        match lat.is_empty()
        {
            true => println!("host ban {host_busy_us}us: khong vong nao du worker, never={never}/400"),
            false => println!(
                "host ban {host_busy_us:5}us: tre worker cuoi p50={:8.1}us p90={:9.1} p99={:9.1} max={:9.1}  (co worker khong chay: {never}/400)",
                percentile(&lat, 0.5),
                percentile(&lat, 0.9),
                percentile(&lat, 0.99),
                lat[lat.len() - 1]
            ),
        }
    }
    std::hint::black_box(&mut buffer);
}
