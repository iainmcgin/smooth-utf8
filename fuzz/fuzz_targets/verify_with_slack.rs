#![no_main]
//! Differential fuzz: `verify_with_slack` over an arbitrary sub-range of an
//! arbitrary buffer (with `SLACK` bytes appended) must agree with
//! `core::str::from_utf8` on that sub-range, and with `verify` on the same
//! bytes.

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use smoothutf8::SLACK;

#[derive(Arbitrary, Debug)]
struct Case {
    body: Vec<u8>,
    start: u16,
    len: u16,
    slack_fill: u8,
}

fuzz_target!(|c: Case| {
    let body_len = c.body.len();
    let start = (c.start as usize).min(body_len);
    let end = start.saturating_add(c.len as usize).min(body_len);
    let logical = &c.body[start..end];

    let std = core::str::from_utf8(logical).is_ok();
    let safe = smoothutf8::verify(logical);
    assert_eq!(safe, std, "verify vs std on {logical:x?}");

    let mut buf = c.body.clone();
    buf.resize(body_len + SLACK, c.slack_fill);
    // SAFETY: `start <= end` and `end + SLACK == buf.len()` by construction
    // (`end <= body_len` and we appended exactly `SLACK` bytes).
    let slack = unsafe { smoothutf8::verify_with_slack(&buf, start..end) };
    assert_eq!(slack, std, "verify_with_slack vs std on {logical:x?}");
});
