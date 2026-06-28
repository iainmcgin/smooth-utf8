# Benchmarks

Per-architecture throughput tables and methodology. The README carries a short summary; this file has the detail.

## Methodology

All numbers are 250-sample criterion medians on dedicated bare-metal AWS instances (one whole physical box; no noisy-neighbour variance), turbo disabled, governor pinned to `performance`, the SMT sibling of the pinned core offlined, background services stopped. Builds use `lto = true`, `codegen-units = 1`, `-Cllvm-args=-align-all-nofallthru-blocks=6` (64-byte alignment for jump-only block targets) and `-align-loops=32` (32-byte alignment for fallthrough-entered loop headers). Both are needed to neutralise the DSB µop-cache layout lottery on Intel: the 28-byte SWAR ASCII loop is small enough that whichever inlined copy's header lands at a 32 B boundary fits in one DSB window and runs ~35% faster than a copy that straddles, and the SWAR loop is fallthrough-entered so the nofallthru flag alone does not align it.

Even with all of that, run-to-run reproducibility is ~±5%; deltas inside that band are not meaningful. Reproduce with `cargo bench --bench throughput`.

The columns:

- `verify` — `smoothutf8::verify` (safe; overlapping in-bounds tail loads).
- `slack` — `unsafe smoothutf8::verify_with_slack` (the eps-copy over-read path; one masked load for sub-8-byte ranges).
- `SlackBuf` — `SlackBuf::verify` (safe; one combined range assert per call on top of `slack`).
- `core::str` — `core::str::from_utf8(b).is_ok()`.
- `simdutf8` — `simdutf8::basic::from_utf8(b).is_ok()`.

## Sapphire Rapids (`c7i.metal-24xl`, default x86-64 build)

ASCII input, ns/call:

| size | `verify` | `slack` | `SlackBuf` | `core::str` | `simdutf8` | slack÷std | slack÷simd |
|--:|--:|--:|--:|--:|--:|--:|--:|
| 1 | 8.77 | 1.63 | 2.20 | 4.00 | 4.04 | 0.41 | 0.40 |
| 2 | 9.70 | 1.61 | 2.19 | 4.55 | 4.54 | 0.35 | 0.36 |
| 4 | 8.77 | 1.64 | 2.19 | 6.80 | 6.55 | 0.24 | 0.25 |
| 8 | 2.17 | 1.37 | 2.02 | 8.18 | 7.95 | 0.17 | 0.17 |
| 16 | 2.63 | 2.03 | 2.60 | 4.75 | 4.52 | 0.43 | 0.45 |
| 32 | 3.09 | 2.64 | 3.03 | 6.72 | 6.24 | 0.39 | 0.42 |
| 64 | 5.51 | 5.04 | 5.51 | 10.05 | 3.45 | 0.50 | 1.46 |
| 128 | 6.83 | 6.39 | 6.81 | 10.52 | 4.54 | 0.61 | 1.41 |

`SlackBuf` − `slack` is a flat ~0.4–0.7 ns at every size: the per-call range assert (`cmp/ja, add, cmp/ja`).

## Graviton4 (`c8g.metal-24xl`, Neoverse-V2, default aarch64 build)

ASCII input, ns/call:

| size | `verify` | `slack` | `SlackBuf` | `core::str` | `simdutf8` | slack÷std | slack÷simd |
|--:|--:|--:|--:|--:|--:|--:|--:|
| 1 | 7.33 | 1.01 | 1.56 | 2.25 | 2.67 | 0.45 | 0.38 |
| 2 | 7.34 | 1.00 | 1.56 | 2.97 | 3.01 | 0.34 | 0.33 |
| 4 | 3.77 | 1.01 | 1.55 | 3.76 | 3.76 | 0.27 | 0.27 |
| 8 | 1.51 | 1.19 | 1.73 | 5.28 | 6.00 | 0.23 | 0.20 |
| 16 | 1.90 | 1.51 | 2.28 | 3.43 | 3.46 | 0.44 | 0.44 |
| 32 | 1.72 | 1.55 | 1.96 | 3.39 | 3.39 | 0.46 | 0.46 |
| 64 | 2.08 | 2.01 | 2.31 | 4.98 | 3.20 | 0.40 | 0.63 |
| 128 | 3.28 | 3.01 | 3.40 | 6.02 | 3.45 | 0.50 | 0.87 |

The aarch64 build uses a 32 B/iter NEON `umaxv` ASCII scan (LLVM lowers it to `ldp q0,q1; orr; umaxv; tbnz #7`). The shift-DFA multibyte path needs no NEON: A64 `lsr` already takes the shift amount mod 64, so LLVM elides the intermediate `& 63` masks in the unrolled loop and the on-chain latency is one cycle per step — the same as BMI2's `shrx` on x86.

## What the curves look like

The shape follows from where the work is and what bounds it at each input size:

- **1–32 B (the short-string regime).** Per-call fixed cost dominates per-byte work. `verify_with_slack` covers a sub-8-byte range with one masked load and has no runtime CPU dispatch. `SlackBuf::verify` adds ~0.7 ns of range-assert overhead. `verify` (safe) covers 2–7 B with an overlapping load pair and 8+ B tails with the last-8-byte window — the stack-copy tail it paid here before 0.2.2 (a libc `memcpy` call at 1–7 B) is gone, so the safe path now tracks the slack path closely on short input. (The numbers in the tables above predate 0.2.2 for the `verify` series and will be refreshed.)
- **32 B – ~32 KiB (L1-resident, compute-bound).** Throughput is set by instructions per byte. The default build's verified scalar loop plateaus alongside stdlib (which auto-vectorizes its ASCII fast path to the same width); a `+simdutf8` build hands off to simdutf8's Keiser–Lemire kernel and matches it.
- **~32 KiB – L3 (cache step-downs).** All implementations slow together; relative ordering is unchanged.
- **Beyond L3 (DRAM-bound).** Throughput is set by memory bandwidth, not the validator. Curves converge towards a common floor.

## Full-sweep plots (0.1.0 — pending refresh)

These plots are from the 0.1.0 release on Sapphire Rapids only. They predate the inline-partition fix in `71e3c11`, which improved `verify_with_slack` by 38–64% at 1–32 B, so the slack curves here understate by roughly 2× at the short end. They will be regenerated for the next release.

![throughput vs input size](throughput.svg)

![speedup vs core::str::from_utf8](throughput-relative.svg)

<sub>ASCII input. `stdlib` is `core::str::from_utf8`; `simdutf8` is `simdutf8::basic::from_utf8`. The `+simdutf8` build is `--features simdutf8` with `-C target-cpu=x86-64-v3`. Raw data is in [`throughput-data.csv`](throughput-data.csv); plots regenerated by `python3 doc/gen-throughput-svg.py`.</sub>
