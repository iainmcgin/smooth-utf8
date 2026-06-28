//! Raw-pointer leaf loads — the [RefinedRust] verification target.
//!
//! These are the only place this crate dereferences a raw pointer. The
//! slice-typed wrappers in the crate root (`load64` etc.) call into here
//! after `as_ptr()`, and Verus treats those wrappers as `external_body`
//! with the precondition `at + N ≤ buf.len()`. [RefinedRust] then verifies
//! *these* bodies — `ptr.add(at).cast().read_unaligned()` — against
//! ownership of `n` initialized bytes at `ptr`, closing the gap Verus trusts.
//!
//! [RefinedRust]: https://plv.mpi-sws.org/refinedrust/
//!
//! # Spec rationale
//!
//! The precondition is `value_t (mk_array_layout u8 n)` ownership of the
//! whole buffer (not bare `loc_in_bounds`, which is `Persistent` and admits
//! uninitialized memory — see the proof-gap audit). The `read_unaligned`
//! shim's contract is stated over that same whole-buffer ownership with the
//! source refined as `l offsetₗ at`, so the carving step is part of the
//! trusted contract rather than a manual `value_t` split/join lemma. The
//! ownership is returned unchanged in the postcondition.
//!
//! `n ≤ MaxInt ISize` is required so the `<*const u8>::add` side-condition
//! `at * 1 ∈ ISize` discharges (Rust forbids allocations larger than
//! `isize::MAX` bytes; this just states it).
#![allow(clippy::missing_const_for_fn)] // const is cosmetic; RR may not model it

#[cfg(rr)]
mod shims;

/// # Safety
/// `ptr` is valid for reads of `n ≥ at + 8` initialized bytes.
#[cfg_attr(rr, rr::params("l" : "loc", "n" : "Z", "i" : "Z", "vs"))]
#[cfg_attr(rr, rr::args("l", "i"))]
#[cfg_attr(rr, rr::requires(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))"))]
#[cfg_attr(rr, rr::requires("(0 ≤ i)%Z"))]
#[cfg_attr(rr, rr::requires("(i + 8 ≤ n)%Z"))]
#[cfg_attr(rr, rr::requires("(n ≤ MaxInt ISize)%Z"))]
#[cfg_attr(rr, rr::ensures(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))"))]
#[cfg_attr(rr, rr::exists("v" : "Z"))]
#[cfg_attr(rr, rr::returns("v"))]
#[inline(always)]
pub unsafe fn load64_raw(ptr: *const u8, at: usize) -> u64 {
    // SAFETY: caller guarantees `at + 8 ≤ n` initialized bytes at `ptr`.
    let raw = unsafe { core::ptr::read_unaligned(ptr.add(at).cast::<u64>()) };
    u64::from_le(raw)
}

/// # Safety
/// `ptr` is valid for reads of `n ≥ at + 4` initialized bytes.
#[cfg_attr(rr, rr::params("l" : "loc", "n" : "Z", "i" : "Z", "vs"))]
#[cfg_attr(rr, rr::args("l", "i"))]
#[cfg_attr(rr, rr::requires(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))"))]
#[cfg_attr(rr, rr::requires("(0 ≤ i)%Z"))]
#[cfg_attr(rr, rr::requires("(i + 4 ≤ n)%Z"))]
#[cfg_attr(rr, rr::requires("(n ≤ MaxInt ISize)%Z"))]
#[cfg_attr(rr, rr::ensures(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))"))]
#[cfg_attr(rr, rr::exists("v" : "Z"))]
#[cfg_attr(rr, rr::returns("v"))]
#[inline(always)]
pub unsafe fn load32_raw(ptr: *const u8, at: usize) -> u32 {
    // SAFETY: caller guarantees `at + 4 ≤ n` initialized bytes at `ptr`.
    let raw = unsafe { core::ptr::read_unaligned(ptr.add(at).cast::<u32>()) };
    u32::from_le(raw)
}

/// # Safety
/// `ptr` is valid for reads of `n ≥ at + 2` initialized bytes.
#[cfg_attr(rr, rr::params("l" : "loc", "n" : "Z", "i" : "Z", "vs"))]
#[cfg_attr(rr, rr::args("l", "i"))]
#[cfg_attr(rr, rr::requires(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))"))]
#[cfg_attr(rr, rr::requires("(0 ≤ i)%Z"))]
#[cfg_attr(rr, rr::requires("(i + 2 ≤ n)%Z"))]
#[cfg_attr(rr, rr::requires("(n ≤ MaxInt ISize)%Z"))]
#[cfg_attr(rr, rr::ensures(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))"))]
#[cfg_attr(rr, rr::exists("v" : "Z"))]
#[cfg_attr(rr, rr::returns("v"))]
#[inline(always)]
pub unsafe fn load16_raw(ptr: *const u8, at: usize) -> u16 {
    // SAFETY: caller guarantees `at + 2 ≤ n` initialized bytes at `ptr`.
    let raw = unsafe { core::ptr::read_unaligned(ptr.add(at).cast::<u16>()) };
    u16::from_le(raw)
}

/// # Safety
/// `ptr` is valid for reads of `n > at` initialized bytes.
#[cfg_attr(rr, rr::params("l" : "loc", "n" : "Z", "i" : "Z", "vs"))]
#[cfg_attr(rr, rr::args("l", "i"))]
#[cfg_attr(rr, rr::requires(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))"))]
#[cfg_attr(rr, rr::requires("(0 ≤ i)%Z"))]
#[cfg_attr(rr, rr::requires("(i + 1 ≤ n)%Z"))]
#[cfg_attr(rr, rr::requires("(n ≤ MaxInt ISize)%Z"))]
#[cfg_attr(rr, rr::ensures(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))"))]
#[cfg_attr(rr, rr::exists("v" : "Z"))]
#[cfg_attr(rr, rr::returns("v"))]
#[inline(always)]
pub unsafe fn load8_raw(ptr: *const u8, at: usize) -> u8 {
    // SAFETY: caller guarantees `at + 1 ≤ n` initialized bytes at `ptr`.
    // `read_unaligned::<u8>` is identical to `read::<u8>` (alignment 1) and
    // routes through the same shim contract as the wider loads.
    unsafe { core::ptr::read_unaligned(ptr.add(at)) }
}
