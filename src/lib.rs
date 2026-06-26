//! Portable, formally verified UTF-8 validation.
//!
//! This crate provides two entry points for validating that a byte sequence is
//! well-formed UTF-8:
//!
//! - [`verify`] takes a plain `&[u8]` and is fully safe. It uses an 8-byte
//!   SWAR (SIMD-within-a-register) ASCII fast path, and handles the final
//!   partial chunk with an in-bounds stack copy so that it never reads past
//!   the slice.
//!
//! - [`verify_with_slack`] is an `unsafe` variant for zero-copy parsers that
//!   maintain at least [`SLACK`] readable bytes after every logical field
//!   (the "eps-copy" pattern used by hyperpb and UPB). It reads up to
//!   `SLACK - 1` bytes past the logical end and masks them off, which removes
//!   the per-string tail-handling cost.
//!
//! Both share the same multi-byte path: a shift-encoded DFA (the
//! Vognsen/Langdale encoding of Höhrmann's UTF-8 automaton).
//!
//! The crate is `#![no_std]` and zero-dependency by default.
//!
//! # Features
//!
//! - **`simdutf8`** — delegate inputs ≥ 128 bytes to
//!   [`simdutf8::basic::from_utf8`](https://docs.rs/simdutf8). Adds one
//!   dependency. Below the threshold the verified path runs.
//! - Building with `-C target-cpu=x86-64-v3` (or `native` on a Haswell+
//!   machine) enables a 32-byte/iteration `movemask` ASCII prefix scan
//!   (no runtime dispatch; not covered by the proofs below) and BMI2
//!   `shrx` for the shift-DFA (~+40% on the multibyte path).
//! - **`verus`** is verification-only (CI); it does not change runtime
//!   behaviour and is not intended to be combined with `simdutf8`.
//!
//! # Verification
//!
//! Under `--features verus` (portable 64-bit build, no `simdutf8`/`avx2`),
//! Verus proves **functional correctness**: [`verify`] and [`verify_with_slack`]
//! carry `ensures ret == is_valid_utf8(b@)`, where `spec::is_valid_utf8`
//! is a direct transcription of Unicode §3.9 Table 3-7. Every bit-trick in
//! the SWAR fast path and the multi-byte decoder is connected to that table
//! by a `by(bit_vector)` lemma; nothing is `assume`d. Differential testing
//! against [`core::str::from_utf8`] (proptest, libfuzzer) remains as a
//! cross-check on the trusted leaf-load specs.
//!
//! Memory safety of the raw-pointer loads — the only `unsafe` on the
//! verified path — is checked by two tools targeting complementary parts:
//!
//! - **Verus** (SMT-backed; enable with `--features verus`) verifies the
//!   slice-typed core end-to-end, including the multibyte state machine.
//!   The leaf load helpers are `external_body` with the spec
//!   `ret == pack64(buf@, at)` — the standard little-endian load contract.
//!
//! - **[RefinedRust]** (Rocq-backed; build with `--cfg rr`) verifies exactly
//!   those leaf bodies in `raw`: that `ptr.add(at).cast().read_unaligned()`
//!   is sound given separating ownership of `n` bytes with `at + N ≤ n`. The
//!   slice-typed core is out of its reach (it does not currently model Rust
//!   slices).
//!
//! Each tool's trusted base is what the other proves. The connecting step —
//! that `&[u8]::as_ptr()` yields a pointer valid for `len()` initialized
//! bytes — is the standard-library contract for slices. The
//! `simdutf8`-feature delegation path, the `cfg(avx2)` prefix scan, and the
//! `core::str::from_utf8` delegation on 32-bit targets are *not* covered by
//! these proofs. [`to_str`]'s call to `from_utf8_unchecked` is justified by
//! the functional-correctness proof of [`verify`], on the assumption that
//! `spec::is_valid_utf8` coincides with Rust's `str` invariant — both are
//! Unicode §3.9, but neither tool checks that equivalence.
//!
//! This crate is bool-only by design. Callers needing the byte position of a
//! validation error should use [`core::str::from_utf8`].
//!
//! [RefinedRust]: https://plv.mpi-sws.org/refinedrust/

#![no_std]
// Verus's driver injects `stmt_expr_attributes` and friends via `-Zcrate-attr`;
// `proc_macro_hygiene` is additionally needed for `#[verus_spec(invariant)]`
// on `while` loops (see rust_verify_test/tests/common/mod.rs). The `verus`
// feature is verification-only and is never built under stock rustc.
#![cfg_attr(feature = "verus", feature(proc_macro_hygiene))]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
// The load helpers below are one or two instructions each and sit in the hot
// loop; a non-inlined call boundary there would dominate the work. The
// `#[inline(always)]` is load-bearing, not decorative.
#![allow(clippy::inline_always)]
// RefinedRust attribute tool registration. The `rr` cfg is set only when
// running under the RefinedRust frontend; on a normal build these are no-ops.
#![cfg_attr(rr, feature(register_tool))]
#![cfg_attr(rr, feature(custom_inner_attributes))]
#![cfg_attr(rr, register_tool(rr))]
#![cfg_attr(rr, rr::package("smooth_utf8"))]
#![cfg_attr(rr, rr::coq_prefix("smooth_utf8"))]
#![cfg_attr(rr, rr::include("ptr"))]

#[cfg(feature = "alloc")]
extern crate alloc;

use core::ops::Range;

/// `debug_assert!` that compiles out under Verus (which does not model
/// panics; the Verus `requires` clause states the same condition).
macro_rules! da {
    ($($t:tt)*) => { #[cfg(not(feature = "verus"))] { core::debug_assert!($($t)*); } };
}

#[cfg(feature = "verus")]
use spec::*;
#[cfg(feature = "verus")]
use verus_builtin_macros::{proof, verus_spec, verus_verify};
#[cfg(feature = "verus")]
#[allow(unused_imports)]
use vstd::prelude::*;

mod ascii_skip;
mod raw;
#[cfg(feature = "verus")]
pub mod spec;

/// Inputs of this length or longer are delegated to `simdutf8` when the
/// `simdutf8` feature is enabled. Below it, the verified SWAR/slack path
/// is faster (no runtime dispatch); at and above, simdutf8's Keiser–Lemire
/// SIMD validator wins, decisively on mixed input.
#[cfg(feature = "simdutf8")]
const LONG_THRESHOLD: usize = 128;

/// Mask with the high bit of every byte set.
#[cfg_attr(feature = "verus", verus_verify)]
#[allow(clippy::unreadable_literal)] // grouping by byte is the readable form here
const SIGN_BITS: u64 = 0x8080_8080_8080_8080;

/// Number of readable bytes that must follow the logical end of the input
/// passed to [`verify_with_slack`].
///
/// The current implementation reads at most 7 bytes past the logical end; the
/// constant is rounded up to 8 to keep the contract simple and leave headroom.
#[cfg_attr(feature = "verus", verus_verify)]
pub const SLACK: usize = 8;

/// Returns `true` if `b` is well-formed UTF-8.
///
/// This is functionally equivalent to `core::str::from_utf8(b).is_ok()`, but
/// is tuned for the short, mostly-ASCII strings typical of serialized
/// protocols. It contains no `unsafe` over-reads: every wide load is either
/// fully in bounds or goes through a zero-padded stack buffer.
///
/// With `feature = "simdutf8"`, inputs of 128 bytes or more are delegated to
/// [`simdutf8::basic::from_utf8`](https://docs.rs/simdutf8).
///
/// ```
/// assert!(smoothutf8::verify("hello, 世界! 🌍".as_bytes()));
/// assert!(!smoothutf8::verify(&[0xC0, 0x80])); // overlong NUL
/// ```
#[cfg_attr(feature = "verus", verus_spec(ret =>
    ensures ret == is_valid_utf8(b@),
))]
#[inline]
#[must_use]
pub fn verify(b: &[u8]) -> bool {
    #[cfg(feature = "simdutf8")]
    if b.len() >= LONG_THRESHOLD {
        return simdutf8::basic::from_utf8(b).is_ok();
    }
    #[cfg(feature = "verus")]
    proof! { assert(b@.subrange(0, b@.len() as int) =~= b@); }
    verify_impl::<0, SafeTail>(b, 0..b.len())
}

/// Returns `Some(b as &str)` if `b` is well-formed UTF-8.
///
/// Single-scan: validates with [`verify`] and converts via
/// [`core::str::from_utf8_unchecked`] on success.
///
/// ```
/// assert_eq!(smoothutf8::to_str(b"abc"), Some("abc"));
/// assert_eq!(smoothutf8::to_str(&[0xFF]), None);
/// ```
#[inline]
#[must_use]
pub fn to_str(b: &[u8]) -> Option<&str> {
    if verify(b) {
        // SAFETY: `verify` returned true, so `b` is valid UTF-8.
        Some(unsafe { core::str::from_utf8_unchecked(b) })
    } else {
        None
    }
}

/// Returns `true` if `buf[range]` is well-formed UTF-8, using the slack-buffer
/// fast path.
///
/// This variant performs unaligned 8-byte loads that may read up to
/// [`SLACK`] − 1 bytes past `range.end`. Those bytes are masked off and never
/// influence the result; they need only be *readable*.
///
/// # Safety
///
/// The caller must guarantee all of the following:
///
/// - `range.start <= range.end`
/// - `range.end + SLACK <= buf.len()`
///
/// Because `buf` is a slice, satisfying the second condition is sufficient to
/// keep every load within the allocation that `buf` points into; the over-read
/// stays inside `buf` and is therefore not an out-of-bounds memory access at
/// the machine level. Violating either condition is undefined behaviour.
///
/// This is why the function takes the full backing buffer plus a logical
/// `range`, rather than a pre-sliced `&buf[range]`: the slack bytes past
/// `range.end` must remain part of the slice so that reading them is sound.
///
/// With `feature = "simdutf8"`, inputs of 128 bytes or more are delegated to
/// [`simdutf8::basic::from_utf8`](https://docs.rs/simdutf8) (the slack region
/// is not used on that path).
///
/// # Examples
///
/// ```
/// use smoothutf8::{verify_with_slack, SLACK};
/// let mut buf = b"field-value".to_vec();
/// let end = buf.len();
/// buf.resize(end + SLACK, 0); // your decoder's eps-copy padding
/// // SAFETY: `0 <= end` and `end + SLACK == buf.len()`.
/// assert!(unsafe { verify_with_slack(&buf, 0..end) });
/// ```
#[cfg_attr(feature = "verus", verus_spec(ret =>
    requires
        range.start <= range.end,
        range.end + SLACK <= buf@.len(),
    ensures
        ret == is_valid_utf8(buf@.subrange(range.start as int, range.end as int)),
))]
#[inline]
#[must_use]
pub unsafe fn verify_with_slack(buf: &[u8], range: Range<usize>) -> bool {
    da!(range.start <= range.end);
    da!(range.end.saturating_add(SLACK) <= buf.len());
    #[cfg(feature = "simdutf8")]
    if range.end - range.start >= LONG_THRESHOLD {
        return simdutf8::basic::from_utf8(&buf[range]).is_ok();
    }
    verify_impl::<SLACK, SlackTail>(buf, range)
}

// -- SlackBuf: safe wrapper over the slack-buffer invariant ------------------
//
// Verus specs are not yet written for this surface (issue #5); the wrapper is
// gated out of the verus build until they are. The underlying `verify_impl`
// path it calls is the same verified code.
#[cfg(not(feature = "verus"))]
pub use slack_buf::SlackBuf;

#[cfg(not(feature = "verus"))]
mod slack_buf {
    use super::{verify_with_slack, Range, SLACK};

    /// A borrowed byte buffer whose final [`SLACK`] bytes are padding.
    ///
    /// Any read of width up to `SLACK` starting at a position `≤`
    /// [`payload_len`](Self::payload_len) stays in bounds, so per-field
    /// validation and fixed-width loads need no `unsafe` at the call site.
    #[repr(transparent)]
    #[derive(Clone, Copy, Debug)]
    pub struct SlackBuf<'a>(&'a [u8]);

    // `le_u32`'s SAFETY argument relies on this; pin it at compile time so a
    // future change to `SLACK` cannot silently make the load OOB.
    const _: () = assert!(SLACK >= 4);

    impl<'a> SlackBuf<'a> {
        /// Wraps `buf` without checking the length invariant.
        ///
        /// # Safety
        /// `buf.len() >= SLACK`.
        #[inline]
        #[must_use]
        pub const unsafe fn new_unchecked(buf: &'a [u8]) -> Self {
            debug_assert!(buf.len() >= SLACK);
            Self(buf)
        }

        /// Wraps `buf`, or returns `None` if `buf.len() < SLACK`.
        #[inline]
        #[must_use]
        pub const fn new(buf: &'a [u8]) -> Option<Self> {
            if buf.len() >= SLACK {
                Some(Self(buf))
            } else {
                None
            }
        }

        /// Wraps `buf`, treating its final [`SLACK`] bytes as padding.
        ///
        /// This is the alloc-free constructor for callers that have already
        /// padded the buffer themselves. With the `alloc` feature,
        #[cfg_attr(feature = "alloc", doc = " [`new_add_slack`](Self::new_add_slack)")]
        #[cfg_attr(not(feature = "alloc"), doc = " `new_add_slack`")]
        /// appends the padding for you.
        ///
        /// # Panics
        /// If `buf.len() < SLACK`.
        #[inline]
        #[must_use]
        #[track_caller]
        pub const fn new_embedded_slack(buf: &'a [u8]) -> Self {
            assert!(buf.len() >= SLACK, "buf must carry SLACK trailing bytes");
            Self(buf)
        }

        /// Appends [`SLACK`] zero bytes to `v` and wraps the result.
        ///
        /// `v` is borrowed for `'a`: it cannot be grown or otherwise mutated
        /// while the returned `SlackBuf` is alive, and on return its length is
        /// `original_len + SLACK`. May reallocate; reserve `SLACK` extra
        /// capacity before filling `v` if that matters.
        #[cfg(feature = "alloc")]
        #[inline]
        #[must_use]
        pub fn new_add_slack(v: &'a mut alloc::vec::Vec<u8>) -> Self {
            v.extend_from_slice(&[0u8; SLACK]);
            Self(v.as_slice())
        }

        /// The full backing slice, including the trailing slack bytes.
        #[inline]
        #[must_use]
        pub const fn as_bytes(&self) -> &'a [u8] {
            self.0
        }

        /// `as_bytes().len() - SLACK`: the largest valid `range.end` for
        /// [`verify`](Self::verify) and the largest valid `at` for the
        /// fixed-width loads.
        #[inline]
        #[must_use]
        pub const fn payload_len(&self) -> usize {
            self.0.len() - SLACK
        }

        /// Returns `true` if `self.as_bytes()[range]` is well-formed UTF-8,
        /// using the slack-buffer fast path.
        ///
        /// # Panics
        /// If `range.start > range.end` or `range.end > self.payload_len()`.
        #[inline]
        #[must_use]
        pub fn verify(&self, range: Range<usize>) -> bool {
            // One combined assert (not two) so there is a single panic call
            // site and the function stays prologue-free for the tail call.
            assert!(range.start <= range.end && range.end <= self.payload_len());
            // SAFETY: the assert establishes both of `verify_with_slack`'s
            // preconditions (`start <= end` and `end + SLACK <= len`, given the
            // type invariant `len >= SLACK`).
            unsafe { verify_with_slack(self.0, range) }
        }

        /// Returns `Some(self.as_bytes()[range] as &str)` if the range is
        /// well-formed UTF-8. Single-scan; the returned slice borrows the
        /// backing buffer for `'a`.
        ///
        /// # Panics
        /// Same conditions as [`verify`](Self::verify).
        #[inline]
        #[must_use]
        pub fn to_str(&self, range: Range<usize>) -> Option<&'a str> {
            if self.verify(range.clone()) {
                // SAFETY: `verify`'s asserts established
                // `range.start <= range.end <= len - SLACK <= len`, so
                // `get_unchecked(range)` is in-bounds; `verify` returned true,
                // so the bytes are valid UTF-8.
                Some(unsafe { core::str::from_utf8_unchecked(self.0.get_unchecked(range)) })
            } else {
                None
            }
        }

        /// Loads 4 bytes at `at..at+4` as a little-endian `u32`. The slack
        /// invariant (`SLACK >= 4`) makes this a single unaligned load with no
        /// tail handling.
        ///
        /// # Panics
        /// If `at > self.payload_len()`.
        #[inline]
        #[must_use]
        pub fn le_u32(&self, at: usize) -> u32 {
            assert!(at <= self.payload_len());
            // SAFETY: `at <= len - SLACK` and `SLACK >= 4` (const-asserted
            // above), so `at + 4 <= len`; `as_ptr()` carries provenance over
            // the full slice and `read_unaligned` avoids alignment UB.
            u32::from_le(unsafe { self.0.as_ptr().add(at).cast::<u32>().read_unaligned() })
        }
    }
}

// -- leaf loads (Verus-trusted, RefinedRust-verified) ------------------------

/// Load 8 bytes at `buf[at..at+8]` as a little-endian `u64`.
///
/// # Safety
/// `at + 8 <= buf.len()`.
#[cfg_attr(feature = "verus", verus_verify(external_body))]
#[cfg_attr(feature = "verus", verus_spec(ret =>
    requires at + 8 <= buf@.len(),
    ensures ret == pack64(buf@, at as int),
))]
#[inline(always)]
unsafe fn load64(buf: &[u8], at: usize) -> u64 {
    da!(at + 8 <= buf.len());
    // SAFETY: `at + 8 <= buf.len()`, so `buf.as_ptr()` is valid for `at + 8`
    // bytes; this is exactly `load64_raw`'s precondition.
    unsafe { raw::load64_raw(buf.as_ptr(), at) }
}

/// Load 1 byte at `buf[at]`.
///
/// # Safety
/// `at < buf.len()`.
#[cfg_attr(feature = "verus", verus_verify(external_body))]
#[cfg_attr(feature = "verus", verus_spec(ret =>
    requires at < buf@.len(),
    ensures ret == buf@[at as int],
))]
#[inline(always)]
#[cfg(target_pointer_width = "64")]
unsafe fn load8(buf: &[u8], at: usize) -> u8 {
    da!(at < buf.len());
    // SAFETY: `at < buf.len()`; see `load64`.
    unsafe { raw::load8_raw(buf.as_ptr(), at) }
}

// -- implementation ----------------------------------------------------------

/// Tail-load strategy: how to obtain a `u64` when fewer than 8 bytes remain
/// before the logical end.
///
/// The const parameter `PAD` is the number of bytes that must be readable
/// past `end`: `0` for [`SafeTail`], [`SLACK`] for [`SlackTail`]. It is a
/// trait parameter (not an associated const) so the trait-level safety
/// contract `at < end ∧ end + PAD ≤ buf.len()` can be expressed in Verus's
/// attribute mode, which does not yet support associated consts in
/// trait-impl proxies.
#[cfg_attr(feature = "verus", verus_verify)]
trait Tail<const PAD: usize> {
    /// Load 8 bytes' worth at `buf[at..]`, where `at < end` and possibly
    /// `at + 8 > end`. Bytes at positions `>= end` may be garbage; the caller
    /// masks them.
    #[cfg_attr(feature = "verus", verus_spec(ret =>
        requires at < end, end + PAD <= buf@.len(),
        ensures forall |j: int| 0 <= j < 8 && at + j < end
            ==> #[trigger] byte64(ret, j) == buf@[at + j],
    ))]
    fn load64(buf: &[u8], at: usize, end: usize) -> u64;
}

/// Safe tail: copies the remaining `end - at` bytes into a zero-padded stack
/// buffer and loads from that. Never reads past `end`.
#[cfg_attr(feature = "verus", verus_verify)]
struct SafeTail;

#[cfg_attr(feature = "verus", verus_verify)]
impl Tail<0> for SafeTail {
    #[cfg_attr(feature = "verus", verus_verify(external_body))]
    #[inline(always)]
    fn load64(buf: &[u8], at: usize, end: usize) -> u64 {
        da!(at < end && end <= buf.len());
        if at + 8 <= end {
            // SAFETY: `at + 8 <= end <= buf.len()`.
            unsafe { load64(buf, at) }
        } else {
            let mut tmp = [0u8; 8];
            tmp[..end - at].copy_from_slice(&buf[at..end]);
            u64::from_le_bytes(tmp)
        }
    }
}

/// Slack tail: reads 8 bytes unconditionally. Requires the [`SLACK`]
/// precondition on `buf`, which the caller of [`verify_with_slack`] upholds.
#[cfg_attr(feature = "verus", verus_verify)]
struct SlackTail;

#[cfg_attr(feature = "verus", verus_verify)]
impl Tail<SLACK> for SlackTail {
    #[inline(always)]
    fn load64(buf: &[u8], at: usize, end: usize) -> u64 {
        da!(at < end);
        let _ = end;
        // SAFETY: `verify_with_slack`'s precondition gives
        // `end + SLACK <= buf.len()` with `SLACK >= 8`, and every call site
        // guarantees `at < end`, hence `at + 8 <= buf.len()`.
        let ret = unsafe { load64(buf, at) };
        #[cfg(feature = "verus")]
        proof! {
            assert forall |j: int| 0 <= j < 8 && at + j < end
                implies #[trigger] byte64(ret, j) == buf@[at + j] by {
                lemma_pack64_byte(buf@, at as int, j);
            };
        }
        ret
    }
}

#[cfg_attr(feature = "verus", verus_spec(ret =>
    requires
        range.start <= range.end,
        range.end + PAD <= buf@.len(),
    ensures
        ret == is_valid_utf8(buf@.subrange(range.start as int, range.end as int)),
))]
#[inline]
fn verify_impl<const PAD: usize, T: Tail<PAD>>(buf: &[u8], range: Range<usize>) -> bool {
    let start = range.start;
    let end = range.end;
    let mut p = start;
    if p == end {
        #[cfg(feature = "verus")]
        proof! { assert(buf@.subrange(start as int, end as int).len() == 0); }
        return true;
    }

    // ---- ASCII fast path: full STEP-byte blocks ---------------------------
    // Portable SWAR (16 B/iter) by default; AVX2 (32 B/iter) when built with
    // `+avx2`. Returns at the first non-ASCII byte or with `< STEP` left.
    p = ascii_skip::skip(buf, p, end);
    if end - p >= ascii_skip::STEP {
        #[cfg(feature = "verus")]
        proof! { lemma_ascii_prefix_iff(buf@, start as int, p as int, end as int); }
        return verify_multibyte(buf, p, end);
    }

    // ---- ASCII fast path: remaining full 8-byte words ---------------------
    // At most one iteration when `STEP == 16`; up to three when `STEP == 32`.
    #[cfg_attr(feature = "verus", verus_spec(
        invariant
            start == range.start, end == range.end,
            start <= p, p <= end, end + PAD <= buf@.len(),
            all_ascii(buf@, start as int, p as int),
        decreases end - p
    ))]
    while end - p >= 8 {
        // SAFETY: `p + 8 <= end <= buf.len()`.
        let bytes = unsafe { load64(buf, p) };
        if bytes & SIGN_BITS != 0 {
            #[cfg(feature = "verus")]
            proof! { lemma_ascii_prefix_iff(buf@, start as int, p as int, end as int); }
            return verify_multibyte(buf, p, end);
        }
        #[cfg(feature = "verus")]
        proof! {
            lemma_signbits8(buf@, p as int);
            lemma_ascii_extend(buf@, start as int, p as int, p as int + 8);
        }
        p += 8;
    }

    // ---- ASCII fast path: 0..=7 trailing bytes ----------------------------
    if p < end {
        let left = end - p; // 1..=7
        let bytes = T::load64(buf, p, end);
        // `left ∈ 1..=7`, so the shift amount is in `8..=56` — never 0 or 64.
        let mask = SIGN_BITS >> ((8 - left) * 8);
        if bytes & mask != 0 {
            #[cfg(feature = "verus")]
            proof! { lemma_ascii_prefix_iff(buf@, start as int, p as int, end as int); }
            return verify_multibyte(buf, p, end);
        }
        #[cfg(feature = "verus")]
        proof! {
            assert(mask == sign_mask(left as int));
            lemma_mask_zero_ascii(buf@, p as int, left as int, bytes);
            lemma_ascii_extend(buf@, start as int, p as int, end as int);
            lemma_ascii_valid(buf@, start as int, end as int);
        }
    } else {
        #[cfg(feature = "verus")]
        proof! { lemma_ascii_valid(buf@, start as int, end as int); }
    }

    true
}

// -- Höhrmann-style UTF-8 DFA -----------------------------------------------
//
// The shift-DFA's per-byte step is `(ROW[byte] >> state) & 63` — a 64-bit
// variable shift. That's one instruction on 64-bit targets but 7-13 on 32-bit
// (Cortex-M, i686), where it ends up slower than the standard library's
// branchy validator. On non-64-bit targets `verify_multibyte` delegates to
// `core::str::from_utf8` instead, and the tables below are dead `const`s
// (no runtime footprint). The ASCII fast path above is kept regardless.
//
// `target_pointer_width` is a proxy for "has native u64 ops". It is slightly
// conservative — wasm32 has native i64 but is gated out here — but the
// fallback is `core::str::from_utf8`, which is fine, not a footgun.

#[cfg(not(target_pointer_width = "64"))]
#[inline]
fn verify_multibyte(buf: &[u8], start: usize, end: usize) -> bool {
    core::str::from_utf8(&buf[start..end]).is_ok()
}

// Byte → class (12 classes) and state×class → next-state tables. State values
// are pre-multiplied by 12 so the transition is a single add+load. Tables are
// the canonical Höhrmann set (also used by `bstr`); see
// <https://bjoern.hoehrmann.de/utf-8/decoder/dfa/>.
//
// `TRANS` is padded from 108 to 256 entries (with REJECT) so that indexing by
// a `u8` sum lets the compiler elide the bounds check.

#[allow(dead_code)] // documentation for `TRANS`; the hot path uses `DFA_ACCEPT`
const ACCEPT: u8 = 12;
#[allow(dead_code)]
const REJECT: u8 = 0;

#[rustfmt::skip]
const CLASS: [u8; 256] = [
     0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
     0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
     0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
     0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
     1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,  9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,
     7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,  7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
     8,8,2,2,2,2,2,2,2,2,2,2,2,2,2,2,  2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
    10,3,3,3,3,3,3,3,3,3,3,3,3,4,3,3, 11,6,6,6,5,8,8,8,8,8,8,8,8,8,8,8,
];

#[rustfmt::skip]
const TRANS: [u8; 256] = [
     0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    12, 0,24,36,60,96,84, 0, 0, 0,48,72,
     0,12, 0, 0, 0, 0, 0,12, 0,12, 0, 0,
     0,24, 0, 0, 0, 0, 0,24, 0,24, 0, 0,
     0, 0, 0, 0, 0, 0, 0,24, 0, 0, 0, 0,
     0,24, 0, 0, 0, 0, 0, 0, 0,24, 0, 0,
     0, 0, 0, 0, 0, 0, 0,36, 0,36, 0, 0,
     0,36, 0, 0, 0, 0, 0,36, 0,36, 0, 0,
     0,36, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    // padding (REJECT) so that a u8 index elides the bounds check
     0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
     0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
     0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
     0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
     0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
];

/// Shift-based DFA row table: `ROW[byte]` packs the next state for *every*
/// current state into one `u64`, at 6-bit stride. The transition is then
/// `state' = (ROW[byte] >> state) & 63` — the only load depends on `byte`
/// (off the critical path), and the state→state' chain is one shift + one
/// mask. This is the Vognsen/Langdale encoding of the Höhrmann automaton.
///
/// State values are `index * 6` (so they double as shift amounts):
/// REJECT = 0, ACCEPT = 6, intermediates 12..=48. Nine states × 6 bits = 54
/// bits used per row.
const fn build_rows() -> [u64; 256] {
    let mut row = [0u64; 256];
    let mut b = 0usize;
    while b < 256 {
        let class = CLASS[b] as usize;
        let mut s = 0usize;
        while s < 9 {
            // `TRANS` next-state in the {0,12,24,..,96} encoding → index → ×6.
            let next = (TRANS[s * 12 + class] / 12) as u64 * 6;
            row[b] |= next << (s * 6);
            s += 1;
        }
        b += 1;
    }
    row
}
const ROW: [u64; 256] = build_rows();

/// Shift-DFA state values (`index * 6`, so each is its own shift amount).
/// Names follow the Unicode §3.9 Table 3-7 partial-match position each
/// represents; the proof's per-state-meaning lemmas reference these.
mod state {
    /// Absorbing reject.
    pub const REJECT: u64 = 0;
    /// At a sequence boundary: input so far is well-formed.
    pub const ACCEPT: u64 = 6;
    /// One continuation byte (`80..=BF`) remaining.
    pub const C1: u64 = 12;
    /// Two continuation bytes remaining.
    pub const C2: u64 = 18;
    /// After `E0`: next byte must be `A0..=BF`, then one continuation.
    pub const E0: u64 = 24;
    /// After `ED`: next byte must be `80..=9F`, then one continuation.
    pub const ED: u64 = 30;
    /// After `F0`: next byte must be `90..=BF`, then two continuations.
    pub const F0: u64 = 36;
    /// Three continuation bytes remaining (after `F1..=F3`).
    pub const C3: u64 = 42;
    /// After `F4`: next byte must be `80..=8F`, then two continuations.
    pub const F4: u64 = 48;
}
#[cfg_attr(feature = "verus", verus_verify)]
const DFA_ACCEPT: u64 = 6;
#[cfg_attr(feature = "verus", verus_verify)]
const DFA_REJECT: u64 = 0;
const _: [(); 0] = [(); (DFA_ACCEPT != state::ACCEPT) as usize];
const _: [(); 0] = [(); (DFA_REJECT != state::REJECT) as usize];
const _: [(); 0] = [(); (state::F4 + 6 > 64) as usize]; // 9 states fit in u64

// Compile-time check that the shift encoding agrees with `TRANS`/`CLASS` at
// the row that drives state assignment (ACCEPT × every lead-byte class). The
// full 2304-cell equivalence is the Verus `by(compute)` obligation.
macro_rules! row_check {
    ($($byte:literal -> $state:path,)*) => {
        $(const _: [(); 0] = [(); ((ROW[$byte] >> state::ACCEPT) & 63 != $state) as usize];)*
    };
}
row_check! {
    0x41 -> state::ACCEPT, 0x80 -> state::REJECT, 0xC1 -> state::REJECT,
    0xC2 -> state::C1,     0xE0 -> state::E0,     0xE1 -> state::C2,
    0xED -> state::ED,     0xEE -> state::C2,     0xF0 -> state::F0,
    0xF1 -> state::C3,     0xF4 -> state::F4,     0xF5 -> state::REJECT,
}

/// `ROW[byte]`. The Verus spec is [`spec_row`]; the compile-time
/// `_CHECK_SPEC_ROW` assertion below validates `ROW[b]` against a literal
/// transcription of `spec_row` for all 256 bytes, so the residual trusted
/// step is the visually-auditable match between that literal and
/// `spec::spec_row` (a `spec fn`, not callable from const-eval).
#[cfg_attr(feature = "verus", verus_verify(external_body))]
#[cfg_attr(feature = "verus", verus_spec(ret =>
    ensures ret == spec_row(byte),
))]
#[inline(always)]
#[cfg(target_pointer_width = "64")]
const fn row(byte: u8) -> u64 {
    ROW[byte as usize]
}

/// Exhaustive const-eval check that `row`'s body agrees with its Verus spec.
#[allow(clippy::unreadable_literal, clippy::cast_possible_truncation)]
const _CHECK_SPEC_ROW: () = {
    let mut b = 0usize;
    while b < 256 {
        let bb = b as u8;
        // Literal transcription of `spec::spec_row`.
        let spec: u64 = if bb <= 0x7F {
            0x0000000000000180
        } else if bb <= 0x8F {
            0x0012480300306000
        } else if bb <= 0x9F {
            0x0000492300306000
        } else if bb <= 0xBF {
            0x000049200C306000
        } else if bb <= 0xC1 {
            0
        } else if bb <= 0xDF {
            0x0000000000000300
        } else if bb == 0xE0 {
            0x0000000000000600
        } else if bb <= 0xEC {
            0x0000000000000480
        } else if bb == 0xED {
            0x0000000000000780
        } else if bb <= 0xEF {
            0x0000000000000480
        } else if bb == 0xF0 {
            0x0000000000000900
        } else if bb <= 0xF3 {
            0x0000000000000A80
        } else if bb == 0xF4 {
            0x0000000000000C00
        } else {
            0
        };
        assert!(ROW[b] == spec);
        b += 1;
    }
};

/// One shift-DFA step. Critical-path latency: shr + and (≈2 cyc); the
/// `ROW[byte]` load depends only on the input byte.
#[cfg_attr(feature = "verus", verus_verify)]
#[cfg_attr(feature = "verus", verus_spec(ret =>
    requires is_state(state),
    ensures ret == spec_step(state, byte), is_state(ret),
))]
#[inline(always)]
#[cfg(target_pointer_width = "64")]
const fn step(state: u64, byte: u8) -> u64 {
    #[cfg(feature = "verus")]
    proof! { lemma_row_step(state, byte); }
    (row(byte) >> (state & 63)) & 63
}

/// Slow path: at least one byte at or after `start` has its high bit set.
///
/// Functional contract: returns `is_valid_utf8(buf[start..end])`.
#[cfg_attr(feature = "verus", verus_verify)]
#[cfg_attr(feature = "verus", verus_spec(ret =>
    requires
        start < end,
        end <= buf@.len(),
    ensures
        ret == is_valid_utf8(buf@.subrange(start as int, end as int)),
))]
#[cfg(target_pointer_width = "64")]
#[inline]
#[allow(clippy::cast_possible_truncation)] // `(w >> 8k) as u8` is byte extraction
#[allow(clippy::too_many_lines)] // proof annotations
fn verify_multibyte(buf: &[u8], start: usize, end: usize) -> bool {
    let mut state = DFA_ACCEPT;
    let mut p = start;
    #[cfg(feature = "verus")]
    proof! { assert(buf@.subrange(start as int, start as int).len() == 0); }

    // Full 8-byte chunks: one unaligned load, eight unrolled DFA steps.
    #[cfg_attr(feature = "verus", verus_spec(
        invariant
            start <= p, p <= end, end <= buf@.len(),
            is_state(state),
            state == run(ST_ACCEPT, buf@.subrange(start as int, p as int)),
        decreases end - p
    ))]
    while end - p >= 8 {
        // SAFETY: `p + 8 <= end` and `end + PAD <= buf.len()` with `PAD >= 0`.
        let w = unsafe { load64(buf, p) };
        // ASCII re-skip: if we are between codepoints and all eight bytes are
        // ASCII, the DFA would walk ACCEPT→ACCEPT eight times. One masked
        // compare per chunk; always-false on dense multibyte (predictable).
        if state == DFA_ACCEPT && w & SIGN_BITS == 0 {
            #[cfg(feature = "verus")]
            proof! {
                lemma_signbits8(buf@, p as int);
                lemma_ascii_valid(buf@, p as int, p as int + 8);
                lemma_run_valid(buf@.subrange(p as int, p as int + 8));
                lemma_run_join(ST_ACCEPT, buf@, start as int, p as int, p as int + 8);
            }
            p += 8;
            continue;
        }
        #[cfg(feature = "verus")]
        proof! {
            assert(w >> 0u64 == w) by (bit_vector);
            lemma_chunk_snoc(buf@, start as int, p as int, w, 0, state);
        }
        state = step(state, w as u8);
        #[cfg(feature = "verus")]
        proof! { lemma_chunk_snoc(buf@, start as int, p as int, w, 1, state); }
        state = step(state, (w >> 8) as u8);
        #[cfg(feature = "verus")]
        proof! { lemma_chunk_snoc(buf@, start as int, p as int, w, 2, state); }
        state = step(state, (w >> 16) as u8);
        #[cfg(feature = "verus")]
        proof! { lemma_chunk_snoc(buf@, start as int, p as int, w, 3, state); }
        state = step(state, (w >> 24) as u8);
        #[cfg(feature = "verus")]
        proof! { lemma_chunk_snoc(buf@, start as int, p as int, w, 4, state); }
        state = step(state, (w >> 32) as u8);
        #[cfg(feature = "verus")]
        proof! { lemma_chunk_snoc(buf@, start as int, p as int, w, 5, state); }
        state = step(state, (w >> 40) as u8);
        #[cfg(feature = "verus")]
        proof! { lemma_chunk_snoc(buf@, start as int, p as int, w, 6, state); }
        state = step(state, (w >> 48) as u8);
        #[cfg(feature = "verus")]
        proof! { lemma_chunk_snoc(buf@, start as int, p as int, w, 7, state); }
        state = step(state, (w >> 56) as u8);
        p += 8;
        if state == DFA_REJECT {
            #[cfg(feature = "verus")]
            proof! {
                lemma_run_join(ST_ACCEPT, buf@, start as int, p as int, end as int);
                lemma_run_reject(buf@.subrange(p as int, end as int));
                lemma_run_valid(buf@.subrange(start as int, end as int));
            }
            return false;
        }
    }
    // Tail: 0..=7 bytes.
    #[cfg_attr(feature = "verus", verus_spec(
        invariant
            start <= p, p <= end, end <= buf@.len(),
            is_state(state),
            state == run(ST_ACCEPT, buf@.subrange(start as int, p as int)),
        decreases end - p
    ))]
    while p < end {
        // SAFETY: `p < end <= end + PAD <= buf.len()`.
        state = step(state, unsafe { load8(buf, p) });
        #[cfg(feature = "verus")]
        proof! { lemma_run_snoc(ST_ACCEPT, buf@, start as int, p as int); }
        p += 1;
    }
    #[cfg(feature = "verus")]
    proof! { lemma_run_valid(buf@.subrange(start as int, end as int)); }
    state == DFA_ACCEPT
}

// -- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests;
