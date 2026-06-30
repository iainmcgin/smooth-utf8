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
    let want_str = core::str::from_utf8(b).ok();
    let want = want_str.is_some();
    assert_eq!(verify(b), want, "verify mismatch on {b:x?}");
    assert_eq!(from_utf8(b), want_str, "from_utf8 mismatch on {b:x?}");

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
        want_str,
        "SlackBuf::to_str mismatch on {b:x?}"
    );
}

#[test]
fn empty() {
    check(b"");
}

#[test]
#[allow(deprecated)]
fn to_str_alias() {
    assert_eq!(to_str(b"abc"), Some("abc"));
    assert_eq!(to_str(&[0xFF]), None);
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

/// `check`s two corruptions of an all-`'a'` buffer of length `len`: a lone
/// continuation byte at `pos` (invalid at any position), and a 2-byte scalar
/// (é) whose lead is at `pos` (truncated, hence invalid, when its tail does
/// not fit).
fn check_corruption_shapes(len: usize, pos: usize) {
    let mut v = std::vec![b'a'; len];
    v[pos] = 0xBF;
    check(&v);
    let mut v = std::vec![b'a'; len];
    v[pos] = 0xC3;
    if pos + 1 < len {
        v[pos + 1] = 0xA9;
    }
    check(&v);
}

/// A non-ASCII byte at every position of every length spanning the short
/// ladder (< 8), the last-8-byte window (1..=7 trailing), and the word/skip
/// loops — the edges of the overlapping-load tail handling.
#[test]
fn non_ascii_every_position() {
    for len in 1..=40 {
        for pos in 0..len {
            check_corruption_shapes(len, pos);
        }
    }
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
fn slackbuf_new_embedded_slack() {
    let buf = [0u8; SLACK + 3];
    let sb = SlackBuf::new_embedded_slack(&buf);
    assert_eq!(sb.payload_len(), 3);
}

#[test]
#[should_panic(expected = "buf must carry SLACK trailing bytes")]
fn slackbuf_new_embedded_slack_panics_on_short() {
    let _ = SlackBuf::new_embedded_slack(&[0u8; SLACK - 1]);
}

#[cfg(feature = "alloc")]
#[test]
fn slackbuf_new_add_slack() {
    let mut v = std::vec![b'a', b'b', b'c'];
    let sb = SlackBuf::new_add_slack(&mut v);
    assert_eq!(sb.payload_len(), 3);
    assert_eq!(sb.as_bytes().len(), 3 + SLACK);
    assert_eq!(sb.to_str(0..3), Some("abc"));
    // After `sb`'s last use the borrow on `v` ends and the padding is
    // observable.
    assert_eq!(v.len(), 3 + SLACK);
    assert_eq!(&v[3..], &[0u8; SLACK]);
}

#[cfg(feature = "alloc")]
#[test]
fn slackbuf_new_add_slack_empty() {
    let mut v: std::vec::Vec<u8> = std::vec::Vec::new();
    let sb = SlackBuf::new_add_slack(&mut v);
    assert_eq!(sb.payload_len(), 0);
    assert!(sb.verify(0..0));
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

/// Pins the little-endian pack contract of the four leaf loads. The contract
/// is `external_body` under Verus and deliberately out of RefinedRust's scope
/// (RR proves the loads in-bounds, not their value), so a value-level bug —
/// e.g. a `from_le`/`from_be` mixup — is invisible to both provers and would
/// otherwise be caught only by differential fuzzing.
#[test]
fn leaf_loads_are_little_endian() {
    let buf: [u8; 9] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09];
    // SAFETY: every `at` below satisfies `at + width <= buf.len()`.
    unsafe {
        assert_eq!(load64(&buf, 0), 0x0807_0605_0403_0201);
        assert_eq!(load64(&buf, 1), 0x0908_0706_0504_0302);
        assert_eq!(load32(&buf, 0), 0x0403_0201);
        assert_eq!(load32(&buf, 5), 0x0908_0706);
        assert_eq!(load16(&buf, 0), 0x0201);
        assert_eq!(load16(&buf, 7), 0x0908);
        assert_eq!(load8(&buf, 0), 0x01);
        assert_eq!(load8(&buf, 8), 0x09);
    }
}

/// Asserts `ascii_skip::skip`'s documented contract on one input: the result
/// `q` is in `[start, end]`, everything before it is ASCII, and it stops only
/// at the end, within a STEP-sized tail, or at a block containing a
/// non-ASCII byte. Callers must pass `start <= end <= buf.len()`.
fn check_skip_contract(buf: &[u8], start: usize, end: usize) {
    assert!(start <= end && end <= buf.len());
    // SAFETY: asserted just above.
    let q = unsafe { ascii_skip::skip(buf, start, end) };
    assert!(start <= q && q <= end, "skip left [start, end]: {q}");
    assert!(
        buf[start..q].iter().all(|&b| b < 0x80),
        "skip jumped a non-ASCII byte: {buf:x?} start={start} q={q}"
    );
    if end - q >= ascii_skip::STEP {
        assert!(
            buf[q..q + ascii_skip::STEP].iter().any(|&b| b >= 0x80),
            "skip stopped early on an all-ASCII block: {buf:x?} start={start} q={q}"
        );
    }
}

/// The AVX2 and NEON `skip` variants are outside the Verus proof and trusted
/// to satisfy the verified SWAR variant's contract; `verify_impl`'s proof
/// transfers to those builds only if they do. This pins the contract directly
/// on whichever variant the build selected, across both sides of every
/// STEP-block boundary.
#[test]
fn ascii_skip_contract_sweep() {
    for len in 0..=72 {
        // All-ASCII, and a non-ASCII byte at every position.
        for pos in 0..=len {
            let mut v = std::vec![b'a'; len];
            if pos < len {
                v[pos] = 0xC3;
            }
            // `end == buf.len()`, as the plain `verify` path calls it...
            for start in [0, 1, len / 2, len] {
                check_skip_contract(&v, start.min(len), len);
            }
            // ...and `end < buf.len()` with non-ASCII bytes past `end`, as
            // the slack path calls it — catches a variant comparing against
            // `buf.len()` instead of `end`.
            v.extend_from_slice(&[0xFF; SLACK]);
            check_skip_contract(&v, 0, len);
        }
    }
}

#[cfg(feature = "simdutf8")]
mod long_threshold {
    use super::*;

    /// Deterministic sweep of the simdutf8 delegation boundary: every length
    /// in `LONG_THRESHOLD - 1 ..= LONG_THRESHOLD + 1` (so both the verified
    /// path's last length and the delegated path's first), with an invalid
    /// byte at the first, middle, and last position, plus the all-valid and
    /// straddling-multibyte shapes. `check` routes each input through all
    /// five public entry points.
    #[test]
    fn boundary_lengths() {
        for len in LONG_THRESHOLD - 1..=LONG_THRESHOLD + 1 {
            check(&std::vec![b'a'; len]);
            for pos in [0, len / 2, len - 1] {
                check_corruption_shapes(len, pos);
            }
            // A 4-byte scalar ending exactly at the boundary length.
            let mut v = std::vec![b'a'; len - 4];
            v.extend_from_slice("🌍".as_bytes());
            check(&v);
        }
    }

    /// The delegation in `verify_with_slack` compares `end - start` (not
    /// `buf.len()`) against the threshold and must slice exactly `[start,
    /// end)` — the prefix and the slack region are filled with non-ASCII
    /// garbage so any off-by-one in the delegated slicing flips the result.
    #[test]
    fn boundary_lengths_with_offset_range() {
        for body_len in LONG_THRESHOLD - 1..=LONG_THRESHOLD + 1 {
            for start in [1, 7, SLACK + 5] {
                for &(bad_at, want) in
                    &[(None, true), (Some(0), false), (Some(body_len - 1), false)]
                {
                    let mut buf = std::vec![0xFFu8; start];
                    buf.extend_from_slice(&std::vec![b'a'; body_len]);
                    if let Some(at) = bad_at {
                        buf[start + at] = 0xBF;
                    }
                    let end = buf.len();
                    buf.extend_from_slice(&[0xFF; SLACK]);
                    // SAFETY: `start <= end` and `end + SLACK == buf.len()`.
                    let got = unsafe { verify_with_slack(&buf, start..end) };
                    assert_eq!(
                        got, want,
                        "start={start} body_len={body_len} bad_at={bad_at:?}"
                    );
                    let sb = SlackBuf::new(&buf).unwrap();
                    assert_eq!(sb.verify(start..end), want);
                }
            }
        }
    }
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

    /// Random-input pinning of the `ascii_skip::skip` contract (see
    /// `ascii_skip_contract_sweep`). Bytes are biased 15:1 toward ASCII so
    /// the scan regularly clears several STEP blocks before hitting a
    /// non-ASCII byte.
    #[test]
    fn prop_ascii_skip_contract(
        buf in proptest::collection::vec(
            prop_oneof![15 => 0u8..0x80u8, 1 => 0x80u8..=0xFFu8],
            0..256,
        ),
        start in any::<proptest::sample::Index>(),
    ) {
        // `check_skip_contract` uses plain `assert!`; proptest treats the
        // panic as a failure and shrinks normally.
        check_skip_contract(&buf, start.index(buf.len() + 1), buf.len());
    }
}
