//! Bao nhieu thread thuc su chay cung luc, theo tung lop QoS.
//!
//! Moi thread quay dung `SPIN_US` micro giay do bang dong ho tuong (khong phai so vong lap), nen
//! neu N thread that su chay song song thi tong thoi gian van la `SPIN_US`. Neu no thanh boi so cua
//! `SPIN_US` thi cac thread dang thay phien nhau tren it core hon.

use std::sync::{Arc, Barrier};
use std::time::Instant;

const SPIN_US: u128 = 2000;

unsafe extern "C" {
    fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
}

fn spin()
{
    let t = Instant::now();
    while t.elapsed().as_micros() < SPIN_US
    {
        std::hint::spin_loop();
    }
}

fn run(label: &str, qos: Option<u32>, n: usize)
{
    let barrier = Arc::new(Barrier::new(n + 1));
    let mut handles = Vec::new();
    for _ in 0..n
    {
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            if let Some(class) = qos
            {
                unsafe {
                    pthread_set_qos_class_self_np(class, 0);
                }
            }
            barrier.wait();
            spin();
        }));
    }
    barrier.wait();
    let t = Instant::now();
    for h in handles
    {
        h.join().unwrap();
    }
    let ms = t.elapsed().as_secs_f64() * 1e3;
    println!("{label:22} n={n:2}  {ms:.3}ms  = {:.2}x ly tuong", ms / (SPIN_US as f64 / 1000.0));
}

fn main()
{
    const USER_INTERACTIVE: u32 = 0x21;
    const USER_INITIATED: u32 = 0x19;
    const DEFAULT: u32 = 0x15;
    const UTILITY: u32 = 0x11;

    for n in [4usize, 8, 10, 12, 16]
    {
        run("mac dinh (khong dat)", None, n);
        run("USER_INTERACTIVE", Some(USER_INTERACTIVE), n);
        run("USER_INITIATED", Some(USER_INITIATED), n);
        run("DEFAULT", Some(DEFAULT), n);
        run("UTILITY", Some(UTILITY), n);
        println!();
    }
}
