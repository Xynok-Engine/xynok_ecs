//! Tran bang thong that su cua may cho dung workload nay, tren ba cach xep bo nho khac nhau.

use std::time::Instant;

use xynok_concurrency::pool::{Config as PoolConfig, ThreadPool};
use xynok_ecs::query::Query;
use xynok_ecs_benches::workload::{ArchetypeLayout, Position, Velocity};
use xynok_ecs_benches::xynok::build_world;

fn med(mut v: Vec<f64>) -> f64
{
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

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
    let n: usize = std::env::var("PROBE_N").ok().and_then(|v| v.parse().ok()).unwrap_or(1_000_000);
    let threads: usize = std::env::var("PROBE_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(11);
    let pool = ThreadPool::new(PoolConfig {
        threads: threads,
        thread_name: "probe".to_string(),
        spin_rounds: 11,
        ..PoolConfig::default()
    });
    let participants = pool.worker_count();
    println!("participants={participants}, 1M entity");

    // 1. Hai mang phang, moi component mot mang lien tuc.
    let mut pos: Vec<Position> = (0..n).map(|i| Position { x: i as f32, y: 0.0 }).collect();
    let vel: Vec<Velocity> = (0..n).map(|_| Velocity { x: 1.0, y: -1.0 }).collect();
    let pos_ptr = pos.as_mut_ptr() as usize;
    let vel_ptr = vel.as_ptr() as usize;

    let flat_batch = 4096usize; // 4096 entity moi lo, ~32 KiB
    let lots = n.div_ceil(flat_batch);
    let mut v = Vec::new();
    for _ in 0..300
    {
        let t = Instant::now();
        pool.parallel_for(lots, lots.div_ceil(participants * 8).max(1), |lot| {
            let start = lot * flat_batch;
            let len = flat_batch.min(n - start);
            let p = unsafe { std::slice::from_raw_parts_mut((pos_ptr as *mut Position).add(start), len) };
            let q = unsafe { std::slice::from_raw_parts((vel_ptr as *const Velocity).add(start), len) };
            for (p, q) in p.iter_mut().zip(q.iter())
            {
                p.x += q.x;
                p.y += q.y;
            }
        });
        v.push(t.elapsed().as_secs_f64() * 1e3);
    }
    println!("mang phang, song song : {:.3}ms", med(v));

    let mut v = Vec::new();
    for _ in 0..200
    {
        let t = Instant::now();
        for (p, q) in pos.iter_mut().zip(vel.iter())
        {
            p.x += q.x;
            p.y += q.y;
        }
        v.push(t.elapsed().as_secs_f64() * 1e3);
    }
    println!("mang phang, tuan tu   : {:.3}ms  ({:.3} ns/entity)", med(v.clone()), med(v) * 1e6 / n as f64);

    // 1b. Van mang phang, nhung cat thanh doan 577 entity dung nhu chunk ECS: tach "chia nho vong
    // lap" ra khoi "cap phat roi rac".
    let seg = 577usize;
    let segs = n.div_ceil(seg);
    let mut v = Vec::new();
    for _ in 0..300
    {
        let t = Instant::now();
        pool.parallel_for(segs, segs.div_ceil(participants * 8).max(1), |k| {
            let start = k * seg;
            let len = seg.min(n - start);
            let p = unsafe { std::slice::from_raw_parts_mut((pos_ptr as *mut Position).add(start), len) };
            let q = unsafe { std::slice::from_raw_parts((vel_ptr as *const Velocity).add(start), len) };
            for (p, q) in p.iter_mut().zip(q.iter())
            {
                p.x += q.x;
                p.y += q.y;
            }
        });
        v.push(t.elapsed().as_secs_f64() * 1e3);
    }
    println!("mang phang cat 577    : {:.3}ms (song song)", med(v));

    // 2. Chunk 16 KiB cua ECS.
    let mut world = Box::new(build_world(n, ArchetypeLayout::Fragmented5));
    let query: Query<'static, (&'static mut Position, &'static Velocity)> = world.create_query::<(&mut Position, &Velocity)>();
    let mut cols: Vec<Cols> = Vec::new();
    query.for_each_chunk(|view| {
        let (p, q) = view.columns;
        cols.push(Cols {
            pos: p.as_mut_ptr(),
            vel: q.as_ptr(),
            len: p.len(),
        });
    });
    println!("so chunk = {}, {} entity moi chunk", cols.len(), cols[0].len);
    {
        let mut addrs: Vec<usize> = cols.iter().map(|c| c.pos as usize).collect();
        let lo = *addrs.iter().min().unwrap();
        let hi = *addrs.iter().max().unwrap();
        println!(
            "  vung dia chi cot Position: {:.1} MiB cho {} chunk",
            (hi - lo) as f64 / (1024.0 * 1024.0),
            addrs.len()
        );
        addrs.sort();
        let mut deltas: Vec<usize> = addrs.windows(2).map(|w| w[1] - w[0]).collect();
        deltas.sort();
        println!(
            "  khoang cach giua hai chunk ke nhau: p50={} p90={} max={}",
            deltas[deltas.len() / 2],
            deltas[deltas.len() * 9 / 10],
            deltas[deltas.len() - 1]
        );
        println!(
            "  khoang cach Position -> Velocity trong mot chunk: {}",
            cols[0].vel as usize - cols[0].pos as usize
        );
    }

    let work = |i: usize| {
        let c = &cols[i];
        let p = unsafe { std::slice::from_raw_parts_mut(c.pos, c.len) };
        let q = unsafe { std::slice::from_raw_parts(c.vel, c.len) };
        for (p, q) in p.iter_mut().zip(q.iter())
        {
            p.x += q.x;
            p.y += q.y;
        }
    };
    let total = cols.len();
    let mut v = Vec::new();
    for _ in 0..300
    {
        let t = Instant::now();
        pool.parallel_for(total, total.div_ceil(participants * 8).max(1), &work);
        v.push(t.elapsed().as_secs_f64() * 1e3);
    }
    println!("chunk ECS, song song  : {:.3}ms", med(v));

    let mut v = Vec::new();
    for _ in 0..200
    {
        let t = Instant::now();
        for i in 0..total
        {
            work(i);
        }
        v.push(t.elapsed().as_secs_f64() * 1e3);
    }
    println!("chunk ECS, tuan tu    : {:.3}ms  ({:.3} ns/entity)", med(v.clone()), med(v) * 1e6 / n as f64);
}
