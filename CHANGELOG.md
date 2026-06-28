# Changelog

## 0.2.2

- Replace the safe path's tail handling. The zero-padded stack-copy tail (`SafeTail`) compiled to a libc `memcpy` call plus a store-to-load-forwarding stall, which dominated `verify`'s latency at 1–7 bytes. Ranges of 8+ bytes now test the trailing 1–15 bytes with one or two unmasked in-bounds 8-byte loads anchored at `end-8` (overlap with already-validated ASCII is free — proven sign bits are zero); ranges under 8 bytes without slack use a pair of overlapping 4- or 2-byte loads. The `Tail` trait, `SafeTail`, and `SlackTail` are deleted; `verify_with_slack` keeps its single masked load for sub-8-byte ranges. The safe path now never reads outside the input slice and never stores to the stack.
- Verus: `verify_short` and the restructured `verify_impl` are fully verified; `SafeTail::load64` (the one `external_body` item with a multi-line body) is gone. New trusted leaves `load32`/`load16` carry the same one-line pack-spec contract as `load64`, and all four raw leaf loads are now RefinedRust-verified (`4 Qed, 0 failed`, was 2). `cargo verus verify` is now 85/0 (was 78/0).
- `verify_impl`/`verify_short` are `unsafe fn` with documented `# Safety` contracts mirroring their Verus `requires`, plus debug assertions.

- Rename the free function `to_str` to `from_utf8`, matching `core::str::from_utf8` and the wider ecosystem (`simdutf8`, `bstr`). `to_str` remains as a deprecated, doc-hidden alias and will be removed in 0.3.0. `SlackBuf::to_str` is unchanged — `to_str` is the conventional name for a borrowing conversion *method* (cf. `CStr::to_str`).
- `SlackBuf`'s inner slice field is now private to its module (was `pub(super)`), so the `len >= SLACK` invariant can only be established through the constructors, even from elsewhere in the crate.

## 0.2.1

- aarch64: 32 B/iter NEON `ascii_skip` (`ldp; orr; umaxv; tbnz`). On Graviton4, `verify_with_slack` ASCII at 32–512 B improves 28–39% over the SWAR path, and the simdutf8 crossover on ASCII moves from ~64 B to ~256 B. The `feature = "simdutf8"` delegation threshold is lowered to 64 B on aarch64 (simdutf8's NEON kernel still wins on multibyte from 64 B).
- Partition inlining: `verify_impl` is now `#[inline(always)]` and `verify_multibyte` is no longer `#[inline]`. With two `SlackTail` call sites in a binary, plain `#[inline]` let LLVM out-line `verify_impl`, and the call boundary then dominated the work on short inputs (~2 ns of prologue/epilogue versus ~1.3 ns of actual validation at 8 B). On Sapphire Rapids, `verify_with_slack` ASCII at 1–32 B improves 38–64%.
- **Correction to 0.2.0's `SlackBuf` measurement:** the "within ±5% reproducibility floor" claim compared two numbers that were both inflated by the ~2 ns call overhead above. With the inline partition fixed, `SlackBuf::verify`'s per-call range assert costs ~0.7 ns (≈+50% at 8 B, ≈+25% at 16 B) relative to `unsafe verify_with_slack`. The asm-predicted absolute cost was right; the relative claim was wrong. `SlackBuf` is the safe path; `verify_with_slack` remains the zero-overhead path.
- Benchmark methodology and current per-architecture tables moved to [`doc/BENCHMARKS.md`](doc/BENCHMARKS.md).

## 0.2.0

- Add `SlackBuf<'a>`: a safe wrapper over the slack-buffer invariant. Construct once per padded buffer (`new_add_slack`, `new_embedded_slack`, `new`, or `unsafe new_unchecked`); per-field `verify` / `to_str` / `le_u32` calls are then safe and skip the per-string tail copy. The per-call cost is one combined range assert (`cmp/ja, add, cmp/ja`), measured within the ±5% reproducibility floor of `unsafe verify_with_slack` on 1–128 B inputs. `verify_with_slack` remains as the underlying zero-overhead entry point.
- New `alloc` feature (default-off) for `SlackBuf::new_add_slack(&mut Vec<u8>)`. Everything else stays no-alloc.
- `wasm32` now runs the shift-DFA multibyte path instead of delegating to `core::str::from_utf8`: wasm has native `i64.shr_u`, so the previous `target_pointer_width = "64"` gate was unnecessarily conservative there.
- Verus: `SlackBuf::verify` carries `ensures ret == is_valid_utf8(self@.subrange(...))`, discharged via `verify_with_slack`'s existing postcondition. `cargo verus verify` is now 78/0 (was 72/0). `to_str`, `le_u32`, `new`, and `new_add_slack` are not yet in the verified surface.

## 0.1.1

- Fix: the `simdutf8` feature now enables `simdutf8/std`, so simdutf8's runtime CPU-feature dispatch is active on the default x86-64 target. In 0.1.0 the dependency was pulled with `default-features = false` and `std` was never re-enabled, so without `-C target-cpu=x86-64-v3` (or another build that sets `target_feature = "avx2"`/`"sse4.2"` at compile time) simdutf8 fell back to `core::str::from_utf8` and the ≥128-byte delegation in `verify`/`verify_with_slack` was silently scalar. Correctness was unaffected.

## 0.1.0

Initial release.

- `verify(&[u8]) -> bool` and `unsafe verify_with_slack(&[u8], Range<usize>) -> bool`.
- `to_str(&[u8]) -> Option<&str>` convenience.
- Optional `simdutf8` feature: delegates inputs ≥128 bytes.
- Compile-time `cfg(target_feature = "avx2")` ASCII prefix scan.
- Verus-verified functional correctness against Unicode §3.9 Table 3-7 (72/0, runs in CI); RefinedRust-checked leaf-load bodies (2 Qed, reproducible via `verify/REFINEDRUST.md`).
- 100% line coverage; differential cargo-fuzz targets vs `core::str::from_utf8`; miri-clean under strict provenance.
- Multibyte path: shift-encoded DFA (Vognsen/Langdale encoding of Höhrmann's UTF-8 automaton). On 32-bit targets the multibyte path delegates to `core::str::from_utf8` and the DFA table is compiled out.
- BMI2 `shrx` (via `-C target-cpu=x86-64-v3`) gives ~+40% on the multibyte path.
