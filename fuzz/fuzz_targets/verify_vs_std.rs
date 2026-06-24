#![no_main]
//! Differential fuzz: `smoothutf8::verify` must agree with
//! `core::str::from_utf8` on every byte sequence.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let ours = smoothutf8::verify(data);
    let std = core::str::from_utf8(data).is_ok();
    assert_eq!(ours, std, "disagreement on {data:x?}");
});
