//! Diagnostic: does simdutf8 throughput depend on the input buffer's address?
//! Runs `simdutf8::basic::from_utf8` on the same logical bytes via two different
//! allocations (`data` vs `padded = clone+resize(+SLACK)`), prints the address,
//! 4 KiB-page offset, and ns/call for each, repeated.
#![allow(clippy::cast_precision_loss, clippy::incompatible_msrv)]

use smoothutf8::SLACK;
use std::hint::black_box;
use std::time::Instant;

fn time(buf: &[u8], iters: u64) -> f64 {
    let t = Instant::now();
    for _ in 0..iters {
        black_box(simdutf8::basic::from_utf8(black_box(buf)).is_ok());
    }
    t.elapsed().as_nanos() as f64 / iters as f64
}

fn main() {
    let len: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(262_144);
    let iters: u64 = 50_000;

    for trial in 0..3 {
        let data = vec![b'x'; len];
        let mut padded = data.clone();
        padded.resize(len + SLACK, 0xA5);
        let s = &padded[..len];

        let a = data.as_ptr() as usize;
        let b = s.as_ptr() as usize;
        let na = time(&data, iters);
        let nb = time(s, iters);
        let g = |ns: f64| len as f64 / ns / 1.073_741_824;
        println!(
            "trial={trial} len={len}\n  data   addr={a:#x} page+{:#05x}  {na:7.1}ns ({:5.1} GiB/s)\n  padded addr={b:#x} page+{:#05x}  {nb:7.1}ns ({:5.1} GiB/s)  ratio={:.2}",
            a & 0xfff, g(na), b & 0xfff, g(nb), na / nb
        );
    }
}
