//! Codegen probes for `SlackBuf` vs `verify_with_slack` (issue #5). Build with:
//!   cargo rustc --release --example asm_probe_slackbuf -- --emit asm \
//!     -C opt-level=3 -C codegen-units=1 -C llvm-args=-x86-asm-syntax=intel
//! and diff the `probe_*` bodies.
#![allow(clippy::missing_safety_doc, clippy::pedantic, clippy::nursery)]

use core::ops::Range;
use smoothutf8::{verify_with_slack, SlackBuf};

/// Baseline: today's `unsafe` entry point. 0 runtime checks (release).
#[no_mangle]
#[inline(never)]
pub unsafe fn probe_unsafe(buf: &[u8], range: Range<usize>) -> bool {
    verify_with_slack(buf, range)
}

/// Safe path: `SlackBuf` by value (it's `Copy`); per-call asserts on `range`.
#[no_mangle]
#[inline(never)]
pub fn probe_slackbuf(sb: SlackBuf<'_>, range: Range<usize>) -> bool {
    sb.verify(range)
}

/// Q3 sketch: fixed-width over-read primitive on the same type.
#[no_mangle]
#[inline(never)]
pub fn probe_le_u32(sb: SlackBuf<'_>, at: usize) -> u32 {
    sb.le_u32(at)
}

/// Realistic caller: construct once, verify two fields. Shows whether LLVM
/// hoists `payload_len()` out of the per-field check.
#[no_mangle]
#[inline(never)]
pub fn probe_two_fields(buf: &[u8], a: Range<usize>, b: Range<usize>) -> bool {
    let sb = SlackBuf::new(buf).unwrap();
    sb.verify(a) & sb.verify(b)
}

fn main() {}
