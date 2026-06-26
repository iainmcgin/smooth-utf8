extern crate std;

use super::*;
use proptest::prelude::*;
use std::vec::Vec;

fn std_ok(b: &[u8]) -> bool {
    core::str::from_utf8(b).is_ok()
}

/// Wraps `b` in a buffer with `SLACK` trailing bytes so `verify_with_slack`
/// can be exercised on arbitrary input.
fn with_slack(b: &[u8]) -> (Vec<u8>, Range<usize>) {
    let mut v = Vec::with_capacity(b.len() + SLACK);
    v.extend_from_slice(b);
    // Non-zero garbage in the slack region, to catch any failure to mask.
    v.extend_from_slice(&[0xA5; SLACK]);
    (v, 0..b.len())
}

fn check(b: &[u8]) {
    let want = std_ok(b);
    assert_eq!(verify(b), want, "verify mismatch on {b:x?}");

    let (buf, range) = with_slack(b);
    // SAFETY: `with_slack` guarantees `range.end + SLACK == buf.len()`.
    let got = unsafe { verify_with_slack(&buf, range.clone()) };
    assert_eq!(got, want, "verify_with_slack mismatch on {b:x?}");

    let sb = SlackBuf::new(&buf).unwrap();
    assert_eq!(
        sb.verify(range.clone()),
        want,
        "SlackBuf::verify mismatch on {b:x?}"
    );
    assert_eq!(
        sb.to_str(range),
        if want {
            core::str::from_utf8(b).ok()
        } else {
            None
        },
        "SlackBuf::to_str mismatch on {b:x?}",
    );
}

#[test]
fn empty() {
    check(b"");
}

#[test]
fn ascii_short() {
    // Exercise every remainder length up to and past the widest ASCII-skip
    // step (32 bytes under AVX2).
    for n in 0..=40 {
        check(&[b'a'; 40][..n]);
    }
}

#[test]
fn ascii_long() {
    check(&[b'x'; 4096]);
    check(&[0x7f; 33]);
}

#[test]
fn boundary_scalars() {
    for s in [
        "\u{0080}",  // first 2-byte
        "\u{07FF}",  // last 2-byte
        "\u{0800}",  // first 3-byte
        "\u{D7FF}",  // last before surrogates
        "\u{E000}",  // first after surrogates
        "\u{FFFD}",  // replacement
        "\u{FFFF}",  // last 3-byte
        "\u{10000}", // first 4-byte
        "\u{7FFFF}",
        "\u{80000}",
        "\u{10FFFF}", // max scalar
    ] {
        check(s.as_bytes());
    }
}

#[test]
fn mixed() {
    check("hello, 世界! 🌍 done".as_bytes());
    check("naïve façade — résumé".as_bytes());
}

#[test]
fn overlong_rejected() {
    // Overlong encodings of U+0000, U+002F, U+0080, U+0800.
    for b in [
        &[0xC0, 0x80][..],
        &[0xC0, 0xAF],
        &[0xE0, 0x80, 0x80],
        &[0xE0, 0x82, 0x80],
        &[0xF0, 0x80, 0x80, 0x80],
        &[0xF0, 0x80, 0xA0, 0x80],
    ] {
        assert!(!std_ok(b));
        check(b);
    }
}

#[test]
fn surrogates_rejected() {
    // U+D800 and U+DFFF encoded as 3-byte sequences.
    for b in [&[0xED, 0xA0, 0x80][..], &[0xED, 0xBF, 0xBF]] {
        assert!(!std_ok(b));
        check(b);
    }
}

#[test]
fn out_of_range_rejected() {
    // U+110000 encoded as 4 bytes.
    check(&[0xF4, 0x90, 0x80, 0x80]);
    // 0xF5.. lead bytes (would encode > U+13FFFF).
    check(&[0xF5, 0x80, 0x80, 0x80]);
}

#[test]
fn truncated_rejected() {
    for b in [
        &[0xC2][..],
        &[0xE2, 0x82],
        &[0xF0, 0x9F, 0x98],
        &[b'a', 0xE2, 0x82],
    ] {
        assert!(!std_ok(b));
        check(b);
    }
}

#[test]
fn bad_continuation_rejected() {
    for b in [
        &[0x80][..],
        &[0xBF],
        &[0xC2, 0x20],
        &[0xE2, 0x82, 0x20],
        &[0xE2, 0x20, 0xAC],
        &[0xF0, 0x9F, 0x20, 0x80],
    ] {
        assert!(!std_ok(b));
        check(b);
    }
}

#[test]
fn lone_lead_bytes_rejected() {
    // 0xFE and 0xFF are never valid lead bytes.
    check(&[0xFE]);
    check(&[0xFF]);
    check(&[0xF8, 0x80, 0x80, 0x80, 0x80]);
}

#[test]
fn straddle_chunk_boundary() {
    // 7 ASCII bytes then a 3-byte rune: lead byte is at offset 7, so the rune
    // straddles the first 8-byte chunk.
    let mut v = std::vec![b'a'; 7];
    v.extend_from_slice("€".as_bytes());
    check(&v);
    // And an invalid version of the same shape.
    let mut v = std::vec![b'a'; 7];
    v.extend_from_slice(&[0xE2, 0x82, 0x20]);
    check(&v);
}

#[test]
fn slack_garbage_is_masked() {
    // 3 ASCII bytes; slack is non-ASCII garbage. Result must be `true`.
    let mut buf = std::vec![b'a', b'b', b'c'];
    buf.extend_from_slice(&[0xFF; SLACK]);
    // SAFETY: `3 + SLACK == buf.len()`.
    assert!(unsafe { verify_with_slack(&buf, 0..3) });
}

#[test]
fn slackbuf_new_rejects_short() {
    assert!(SlackBuf::new(&[0u8; SLACK - 1]).is_none());
    assert!(SlackBuf::new(&[0u8; SLACK]).is_some());
}

#[test]
fn slackbuf_payload_len_and_as_bytes() {
    let buf = [0u8; SLACK + 5];
    let sb = SlackBuf::new(&buf).unwrap();
    assert_eq!(sb.payload_len(), 5);
    assert_eq!(sb.as_bytes().len(), buf.len());
}

#[test]
fn slackbuf_le_u32() {
    let mut buf = [0u8; SLACK + 4];
    buf[0..4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    buf[4..8].copy_from_slice(&0xCAFE_1234u32.to_le_bytes());
    let sb = SlackBuf::new(&buf).unwrap();
    assert_eq!(sb.le_u32(0), 0xDEAD_BEEF);
    // `at == payload_len()` is the boundary: reads entirely from the slack
    // region, which is in-bounds and well-defined.
    assert_eq!(sb.le_u32(4), 0xCAFE_1234);
}

#[test]
#[should_panic(expected = "range.start <= range.end")]
#[allow(clippy::reversed_empty_ranges)] // exercising the panic path
fn slackbuf_verify_panics_on_inverted_range() {
    let buf = [0u8; SLACK + 4];
    let _ = SlackBuf::new(&buf).unwrap().verify(2..1);
}

#[test]
#[should_panic(expected = "range.end <= self.payload_len()")]
fn slackbuf_verify_panics_on_oob_end() {
    let buf = [0u8; SLACK + 4];
    let _ = SlackBuf::new(&buf).unwrap().verify(0..5);
}

#[test]
#[should_panic(expected = "at <= self.payload_len()")]
fn slackbuf_le_u32_panics_on_oob() {
    let buf = [0u8; SLACK + 4];
    let _ = SlackBuf::new(&buf).unwrap().le_u32(5);
}

#[cfg(not(miri))] // proptest needs `-Zmiri-disable-isolation`; table tests cover miri
proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn prop_matches_std_on_random_bytes(b in proptest::collection::vec(any::<u8>(), 0..512)) {
        check(&b);
    }

    #[test]
    fn prop_accepts_all_valid_utf8(s in "\\PC{0,256}") {
        check(s.as_bytes());
    }

    #[test]
    fn prop_range_offset(
        prefix in proptest::collection::vec(any::<u8>(), 0..32),
        body in proptest::collection::vec(any::<u8>(), 0..128),
    ) {
        let want = std_ok(&body);
        let mut buf = prefix;
        let start = buf.len();
        buf.extend_from_slice(&body);
        let end = buf.len();
        buf.extend_from_slice(&[0x5A; SLACK]);
        // SAFETY: `end + SLACK == buf.len()` and `start <= end`.
        let got = unsafe { verify_with_slack(&buf, start..end) };
        prop_assert_eq!(got, want);
        let sb = SlackBuf::new(&buf).unwrap();
        prop_assert_eq!(sb.verify(start..end), want);
    }
}
