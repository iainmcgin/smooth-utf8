//! Probes for the safe-vs-slack 8 B inversion. Build with:
//!   cargo rustc --release --example asm_probe_inversion -- --emit asm \
//!     -C opt-level=3 -C codegen-units=1 -C llvm-args=-x86-asm-syntax=intel
#![allow(clippy::missing_safety_doc, clippy::pedantic, clippy::nursery)]

use core::ops::Range;

#[no_mangle]
#[inline(never)]
pub fn probe_safe_8(buf: &[u8]) -> bool {
    smoothutf8::verify(buf)
}

#[no_mangle]
#[inline(never)]
pub unsafe fn probe_slack_8(buf: &[u8], range: Range<usize>) -> bool {
    smoothutf8::verify_with_slack(buf, range)
}

/// Same call shape as `probe_safe_8` but routed through the slack path with a
/// pre-padded slice — isolates codegen vs harness.
#[no_mangle]
#[inline(never)]
pub unsafe fn probe_slack_as_slice(padded: &[u8]) -> bool {
    smoothutf8::verify_with_slack(padded, 0..padded.len() - smoothutf8::SLACK)
}

fn main() {}
