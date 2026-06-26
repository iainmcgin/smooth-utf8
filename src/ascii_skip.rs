//! ASCII-prefix skip: returns `q` with `start <= q <= end`, every byte in
//! `buf[start..q]` is ASCII, and either `end - q < STEP` or the block
//! `buf[q..q+STEP]` contains a non-ASCII byte. The caller handles the
//! `< STEP`-byte tail.
//!
//! Three implementations, selected at compile time (no runtime dispatch, so
//! short strings pay no detection overhead):
//!
//! - Portable 16-byte SWAR (the default and the Verus-verified path).
//! - 32-byte AVX2 `movemask` scan when built with `-C target-feature=+avx2`.
//! - 32-byte NEON `umaxv` scan on `aarch64` (NEON is mandatory there, so this
//!   is the default aarch64 build).
//!
//! The SIMD paths are `cfg(not(feature = "verus"))`; under verus the SWAR
//! path is always selected.

#[cfg(feature = "verus")]
use crate::spec::*;
#[cfg(feature = "verus")]
use verus_builtin_macros::{proof, verus_spec, verus_verify};
#[cfg(feature = "verus")]
#[allow(unused_imports)]
use vstd::prelude::*;

// The AVX2 path is suppressed under `feature = "verus"` so the verified SWAR
// `skip` is always the one Verus sees, regardless of the verification host's
// target features.
#[cfg(all(
    target_arch = "x86_64",
    target_feature = "avx2",
    not(feature = "verus")
))]
mod imp {
    use core::arch::x86_64::{_mm256_loadu_si256, _mm256_movemask_epi8};

    pub const STEP: usize = 32;

    /// # Safety
    /// `p + STEP <= end <= buf.len()` on every iteration the loop body runs;
    /// equivalently, `p <= end <= buf.len()` on entry (the loop guard checks
    /// `end - p >= STEP` before each load).
    ///
    /// This module is `cfg(not(feature = "verus"))`; it carries no Verus
    /// spec and is outside the functional-correctness proof.
    #[inline]
    pub fn skip(buf: &[u8], start: usize, end: usize) -> usize {
        let mut p = start;
        // SAFETY: the module is `cfg(target_feature = "avx2")`, so the AVX2
        // intrinsics are sound to call without a runtime check.
        // `_mm256_loadu_si256` reads 32 bytes; the loop guard ensures
        // `p + 32 <= end <= buf.len()`. `movemask` packs each lane's sign bit
        // into a 32-bit mask; a zero mask means all 32 bytes are ASCII, and
        // `trailing_zeros` of a non-zero mask gives the first non-ASCII byte.
        while end - p >= STEP {
            let v = unsafe { _mm256_loadu_si256(buf.as_ptr().add(p).cast()) };
            let m = unsafe { _mm256_movemask_epi8(v) } as u32;
            if m != 0 {
                return p + m.trailing_zeros() as usize;
            }
            p += STEP;
        }
        p
    }
}

#[cfg(all(
    target_arch = "aarch64",
    target_feature = "neon",
    not(feature = "verus")
))]
mod imp {
    use core::arch::aarch64::{vld1q_u8, vmaxvq_u8, vorrq_u8};

    pub const STEP: usize = 32;

    /// 32-byte/iteration NEON ASCII scan.
    ///
    /// Loads two 16-byte vectors, ORs them, and reduces with `umaxv`: a result
    /// `< 0x80` means all 32 bytes are ASCII. On a non-ASCII block this returns
    /// the *block start* (not the precise byte) — the caller's 8-byte loop
    /// re-scans the block, same as the SWAR path's contract. Finding the exact
    /// lane on NEON (no `pmovmskb`) costs more than the re-scan saves on the
    /// short inputs this crate is tuned for.
    ///
    /// Outside the Verus proof; carries no spec.
    #[inline]
    pub fn skip(buf: &[u8], start: usize, end: usize) -> usize {
        let mut p = start;
        while end - p >= STEP {
            // SAFETY: `p + 32 <= end <= buf.len()`; `vld1q_u8` is an unaligned
            // 16-byte load. NEON is mandatory on aarch64, so the intrinsics are
            // sound to call without a runtime feature check.
            let a = unsafe { vld1q_u8(buf.as_ptr().add(p)) };
            let b = unsafe { vld1q_u8(buf.as_ptr().add(p + 16)) };
            if unsafe { vmaxvq_u8(vorrq_u8(a, b)) } >= 0x80 {
                return p;
            }
            p += STEP;
        }
        p
    }
}

#[cfg(not(any(
    all(
        target_arch = "x86_64",
        target_feature = "avx2",
        not(feature = "verus")
    ),
    all(
        target_arch = "aarch64",
        target_feature = "neon",
        not(feature = "verus")
    ),
)))]
mod imp {
    #[allow(unused_imports)]
    use super::*;
    use crate::{load64, SIGN_BITS};

    #[cfg_attr(feature = "verus", verus_verify)]
    pub const STEP: usize = 16;

    #[cfg_attr(feature = "verus", verus_spec(q =>
        requires start <= end, end <= buf@.len(),
        ensures start <= q, q <= end, all_ascii(buf@, start as int, q as int),
    ))]
    #[inline]
    pub fn skip(buf: &[u8], start: usize, end: usize) -> usize {
        let mut p = start;
        #[cfg_attr(feature = "verus", verus_spec(
            invariant
                start <= p, p <= end, end <= buf@.len(),
                all_ascii(buf@, start as int, p as int),
            decreases end - p
        ))]
        while end - p >= STEP {
            // SAFETY: `p + 16 <= end <= buf.len()`.
            let a = unsafe { load64(buf, p) };
            let b = unsafe { load64(buf, p + 8) };
            if (a | b) & SIGN_BITS != 0 {
                return p;
            }
            #[cfg(feature = "verus")]
            proof! {
                lemma_signbits16(buf@, p as int);
                lemma_ascii_extend(buf@, start as int, p as int, p as int + 16);
            }
            p += STEP;
        }
        p
    }
}

pub use imp::{skip, STEP};
