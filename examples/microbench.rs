#![allow(clippy::cast_precision_loss, clippy::incompatible_msrv)]
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let len: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(65536);
    let iters: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);

    let data = vec![b'x'; len];
    let mut padded = data.clone();
    padded.resize(len + smoothutf8::SLACK, 0xA5);

    for _ in 0..3 {
        // safe
        let t = Instant::now();
        for _ in 0..iters {
            black_box(smoothutf8::verify(black_box(&data[..])));
        }
        let safe_ns = t.elapsed().as_nanos() as f64 / iters as f64;

        // slack
        let t = Instant::now();
        for _ in 0..iters {
            black_box(unsafe { smoothutf8::verify_with_slack(black_box(&padded[..]), 0..len) });
        }
        let slack_ns = t.elapsed().as_nanos() as f64 / iters as f64;

        // std
        let t = Instant::now();
        for _ in 0..iters {
            black_box(core::str::from_utf8(black_box(&data[..])).is_ok());
        }
        let std_ns = t.elapsed().as_nanos() as f64 / iters as f64;

        #[allow(clippy::cast_precision_loss)]
        let g = |ns: f64| len as f64 / ns;
        println!(
            "len={len:>8}  safe={:6.1}ns ({:5.1} GiB/s)  slack={:6.1}ns ({:5.1} GiB/s)  std={:6.1}ns ({:5.1} GiB/s)  safe/slack={:.2}",
            safe_ns, g(safe_ns), slack_ns, g(slack_ns), std_ns, g(std_ns), safe_ns / slack_ns
        );
    }
}
