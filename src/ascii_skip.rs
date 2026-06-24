//! ASCII-prefix skip: returns the index of the first byte in `buf[p..end]`
//! whose high bit is set, or some `q` with `end - q < STEP` if every full
//! `STEP`-byte block is ASCII. The caller handles the `< STEP`-byte tail.
//!
//! Two implementations: a portable 16-byte SWAR loop (the default and the
//! Verus-verified path), and a 32-byte AVX2 prefix scan selected at compile
//! time when building with `-C target-feature=+avx2` (or `target-cpu=native`
//! on a host that has it). There is no runtime dispatch — the choice is
//! static — so short strings pay no detection overhead.

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

#[cfg(not(all(
    target_arch = "x86_64",
    target_feature = "avx2",
    not(feature = "verus")
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
