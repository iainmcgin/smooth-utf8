# smoothutf8

[![crates.io](https://img.shields.io/crates/v/smoothutf8.svg)](https://crates.io/crates/smoothutf8)
[![docs.rs](https://img.shields.io/docsrs/smoothutf8)](https://docs.rs/smoothutf8)
[![CI](https://github.com/iainmcgin/smooth-utf8/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/iainmcgin/smooth-utf8/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/crates/msrv/smoothutf8)](Cargo.toml)
[![deps.rs](https://deps.rs/repo/github/iainmcgin/smooth-utf8/status.svg)](https://deps.rs/repo/github/iainmcgin/smooth-utf8)
[![no_std](https://img.shields.io/badge/no__std-compatible-blue)](https://docs.rs/smoothutf8)
[![License](https://img.shields.io/crates/l/smoothutf8)](LICENSE)

> Smooth sailing across input sizes and CPU architectures.

Portable, formally verified UTF-8 validation for Rust — `#![no_std]`, zero-dependency by default, tuned for the short strings typical of serialized protocols. The portable build is mechanically verified for **functional correctness** against Unicode §3.9 Table 3-7; see [Verification](#verification) for what that does and does not cover.

```toml
[dependencies]
smoothutf8 = "0.2"
```

```rust
use smoothutf8::verify;
assert!(verify("hello, 世界! 🌍".as_bytes()));
assert!(!verify(&[0xC0, 0x80])); // overlong NUL
```

On short ASCII inputs (≤32 bytes — the protobuf-field-value regime) even the fully safe `verify` is 2–5× faster than `core::str::from_utf8` and `simdutf8`, and `verify_with_slack` / `SlackBuf::verify` shave the remaining tail-dispatch branches off that. Since 0.2.2 the safe path never copies to the stack — partial chunks are covered by overlapping in-bounds loads — so the unsafe slack path is an increment, not a multiple. On long inputs the default build matches `core::str::from_utf8`, and a `feature = "simdutf8"` build matches `simdutf8`.

| ASCII, ns/call | `verify` (safe) | `verify_with_slack` | `SlackBuf::verify` | `core::str` | `simdutf8` |
|---|--:|--:|--:|--:|--:|
| **4 B** Sapphire Rapids | 1.49 | 1.40 | 1.33 | 7.05 | 7.16 |
| **4 B** Graviton4 | 1.27 | 0.94 | 1.05 | 3.75 | 3.81 |
| **8 B** Sapphire Rapids | 2.23 | 1.37 | 1.31 | 8.23 | 7.96 |
| **8 B** Graviton4 | 1.71 | 1.13 | 1.14 | 5.29 | 5.49 |
| **32 B** Sapphire Rapids | 2.99 | 2.42 | 2.76 | 6.91 | 6.39 |
| **32 B** Graviton4 | 1.88 | 1.50 | 1.79 | 3.39 | 3.42 |
| **128 B** Sapphire Rapids | 6.58 | 5.91 | 5.94 | 10.60 | 5.26 |
| **128 B** Graviton4 | 3.70 | 3.06 | 3.40 | 6.00 | 4.04 |

<sub>Default build (no `target-cpu` override, no `simdutf8` feature on the smoothutf8 columns), 250-sample criterion medians, dedicated bare-metal. See [`doc/BENCHMARKS.md`](doc/BENCHMARKS.md) for methodology, the Graviton4 multibyte path, the full-size sweep plots, and the per-shape table.</sub>

## Choosing an entry point

**`verify(b: &[u8]) -> bool`** — the safe default. Functionally equivalent to `core::str::from_utf8(b).is_ok()`. Use this unless you can guarantee readable bytes after your input. **`from_utf8(b) -> Option<&str>`** is the same check returning the string view on success.

**`SlackBuf<'a>`** — for zero-copy parsers that maintain at least `SLACK` (8) readable bytes after every logical field. The classic example is a protobuf decoder validating string fields inside a larger wire buffer: there is always more data after each field's end (the next field's tag, or the decoder's sentinel padding), so the invariant is free to satisfy. Construct the `SlackBuf` once per buffer; per-field `verify` / `to_str` / `le_u32` calls are then safe and cover any sub-8-byte range with a single masked load instead of the safe path's overlapping load pair.

```rust
use smoothutf8::SlackBuf;

// Transport layer finishes reading the frame into a Vec, then:
let buf = SlackBuf::new_add_slack(&mut wire);  // appends SLACK zero bytes; needs feature = "alloc"
// — or, if you padded yourself / are using BytesMut etc.:
// let buf = SlackBuf::new_embedded_slack(&wire);

// Inside the length-delimited field decoder, per string field:
let field_end = pos + field_len;
let s: &str = buf.to_str(pos..field_end).ok_or(DecodeError::InvalidUtf8)?;
```

The slack path covers a 1–7 byte range with one masked load where `verify` issues an overlapping load pair. `SlackBuf::verify` adds a per-call range assert whose cost is at or below measurement noise (0–0.7 ns); `unsafe verify_with_slack(buf, range)` is the underlying zero-overhead entry point and remains available for callers who hold the range invariant elsewhere.

## Build configurations

Throughput vs `core::str::from_utf8` at 64 KiB (Sapphire Rapids, same setup as the plots above):

| build | ASCII | ~10% non-ASCII | ~30% non-ASCII | 100% non-ASCII | dependencies |
|---|---|---|---|---|---|
| default | 1.0× | 1.6× | 1.3× | 1.1× | none |
| `RUSTFLAGS="-C target-cpu=x86-64-v3"` | 1.7× | 2.2× | 1.8× | 1.6× | none |
| `--features simdutf8` (with `x86-64-v3`) | 2.5× | 5.3× | 9.0× | 7.7× | `simdutf8` |

The `x86-64-v3` baseline (Haswell+, 2013–) enables both the AVX2 ASCII prefix scan and BMI2 `shrx` for the shift-DFA; `-C target-feature=+avx2` alone gets the prefix scan but *not* `shrx` (LLVM treats them as independent features). Use `-C target-cpu=native` if you don't need portability across machines.

The verified path runs unconditionally for inputs <128 bytes in all configurations; the AVX2 prefix scan is `external_body` (see Verification).

### 32-bit targets

On 32-bit targets other than `wasm32` (Cortex-M, i686, riscv32, …) the multibyte path delegates to `core::str::from_utf8` and the 2 KB DFA table is compiled out entirely. The shift-DFA's `(ROW[byte] >> state) & 63` is one instruction on AArch64 and 3–5 on x86-64, but ~10 on i686 (`shrd`/`cmov`) and ~13 on Cortex-M4 (emulated 64-bit shift plus two loads), so the standard library's branchy validator is the better choice there. `wasm32` is opted in explicitly: it has native `i64.shr_u`, so the DFA runs at full speed even though pointers are 32-bit. The SWAR ASCII fast path and the slack-mode tail handling are kept on every target, so short-ASCII inputs still benefit. CI checks `thumbv7em-none-eabihf`, `i686-unknown-linux-gnu`, and `wasm32-unknown-unknown`.

## Verification

The default build (no `simdutf8`, no `target-cpu` override, 64-bit target) is **mechanically verified for functional correctness**: under `--features verus`, both `verify` and `verify_with_slack` carry the postcondition `ret == is_valid_utf8(buf@)`, where [`spec::is_valid_utf8`](src/spec.rs) is a line-by-line transcription of Unicode §3.9 Table 3-7 — a reader can audit it against the standard in five minutes. The SWAR sign-bit ASCII test and the shift-DFA's `(ROW[byte] >> state) & 63` step are each connected to that table by a `by(bit_vector)` lemma, and a 256-cell compile-time check pins `ROW` to `spec_row`; nothing in `spec.rs` is `assume`d or `admit`ted. `SlackBuf::verify` carries the same postcondition (its body delegates to `verify_with_slack`). `cargo verus verify --features verus` reports `84 verified, 0 errors`.

Verus is *not* foundational — it trusts Z3 and its own SMT encoding. The trusted base of the functional-correctness proof is five `external_body` items: the `load64`, `load32`, `load16`, and `load8` leaf loads (`ret == pack64(buf@, at)` etc.; the standard contract for an unaligned read — each a one-line body), and the `row()` table lookup (whose contract `ret == spec_row(byte)` is checked at compile time against a literal transcription of `spec_row` for all 256 inputs — the residual trust is that the literal matches the `spec fn`). Differential testing against `core::str::from_utf8` (table-driven cases, proptest, libfuzzer) cross-checks exactly that trusted base.

Memory safety of the raw-pointer loads — the only `unsafe` on the verified path — is additionally checked by **RefinedRust** (Rocq/Iris, reproducible — see [`verify/REFINEDRUST.md`](verify/REFINEDRUST.md)), which proves the leaf-load *bodies* — `ptr.add(at).cast().read_unaligned()` — sound given separating ownership of the buffer and the `at + N ≤ n` bound. `4 Qed, 0 failed`. This is foundational (machine-checked in Rocq), but rests on an axiomatic spec for `core::ptr::read_unaligned` (`src/raw/shims.rs`) since RefinedRust's stdlib has none.

What is **not** covered by either proof, and is therefore trusted code reviewed and tested in the usual way:

- the `simdutf8`-feature delegation path and the `cfg(avx2)` prefix scan (out of scope by design — third-party / intrinsic code);
- the `core::str::from_utf8` delegation on 32-bit targets (the standard-library implementation itself);
- the bridge step "`&[u8]::as_ptr()` yields a pointer valid for `len()` initialized bytes" (the standard-library slice contract; neither tool models slices and raw pointers in the same proof);
- `from_utf8`'s call to `from_utf8_unchecked`: justified by the functional-correctness proof of `verify`, on the assumption that `spec::is_valid_utf8` coincides with Rust's `str` invariant — both are Unicode §3.9, but neither tool checks that equivalence.

The crate is also miri-clean under strict provenance and has 100% line coverage.

## Minimum supported Rust version

1.60.

The MSRV is the minimum toolchain the library compiles on; we keep it there rather than tracking stable, to maximize compatibility for downstream crates. We will not, however, work around toolchain bugs in releases more than 12 months behind the current stable — if a problem only reproduces on a Rust older than that, the fix is to upgrade Rust. While the crate is pre-1.0, an MSRV bump is a minor (0.x) release and is noted in the CHANGELOG.

Dev-dependencies (criterion, proptest) may require a newer toolchain to run the test and benchmark suites; that does not affect downstream consumers.

## Prior art

The multibyte validator is a shift-encoded DFA: [Björn Höhrmann's UTF-8 automaton](https://bjoern.hoehrmann.de/utf-8/decoder/dfa/), with the per-byte transition packed into a single `u64` per input byte so the state→state' chain is one shift and one mask (the encoding due to Per Vognsen and [Geoff Langdale](https://branchfree.org/2024/06/09/a-draft-paper-on-the-shift-dfa-an-extremely-low-latency-dfa-execution-model-with-state-of-the-art-results-on-random-and-small-dfas/)). The eps-copy slack-buffer pattern that motivates `verify_with_slack` is the one used by [UPB](https://github.com/protocolbuffers/protobuf/tree/main/upb) and [hyperpb](https://github.com/bufbuild/hyperpb-go).

## License

Apache-2.0.
