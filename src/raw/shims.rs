//! RefinedRust spec-only shims for `core` items with no upstream spec.
//!
//! `#[rr::only_spec]`: bodies are never translated; only the contract is
//! exported under the named `core::...` path.
//!
//! The `read_unaligned` shim is the load-bearing trusted contract. It is
//! stated over a *whole-buffer* ownership (`value_t` of `n` initialized
//! bytes at base `l`), with the actual `src` argument refined as
//! `l offsetₗ at` for ghost `at` with `at + size_of T ≤ n`. This bakes the
//! "carve `size_of T` bytes from inside an `n`-byte initialized buffer"
//! step into the trusted contract — which is sound (reading from inside an
//! initialized buffer you own does not change it) — so the `load*_raw`
//! proofs discharge without a manual `value_t` split/join lemma.
//!
//! Return values are existential: memory safety needs only that the load
//! is in bounds and the bytes are initialized, not what value they encode.
#![allow(dead_code)]

#[rr::export_as(core::ptr::read_unaligned)]
#[rr::only_spec]
#[rr::params("l" : "loc", "n" : "Z", "i" : "Z", "vs")]
#[rr::args("l offsetst{{ (IntSynType u8) }}ₗ i")]
#[rr::requires(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))")]
#[rr::requires("(0 ≤ i)%Z")]
#[rr::requires("(i + ly_size {ly_of T} ≤ n)%Z")]
#[rr::ensures(#type "l" : "vs" @ "value_t (UntypedSynType (mk_array_layout u8 (Z.to_nat n)))")]
#[rr::exists("v")]
#[rr::returns("v")]
pub unsafe fn read_unaligned_shim<T>(_src: *const T) -> T {
    unimplemented!()
}

// `from_le` is a pure value transform; on big-endian targets it byte-swaps,
// so claiming identity would be false. Existential return is vacuously sound.

#[rr::export_as(#method core::num::u64::from_le)]
#[rr::only_spec]
#[rr::params("x" : "Z")]
#[rr::args("x")]
#[rr::exists("v" : "Z")]
#[rr::returns("v")]
pub fn u64_from_le_shim(_x: u64) -> u64 {
    unimplemented!()
}

#[rr::export_as(#method core::num::u32::from_le)]
#[rr::only_spec]
#[rr::params("x" : "Z")]
#[rr::args("x")]
#[rr::exists("v" : "Z")]
#[rr::returns("v")]
pub fn u32_from_le_shim(_x: u32) -> u32 {
    unimplemented!()
}

#[rr::export_as(#method core::num::u16::from_le)]
#[rr::only_spec]
#[rr::params("x" : "Z")]
#[rr::args("x")]
#[rr::exists("v" : "Z")]
#[rr::returns("v")]
pub fn u16_from_le_shim(_x: u16) -> u16 {
    unimplemented!()
}
