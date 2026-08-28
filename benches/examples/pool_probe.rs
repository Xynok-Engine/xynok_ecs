//! Chi phi tho cua fork-join: `parallel_for` voi cong viec rong, va voi cong viec gia lap.

use std::hint::black_box;
use std::time::Instant;

use xynok_concurrency::pool::{Config as PoolConfig, ThreadPool};

fn percentile(sorted: &[f64], p: f64) -> f64
{
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

fn stat(name: &str, mut v: Vec<f64>)
{
    v.sort_by(f64::total_cmp);
    println!(
        "{name:28} p50={:.4}ms p10={:.4} p90={:.4} p99={:.4} max={:.4}",
        percentile(&v, 0.5),
        percentile(&v, 0.1),
        percentile(&v, 0.9),
        percentile(&v, 0.99),
        v[v.len() - 1]
    );
}

fn main()
{
    let threads: usize = std::env::var("PROBE_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(11);
    let pool = ThreadPool::new(PoolConfig {
        threads: threads,
        thread_name: "probe".to_string(),
        ..PoolConfig::default()
    });
    println!("participants={}", pool.worker_count());

    // Fork-join rong: n = so lo, batch = 1 => moi lo mot chi so, khong lam gi.
    for jobs in [12usize, 24, 96]
    {
        let mut v = Vec::new();
        for _ in 0..2000
        {
            let t = Instant::now();
            pool.parallel_for(jobs, 1, |i| {
                black_box(i);
            });
            v.push(t.elapsed().as_secs_f64() * 1e3);
        }
        stat(&format!("rong, {jobs} lo"), v);
    }

    // Cong viec gia lap: moi chi so quay mot vong ban 20us, tong 12 lo.
    let spin = |us: u64| {
        move |_i: usize| {
            let t = Instant::now();
            while t.elapsed().as_micros() < us as u128
            {
                std::hint::spin_loop();
            }
        }
    };
    for us in [20u64, 100]
    {
        let mut v = Vec::new();
        let f = spin(us);
        for _ in 0..300
        {
            let t = Instant::now();
            pool.parallel_for(pool.worker_count(), 1, &f);
            v.push(t.elapsed().as_secs_f64() * 1e3);
        }
        stat(&format!("{us}us moi lo, {} lo", pool.worker_count()), v);
        println!("  (ly tuong = {:.4}ms)", us as f64 / 1000.0);
    }
}
