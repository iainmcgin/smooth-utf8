//! Throughput benchmark: `smoothutf8::verify` vs `core::str::from_utf8`
//! across a log-2 size gradient from 1 byte to 128 MiB, on three input
//! shapes (all-ASCII, mixed, all-multibyte).
//!
//! Run locally for relative comparison only; for absolute numbers use a
//! quieted dedicated bare-metal host (see the methodology section of
//! `doc/BENCHMARKS.md`).
// MSRV applies to the library; benches run on stable (see README).
#![allow(clippy::incompatible_msrv)]

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use smoothutf8::SLACK;
use std::hint::black_box;
use std::time::Duration;

/// Powers of two from 2^0 to 2^27 = 128 MiB.
const SIZES: &[usize] = &[
    1,
    2,
    4,
    8,
    16,
    32,
    64,
    128,
    256,
    512,
    1 << 10,
    1 << 11,
    1 << 12,
    1 << 13,
    1 << 14,
    1 << 15,
    1 << 16,
    1 << 17,
    1 << 18,
    1 << 19,
    1 << 20,
    1 << 21,
    1 << 22,
    1 << 23,
    1 << 24,
    1 << 25,
    1 << 26,
    1 << 27,
    1 << 28,
    1 << 29,
];

/// Input shapes spanning the non-ASCII gradient:
/// - `ascii`: 0% non-ASCII; pure fast-path
/// - `sparse`: ~10% non-ASCII bytes (one 2-byte rune per 18 ASCII)
/// - `mixed`: ~30% non-ASCII bytes (one 3-byte rune per 7 ASCII)
/// - `multibyte`: 100% non-ASCII; back-to-back 2-byte runes
fn corpus(shape: &str, len: usize) -> Vec<u8> {
    let unit: &[u8] = match shape {
        "ascii" => b"aBcDeFg0",
        "sparse" => "the-quick-brown-fo\u{00E9}".as_bytes(),
        "mixed" => "abc-123\u{20AC}".as_bytes(),
        "multibyte" => "\u{00E9}".as_bytes(),
        _ => unreachable!(),
    };
    // Tile `unit` to at least `len`, then truncate to the largest prefix that
    // is itself valid UTF-8 (so the std comparator returns `true` and both
    // implementations walk the full input).
    let mut v = unit.repeat(len / unit.len() + 1);
    v.truncate(len);
    while !v.is_empty() && core::str::from_utf8(&v).is_err() {
        v.pop();
    }
    v
}

fn bench_shape(c: &mut Criterion, shape: &'static str) {
    let mut g = c.benchmark_group(format!("verify/{shape}"));
    g.warm_up_time(Duration::from_secs(1));
    g.measurement_time(Duration::from_secs(10));
    g.sample_size(250);

    for &len in SIZES {
        // One allocation shared by every implementation: simdutf8's throughput
        // depends on the buffer's page offset by ~1.6× (see
        // examples/align_probe.rs), so comparing across different allocations
        // measures the address, not the code. The buffer carries `SLACK`
        // trailing bytes so `verify_with_slack` can read it directly; the
        // other impls take `&buf[..end]` over the same logical bytes.
        let mut buf = corpus(shape, len);
        let end = buf.len();
        if end == 0 {
            continue;
        }
        buf.resize(end + SLACK, 0xA5);
        let n = end as u64;
        g.throughput(Throughput::Bytes(n));

        g.bench_with_input(
            BenchmarkId::new("smoothutf8", n),
            &(&buf, end),
            |b, (p, e)| {
                b.iter(|| black_box(smoothutf8::verify(black_box(&p[..*e]))));
            },
        );

        g.bench_with_input(
            BenchmarkId::new("simdutf8", n),
            &(&buf, end),
            |b, (p, e)| {
                b.iter(|| black_box(simdutf8::basic::from_utf8(black_box(&p[..*e])).is_ok()));
            },
        );

        g.bench_with_input(
            BenchmarkId::new("std_from_utf8", n),
            &(&buf, end),
            |b, (p, e)| {
                b.iter(|| black_box(core::str::from_utf8(black_box(&p[..*e])).is_ok()));
            },
        );

        g.bench_with_input(
            BenchmarkId::new("smoothutf8_slack", n),
            &(&buf, end),
            |b, (p, e)| {
                b.iter(|| {
                    // SAFETY: `0 <= e` and `e + SLACK == p.len()` by construction.
                    black_box(unsafe { smoothutf8::verify_with_slack(black_box(&p[..]), 0..*e) })
                });
            },
        );

        // Safe SlackBuf::verify. `sb` is constructed once per buffer
        // (modelling once-per-message); the per-iter call measures the
        // range-check overhead vs `smoothutf8_slack` above.
        g.bench_with_input(
            BenchmarkId::new("smoothutf8_slackbuf", n),
            &(&buf, end),
            |b, (p, e)| {
                let sb = smoothutf8::SlackBuf::new(p).unwrap();
                b.iter(|| black_box(black_box(sb).verify(0..*e)));
            },
        );
    }
    g.finish();
}

fn benches(c: &mut Criterion) {
    for shape in ["ascii", "sparse", "mixed", "multibyte"] {
        bench_shape(c, shape);
    }
}

criterion_group!(throughput, benches);
criterion_main!(throughput);
