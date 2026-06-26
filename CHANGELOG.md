# Changelog

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
