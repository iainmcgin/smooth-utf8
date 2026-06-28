//! Functional-correctness spec and lemmas (Verus-only).
//!
//! The top-level spec [`is_valid_utf8`] is a direct transcription of Unicode
//! §3.9 Table 3-7, "Well-Formed UTF-8 Byte Sequences": a byte sequence is
//! valid iff it is a concatenation of rows of that table. A reader can audit
//! [`table_3_7`] against the standard line-by-line.
//!
//! Everything below the `// -- lemmas --` line is proof machinery connecting
//! that spec to the SWAR implementation's bit tricks.
#![allow(unused_imports, missing_docs)]

use verus_builtin_macros::*;
use vstd::prelude::*;

verus! {

// ============================================================================
// Spec: Unicode §3.9 Table 3-7
// ============================================================================

/// Continuation byte: `10xxxxxx`.
pub open spec fn cont(b: u8) -> bool { 0x80 <= b && b <= 0xBF }

/// Length of the well-formed UTF-8 byte sequence at the start of `s`, or 0
/// if no row of Table 3-7 matches. The nine arms are the nine rows of the
/// table, in order.
pub open spec fn table_3_7(s: Seq<u8>) -> int {
    if s.len() >= 1 && s[0] <= 0x7F { 1 }
    else if s.len() >= 2 && 0xC2 <= s[0] <= 0xDF && cont(s[1]) { 2 }
    else if s.len() >= 3 && s[0] == 0xE0 && 0xA0 <= s[1] && s[1] <= 0xBF && cont(s[2]) { 3 }
    else if s.len() >= 3 && 0xE1 <= s[0] <= 0xEC && cont(s[1]) && cont(s[2]) { 3 }
    else if s.len() >= 3 && s[0] == 0xED && 0x80 <= s[1] && s[1] <= 0x9F && cont(s[2]) { 3 }
    else if s.len() >= 3 && 0xEE <= s[0] <= 0xEF && cont(s[1]) && cont(s[2]) { 3 }
    else if s.len() >= 4 && s[0] == 0xF0 && 0x90 <= s[1] && s[1] <= 0xBF && cont(s[2]) && cont(s[3]) { 4 }
    else if s.len() >= 4 && 0xF1 <= s[0] <= 0xF3 && cont(s[1]) && cont(s[2]) && cont(s[3]) { 4 }
    else if s.len() >= 4 && s[0] == 0xF4 && 0x80 <= s[1] && s[1] <= 0x8F && cont(s[2]) && cont(s[3]) { 4 }
    else { 0 }
}

/// `s` is a concatenation of well-formed UTF-8 byte sequences.
pub open spec fn is_valid_utf8(s: Seq<u8>) -> bool
    decreases s.len()
{
    s.len() == 0 || {
        let n = table_3_7(s);
        &&& n > 0
        &&& is_valid_utf8(s.subrange(n, s.len() as int))
    }
}

// ============================================================================
// Derived spec helpers
// ============================================================================

/// Every byte in `s[i..j]` is ASCII (`< 0x80`).
pub open spec fn all_ascii(s: Seq<u8>, i: int, j: int) -> bool {
    forall |k: int| i <= k < j ==> #[trigger] s[k] < 0x80
}

/// Spec-level little-endian pack of 8 bytes from `s` at offset `i`.
pub open spec fn pack64(s: Seq<u8>, i: int) -> u64 {
      (s[i  ] as u64)
    | (s[i+1] as u64) << 8
    | (s[i+2] as u64) << 16
    | (s[i+3] as u64) << 24
    | (s[i+4] as u64) << 32
    | (s[i+5] as u64) << 40
    | (s[i+6] as u64) << 48
    | (s[i+7] as u64) << 56
}

/// Byte `j` (0..8) of a `u64`, little-endian.
pub open spec fn byte64(x: u64, j: int) -> u8 { ((x >> (8*j) as u64) & 0xFF) as u8 }

// ============================================================================
// Structural lemmas on is_valid_utf8
// ============================================================================

/// `table_3_7` only inspects the first `table_3_7(s)` bytes (≤ 4); a longer
/// sequence with the same prefix matches the same row.
pub proof fn lemma_table_prefix(s: Seq<u8>, t: Seq<u8>)
    requires
        s.len() <= t.len(),
        forall |k: int| 0 <= k < s.len() ==> s[k] == t[k],
        table_3_7(s) > 0,
    ensures table_3_7(t) == table_3_7(s)
{
    // The if-chain is fully concrete on s[0..4]; SMT discharges by case.
}

/// Concatenation of valid sequences is valid.
pub proof fn lemma_valid_concat(a: Seq<u8>, b: Seq<u8>)
    requires is_valid_utf8(a), is_valid_utf8(b),
    ensures is_valid_utf8(a + b)
    decreases a.len()
{
    if a.len() == 0 {
        assert((a + b) =~= b);
    } else {
        let n = table_3_7(a);
        assert(forall |k: int| 0 <= k < a.len() ==> #[trigger] (a + b)[k] == a[k]);
        lemma_table_prefix(a, a + b);
        lemma_valid_concat(a.subrange(n, a.len() as int), b);
        assert((a + b).subrange(n, (a + b).len() as int)
               =~= a.subrange(n, a.len() as int) + b);
    }
}

/// Splitting at a valid prefix leaves a valid suffix (UTF-8 prefix-freeness).
pub proof fn lemma_valid_split(s: Seq<u8>, n: int)
    requires
        0 <= n <= s.len(),
        is_valid_utf8(s),
        is_valid_utf8(s.subrange(0, n)),
    ensures is_valid_utf8(s.subrange(n, s.len() as int))
    decreases n
{
    if n == 0 {
        assert(s.subrange(0, s.len() as int) =~= s);
    } else {
        let p = s.subrange(0, n);
        let kp = table_3_7(p);
        assert(forall |i: int| 0 <= i < p.len() ==> p[i] == s[i]);
        lemma_table_prefix(p, s);
        let k = table_3_7(s);
        // k == kp; recurse on s.subrange(k, len), prefix length n - k.
        let rest = s.subrange(k, s.len() as int);
        assert(rest.subrange(0, n - k) =~= p.subrange(k, n));
        assert(p.subrange(k, n) =~= p.subrange(kp, p.len() as int));
        lemma_valid_split(rest, n - k);
        assert(rest.subrange(n - k, rest.len() as int)
               =~= s.subrange(n, s.len() as int));
    }
}

/// Index-range form of concat: validity of `[i,j)` and `[j,k)` gives `[i,k)`.
pub proof fn lemma_valid_join(s: Seq<u8>, i: int, j: int, k: int)
    requires
        0 <= i <= j <= k <= s.len(),
        is_valid_utf8(s.subrange(i, j)),
        is_valid_utf8(s.subrange(j, k)),
    ensures is_valid_utf8(s.subrange(i, k))
{
    lemma_valid_concat(s.subrange(i, j), s.subrange(j, k));
    assert(s.subrange(i, j) + s.subrange(j, k) =~= s.subrange(i, k));
}

/// `all_ascii` is closed under range concatenation.
pub proof fn lemma_ascii_extend(s: Seq<u8>, i: int, j: int, k: int)
    requires i <= j <= k, all_ascii(s, i, j), all_ascii(s, j, k),
    ensures all_ascii(s, i, k)
{}

/// All-ASCII ranges are valid UTF-8.
pub proof fn lemma_ascii_valid(s: Seq<u8>, i: int, j: int)
    requires 0 <= i <= j <= s.len(), all_ascii(s, i, j),
    ensures is_valid_utf8(s.subrange(i, j))
    decreases j - i
{
    let r = s.subrange(i, j);
    if r.len() > 0 {
        assert(r[0] == s[i]);
        assert(r.subrange(1, r.len() as int) =~= s.subrange(i + 1, j));
        lemma_ascii_valid(s, i + 1, j);
    }
}

// ============================================================================
// Bit-trick lemmas
// ============================================================================

/// `pack64`'s byte `j` is `s[i+j]`.
pub proof fn lemma_pack64_byte(s: Seq<u8>, i: int, j: int)
    requires 0 <= j < 8,
    ensures byte64(pack64(s, i), j) == s[i + j]
{
    let b0 = s[i] as u64; let b1 = s[i+1] as u64; let b2 = s[i+2] as u64;
    let b3 = s[i+3] as u64; let b4 = s[i+4] as u64; let b5 = s[i+5] as u64;
    let b6 = s[i+6] as u64; let b7 = s[i+7] as u64;
    let x = b0 | b1<<8 | b2<<16 | b3<<24 | b4<<32 | b5<<40 | b6<<48 | b7<<56;
    assert(pack64(s, i) == x);
    assert(
        b0<256 && b1<256 && b2<256 && b3<256 &&
        b4<256 && b5<256 && b6<256 && b7<256 ==>
        (x >> 0 ) & 0xFF == b0 && (x >> 8 ) & 0xFF == b1 &&
        (x >> 16) & 0xFF == b2 && (x >> 24) & 0xFF == b3 &&
        (x >> 32) & 0xFF == b4 && (x >> 40) & 0xFF == b5 &&
        (x >> 48) & 0xFF == b6 && (x >> 56) & 0xFF == b7
    ) by (bit_vector)
        requires x == b0 | b1<<8 | b2<<16 | b3<<24 | b4<<32 | b5<<40 | b6<<48 | b7<<56;
    if j == 0 {} else if j == 1 {} else if j == 2 {} else if j == 3 {}
    else if j == 4 {} else if j == 5 {} else if j == 6 {} else {}
}

/// `(a | b) & SIGN_BITS == 0` ⟺ all 16 bytes of the two words are ASCII.
pub proof fn lemma_signbits16(s: Seq<u8>, i: int)
    requires 0 <= i, i + 16 <= s.len(),
    ensures (((pack64(s, i) | pack64(s, i+8)) & 0x8080_8080_8080_8080u64) == 0)
            <==> all_ascii(s, i, i + 16)
{
    lemma_signbits8(s, i);
    lemma_signbits8(s, i + 8);
    let a = pack64(s, i); let b = pack64(s, i+8);
    assert(((a | b) & 0x8080_8080_8080_8080u64 == 0) <==>
           (a & 0x8080_8080_8080_8080u64 == 0 && b & 0x8080_8080_8080_8080u64 == 0))
        by (bit_vector);
}

/// `a & SIGN_BITS == 0` ⟺ all 8 bytes of `a` are ASCII.
pub proof fn lemma_signbits8(s: Seq<u8>, i: int)
    requires 0 <= i, i + 8 <= s.len(),
    ensures ((pack64(s, i) & 0x8080_8080_8080_8080u64) == 0) <==> all_ascii(s, i, i + 8)
{
    let b0 = s[i] as u64; let b1 = s[i+1] as u64; let b2 = s[i+2] as u64;
    let b3 = s[i+3] as u64; let b4 = s[i+4] as u64; let b5 = s[i+5] as u64;
    let b6 = s[i+6] as u64; let b7 = s[i+7] as u64;
    let x = b0 | b1<<8 | b2<<16 | b3<<24 | b4<<32 | b5<<40 | b6<<48 | b7<<56;
    assert(
        b0<256 && b1<256 && b2<256 && b3<256 &&
        b4<256 && b5<256 && b6<256 && b7<256 ==>
        ((x & 0x8080_8080_8080_8080u64 == 0) <==>
         (b0<128 && b1<128 && b2<128 && b3<128 &&
          b4<128 && b5<128 && b6<128 && b7<128))
    ) by (bit_vector)
        requires x == b0 | b1<<8 | b2<<16 | b3<<24 | b4<<32 | b5<<40 | b6<<48 | b7<<56;
    assert(all_ascii(s, i, i+8) <==>
        (s[i]<128 && s[i+1]<128 && s[i+2]<128 && s[i+3]<128 &&
         s[i+4]<128 && s[i+5]<128 && s[i+6]<128 && s[i+7]<128)) by {
        if s[i]<128 && s[i+1]<128 && s[i+2]<128 && s[i+3]<128 &&
           s[i+4]<128 && s[i+5]<128 && s[i+6]<128 && s[i+7]<128 {
            assert forall |k: int| i <= k < i+8 implies #[trigger] s[k] < 0x80 by {
                if k == i {} else if k == i+1 {} else if k == i+2 {} else if k == i+3 {}
                else if k == i+4 {} else if k == i+5 {} else if k == i+6 {} else {}
            };
        }
    };
}

/// Spec-level little-endian pack of 4 bytes from `s` at offset `i`.
pub open spec fn pack32(s: Seq<u8>, i: int) -> u32 {
      (s[i  ] as u32)
    | (s[i+1] as u32) << 8
    | (s[i+2] as u32) << 16
    | (s[i+3] as u32) << 24
}

/// Spec-level little-endian pack of 2 bytes from `s` at offset `i`.
pub open spec fn pack16(s: Seq<u8>, i: int) -> u16 {
    (s[i] as u16) | (s[i+1] as u16) << 8
}

/// `a & 0x8080_8080 == 0` ⟺ all 4 bytes of `a` are ASCII.
pub proof fn lemma_signbits4(s: Seq<u8>, i: int)
    requires 0 <= i, i + 4 <= s.len(),
    ensures ((pack32(s, i) & 0x8080_8080u32) == 0) <==> all_ascii(s, i, i + 4)
{
    let b0 = s[i] as u32; let b1 = s[i+1] as u32;
    let b2 = s[i+2] as u32; let b3 = s[i+3] as u32;
    let x = b0 | b1<<8 | b2<<16 | b3<<24;
    assert(
        b0<256 && b1<256 && b2<256 && b3<256 ==>
        ((x & 0x8080_8080u32 == 0) <==> (b0<128 && b1<128 && b2<128 && b3<128))
    ) by (bit_vector)
        requires x == b0 | b1<<8 | b2<<16 | b3<<24;
    assert(all_ascii(s, i, i+4) <==>
        (s[i]<128 && s[i+1]<128 && s[i+2]<128 && s[i+3]<128)) by {
        if s[i]<128 && s[i+1]<128 && s[i+2]<128 && s[i+3]<128 {
            assert forall |k: int| i <= k < i+4 implies #[trigger] s[k] < 0x80 by {
                if k == i {} else if k == i+1 {} else if k == i+2 {} else {}
            };
        }
    };
}

/// `a & 0x8080 == 0` ⟺ both bytes of `a` are ASCII.
pub proof fn lemma_signbits2(s: Seq<u8>, i: int)
    requires 0 <= i, i + 2 <= s.len(),
    ensures ((pack16(s, i) & 0x8080u16) == 0) <==> all_ascii(s, i, i + 2)
{
    let b0 = s[i] as u16; let b1 = s[i+1] as u16;
    let x = b0 | b1<<8;
    assert(
        b0<256 && b1<256 ==>
        ((x & 0x8080u16 == 0) <==> (b0<128 && b1<128))
    ) by (bit_vector)
        requires x == b0 | b1<<8;
    assert(all_ascii(s, i, i+2) <==> (s[i]<128 && s[i+1]<128)) by {
        if s[i]<128 && s[i+1]<128 {
            assert forall |k: int| i <= k < i+2 implies #[trigger] s[k] < 0x80 by {
                if k == i {} else {}
            };
        }
    };
}

// ============================================================================
// `trailing_zeros` on `bytes & SIGN_BITS-mask` ↔ first non-ASCII byte
// ============================================================================

use vstd::std_specs::bits::{u64_trailing_zeros, axiom_u64_trailing_zeros};

/// SIGN_BITS shifted to mask the low `n` bytes' high bits.
pub open spec fn sign_mask(n: int) -> u64 {
    (0x8080_8080_8080_8080u64 >> ((8 - n) * 8) as u64)
}

/// `sign_mask(n)` has exactly bits `7, 15, ..., 8n-1` set.
pub proof fn lemma_sign_mask_bits(n: int)
    requires 1 <= n <= 8,
    ensures forall |j: u64| #![trigger (sign_mask(n) >> j)] j < 64 ==>
        (((sign_mask(n) >> j) & 1 == 1) <==> (j < 8*n && (j & 7) == 7)),
{
    let mask = sign_mask(n);
    // 8-way case split: mask is concrete in each branch.
    let sh = ((8 - n) * 8) as u64;
    assert(0x8080_8080_8080_8080u64 >> 56u64 == 0x0000_0000_0000_0080u64
        && 0x8080_8080_8080_8080u64 >> 48u64 == 0x0000_0000_0000_8080u64
        && 0x8080_8080_8080_8080u64 >> 40u64 == 0x0000_0000_0080_8080u64
        && 0x8080_8080_8080_8080u64 >> 32u64 == 0x0000_0000_8080_8080u64
        && 0x8080_8080_8080_8080u64 >> 24u64 == 0x0000_0080_8080_8080u64
        && 0x8080_8080_8080_8080u64 >> 16u64 == 0x0000_8080_8080_8080u64
        && 0x8080_8080_8080_8080u64 >>  8u64 == 0x0080_8080_8080_8080u64
        && 0x8080_8080_8080_8080u64 >>  0u64 == 0x8080_8080_8080_8080u64) by (bit_vector);
    if n == 1 { assert(sh == 56); } else if n == 2 { assert(sh == 48); }
    else if n == 3 { assert(sh == 40); } else if n == 4 { assert(sh == 32); }
    else if n == 5 { assert(sh == 24); } else if n == 6 { assert(sh == 16); }
    else if n == 7 { assert(sh == 8); } else { assert(sh == 0); }
    assert forall |j: u64| j < 64 implies
        ((#[trigger] (mask >> j)) & 1 == 1) <==> (j < 8*n && (j & 7) == 7) by {
        assert((mask == 0x0000_0000_0000_0080u64 ==> ((((mask>>j)&1)==1) <==> (j<8  && (j&7)==7)))
            && (mask == 0x0000_0000_0000_8080u64 ==> ((((mask>>j)&1)==1) <==> (j<16 && (j&7)==7)))
            && (mask == 0x0000_0000_0080_8080u64 ==> ((((mask>>j)&1)==1) <==> (j<24 && (j&7)==7)))
            && (mask == 0x0000_0000_8080_8080u64 ==> ((((mask>>j)&1)==1) <==> (j<32 && (j&7)==7)))
            && (mask == 0x0000_0080_8080_8080u64 ==> ((((mask>>j)&1)==1) <==> (j<40 && (j&7)==7)))
            && (mask == 0x0000_8080_8080_8080u64 ==> ((((mask>>j)&1)==1) <==> (j<48 && (j&7)==7)))
            && (mask == 0x0080_8080_8080_8080u64 ==> ((((mask>>j)&1)==1) <==> (j<56 && (j&7)==7)))
            && (mask == 0x8080_8080_8080_8080u64 ==> ((((mask>>j)&1)==1) <==> (j<64 && (j&7)==7))))
            by (bit_vector);
    };
}

/// Given a tail-load `bytes` whose first `n` bytes are `buf[at..at+n]`, and
/// `ascii = (bytes & sign_mask(n)).trailing_zeros() / 8`:
/// - if `ascii < 8`, then `ascii < n`, `buf[at..at+ascii]` are all ASCII,
///   and `buf[at+ascii] >= 0x80`;
/// - if `ascii == 8`, then `buf[at..at+n]` are all ASCII.
pub proof fn lemma_tz_ascii(buf: Seq<u8>, at: int, n: int, bytes: u64)
    requires
        1 <= n <= 8,
        0 <= at, at + n <= buf.len(),
        forall |j: int| 0 <= j < n ==> #[trigger] byte64(bytes, j) == buf[at + j],
    ensures ({
        let x = bytes & sign_mask(n);
        let ascii = u64_trailing_zeros(x) as int / 8;
        &&& 0 <= ascii <= 8
        &&& (ascii < 8 ==> ascii < n && all_ascii(buf, at, at + ascii)
                                      && buf[at + ascii] >= 0x80)
        &&& (ascii == 8 ==> all_ascii(buf, at, at + n))
    })
{
    let nn = n as u64;
    let mask = sign_mask(n);
    let x = bytes & mask;
    axiom_u64_trailing_zeros(x);
    let tz = u64_trailing_zeros(x) as u64;
    let ascii = tz as int / 8;
    assert(0 <= ascii <= 8);
    assert((x == 0) <==> (ascii == 8));
    // Bit `8j+7` of mask is set iff `j < n`; all other bits of mask are clear.
    lemma_sign_mask_bits(n);
    assert(forall |j: u64| #![trigger (mask >> j)] j < 64 ==>
        (((mask >> j) & 1 == 1) <==> (j < (8*nn) && (j & 7) == 7)));
    if x == 0 {
        // ascii == 8; every byte j < n has bit 8j+7 of x clear ⇒ of bytes clear ⇒ < 0x80.
        assert forall |k: int| at <= k < at + n implies #[trigger] buf[k] < 0x80 by {
            let j = (k - at) as u64;
            assert(j == k - at && j < nn);
            let jj: u64 = (8*j + 7) as u64;
            assert(jj < 64 && jj & 7 == 7) by (bit_vector) requires j < 8, jj == 8*j+7;
            assert(jj < 8*nn);
            assert((mask >> jj) & 1 == 1);
            assert(j < 8 && x == 0u64 && (mask >> jj) & 1 == 1u64
                ==> (bytes >> (8*j)) & 0xFF < 0x80) by (bit_vector)
                requires x == bytes & mask, jj == 8*j+7;
            assert(byte64(bytes, j as int) == buf[at + j as int]);
        };
    } else {
        // tz < 64; bit tz of x is set ⇒ bit tz of mask is set ⇒ tz = 8m+7 with m < n.
        assert((x >> tz) & 1 == 1);
        assert((mask >> tz) & 1 == 1 && (bytes >> tz) & 1 == 1) by (bit_vector)
            requires x == bytes & mask, tz < 64, (x >> tz) & 1 == 1u64;
        assert(tz < 8*nn && (tz & 7) == 7);
        let m = (tz / 8);
        assert(tz < 64 && tz < 8*nn && (tz & 7) == 7 && nn <= 8 && m == tz / 8
            ==> tz == 8*m + 7 && m < nn && m < 8) by (bit_vector);
        assert(ascii == m as int);
        assert(m < nn && (m as int) < n);
        // buf[at+m] >= 0x80:
        let sm: u64 = (8*m) as u64;
        assert(m < 8 && (bytes >> tz) & 1 == 1u64 && tz == 8*m+7 && sm == 8*m
            ==> (bytes >> sm) & 0xFF >= 0x80 && (bytes >> sm) & 0xFF < 256) by (bit_vector);
        assert(sm == 8*m && (8 * (m as int)) as u64 == sm);
        assert(byte64(bytes, m as int) >= 0x80);
        assert(byte64(bytes, m as int) == buf[at + m as int]);
        assert(buf[at + ascii] >= 0x80);
        // bytes 0..m are ASCII:
        assert forall |k: int| at <= k < at + m implies #[trigger] buf[k] < 0x80 by {
            let j = (k - at) as u64;
            assert(j == k - at && j < m && j < nn);
            let jj: u64 = (8*j + 7) as u64;
            assert(j < m && tz == 8*m+7 ==> jj < tz) by (bit_vector)
                requires m < 8, jj == 8*j+7;
            assert((x >> jj) & 1 == 0u64);  // axiom: trailing bits are 0
            assert(jj < 64 && jj & 7 == 7) by (bit_vector) requires j < 8, jj == 8*j+7;
            assert(jj < 8*nn);
            assert((mask >> jj) & 1 == 1);
            assert(j < 8 && (x >> jj) & 1 == 0u64 && (mask >> jj) & 1 == 1u64
                ==> (bytes >> (8*j)) & 0xFF < 0x80) by (bit_vector)
                requires x == bytes & mask, jj == 8*j+7;
            assert(byte64(bytes, j as int) == buf[at + j as int]);
        };
        assert(all_ascii(buf, at, at + ascii));
    }
}

/// `bytes & sign_mask(n) == 0` ⟺ the first `n` bytes are ASCII.
pub proof fn lemma_mask_zero_ascii(buf: Seq<u8>, at: int, n: int, bytes: u64)
    requires
        1 <= n <= 8, 0 <= at, at + n <= buf.len(),
        forall |j: int| 0 <= j < n ==> #[trigger] byte64(bytes, j) == buf[at + j],
    ensures (bytes & sign_mask(n) == 0) <==> all_ascii(buf, at, at + n)
{
    lemma_tz_ascii(buf, at, n, bytes);
    axiom_u64_trailing_zeros(bytes & sign_mask(n));
    // ⟹ from lemma_tz_ascii (ascii==8 case). ⟸: if all ASCII but mask&bytes ≠ 0,
    // lemma_tz_ascii's ascii<8 case gives buf[at+ascii] >= 0x80, contradiction.
}

/// A valid all-ASCII prefix reduces validity of `[i,k)` to `[j,k)`.
pub proof fn lemma_ascii_prefix_iff(buf: Seq<u8>, i: int, j: int, k: int)
    requires 0 <= i <= j <= k <= buf.len(), all_ascii(buf, i, j),
    ensures is_valid_utf8(buf.subrange(i, k)) <==> is_valid_utf8(buf.subrange(j, k))
{
    lemma_ascii_valid(buf, i, j);
    if is_valid_utf8(buf.subrange(j, k)) {
        lemma_valid_join(buf, i, j, k);
    }
    if is_valid_utf8(buf.subrange(i, k)) {
        assert(buf.subrange(i, k).subrange(0, j - i) =~= buf.subrange(i, j));
        assert(buf.subrange(i, k).subrange(j - i, k - i) =~= buf.subrange(j, k));
        lemma_valid_split(buf.subrange(i, k), j - i);
    }
}


// ============================================================================
// Shift-DFA spec: Table-3-7-derived transition function
// ============================================================================
//
// State values are `index * 6` so that each is its own shift amount into the
// packed `ROW` word; the names follow the Unicode §3.9 partial-match position
// each represents.

pub spec const ST_REJECT: u64 = 0;
pub spec const ST_ACCEPT: u64 = 6;
pub spec const ST_C1: u64 = 12;
pub spec const ST_C2: u64 = 18;
pub spec const ST_E0: u64 = 24;
pub spec const ST_ED: u64 = 30;
pub spec const ST_F0: u64 = 36;
pub spec const ST_C3: u64 = 42;
pub spec const ST_F4: u64 = 48;

/// `s` is one of the nine reachable DFA states.
pub open spec fn is_state(s: u64) -> bool {
    ||| s == ST_REJECT ||| s == ST_ACCEPT ||| s == ST_C1
    ||| s == ST_C2 ||| s == ST_E0 ||| s == ST_ED
    ||| s == ST_F0 ||| s == ST_C3 ||| s == ST_F4
}

/// DFA transition, defined directly from Table 3-7 byte ranges. Each
/// non-ACCEPT state names the second-byte constraint of the rune in progress.
pub open spec fn spec_step(s: u64, b: u8) -> u64 {
    if s == ST_ACCEPT {
        if b <= 0x7F { ST_ACCEPT }
        else if 0xC2 <= b <= 0xDF { ST_C1 }
        else if b == 0xE0 { ST_E0 }
        else if b == 0xED { ST_ED }
        else if 0xE1 <= b <= 0xEF { ST_C2 }
        else if b == 0xF0 { ST_F0 }
        else if b == 0xF4 { ST_F4 }
        else if 0xF1 <= b <= 0xF3 { ST_C3 }
        else { ST_REJECT }
    }
    else if s == ST_C1 { if cont(b) { ST_ACCEPT } else { ST_REJECT } }
    else if s == ST_C2 { if cont(b) { ST_C1 } else { ST_REJECT } }
    else if s == ST_C3 { if cont(b) { ST_C2 } else { ST_REJECT } }
    else if s == ST_E0 { if 0xA0 <= b <= 0xBF { ST_C1 } else { ST_REJECT } }
    else if s == ST_ED { if 0x80 <= b <= 0x9F { ST_C1 } else { ST_REJECT } }
    else if s == ST_F0 { if 0x90 <= b <= 0xBF { ST_C2 } else { ST_REJECT } }
    else if s == ST_F4 { if 0x80 <= b <= 0x8F { ST_C2 } else { ST_REJECT } }
    else { ST_REJECT } // s == ST_REJECT (absorbing) or any non-state
}

/// Packed row word for byte `b`: bits `[6s..6s+6)` hold `spec_step(s, b)`.
/// These twelve constants are the distinct values of `lib::ROW`; the
/// compile-time `_CHECK_SPEC_ROW` assertion in `lib.rs` validates all 256.
pub open spec fn spec_row(b: u8) -> u64 {
    if b <= 0x7F { 0x0000_0000_0000_0180 }
    else if b <= 0x8F { 0x0012_4803_0030_6000 }
    else if b <= 0x9F { 0x0000_4923_0030_6000 }
    else if b <= 0xBF { 0x0000_4920_0C30_6000 }
    else if b <= 0xC1 { 0 }
    else if b <= 0xDF { 0x0000_0000_0000_0300 }
    else if b == 0xE0 { 0x0000_0000_0000_0600 }
    else if b <= 0xEC { 0x0000_0000_0000_0480 }
    else if b == 0xED { 0x0000_0000_0000_0780 }
    else if b <= 0xEF { 0x0000_0000_0000_0480 }
    else if b == 0xF0 { 0x0000_0000_0000_0900 }
    else if b <= 0xF3 { 0x0000_0000_0000_0A80 }
    else if b == 0xF4 { 0x0000_0000_0000_0C00 }
    else { 0 }
}

/// The shift-DFA transition `(spec_row(b) >> s) & 63` agrees with the
/// Table-3-7-derived [`spec_step`] on every reachable state. This is the
/// 9-state × 12-row exhaustive check that connects the bit encoding to the
/// semantic automaton.
pub proof fn lemma_row_step(s: u64, b: u8)
    requires is_state(s),
    ensures
        s & 63 == s,
        (spec_row(b) >> s) & 63 == spec_step(s, b),
        is_state(spec_step(s, b)),
{
    let r = spec_row(b);
    let st = is_state(s);
    assert(s < 64 ==> s & 63 == s) by (bit_vector);
    // Twelve distinct row constants. For each, a single bit-vector query
    // discharges `(r >> s) & 63` at every reachable `s` simultaneously by
    // case-splitting on the nine concrete shift amounts; the surrounding
    // byte-range branch lets `spec_step(s, b)` evaluate. The final
    // `lemma_row_cell` call lifts the per-branch fact to the postcondition.
    if b <= 0x7F {
        assert(st ==> (r>>s)&63 == (if s==6 {6u64} else {0})) by (bit_vector)
            requires r == 0x180u64,
                st == (s==0||s==6||s==12||s==18||s==24||s==30||s==36||s==42||s==48);
        if s==0 {} else if s==6 {} else if s==12 {} else if s==18 {} else if s==24 {}
        else if s==30 {} else if s==36 {} else if s==42 {} else {}
    } else if b <= 0x8F {
        assert(st ==> (r>>s)&63 == (if s==12 {6u64} else if s==18 {12}
            else if s==30 {12} else if s==42 {18} else if s==48 {18} else {0}))
            by (bit_vector) requires r == 0x0012_4803_0030_6000u64,
                st == (s==0||s==6||s==12||s==18||s==24||s==30||s==36||s==42||s==48);
        if s==0 {} else if s==6 {} else if s==12 {} else if s==18 {} else if s==24 {}
        else if s==30 {} else if s==36 {} else if s==42 {} else {}
    } else if b <= 0x9F {
        assert(st ==> (r>>s)&63 == (if s==12 {6u64} else if s==18 {12}
            else if s==30 {12} else if s==36 {18} else if s==42 {18} else {0}))
            by (bit_vector) requires r == 0x0000_4923_0030_6000u64,
                st == (s==0||s==6||s==12||s==18||s==24||s==30||s==36||s==42||s==48);
        if s==0 {} else if s==6 {} else if s==12 {} else if s==18 {} else if s==24 {}
        else if s==30 {} else if s==36 {} else if s==42 {} else {}
    } else if b <= 0xBF {
        assert(st ==> (r>>s)&63 == (if s==12 {6u64} else if s==18 {12}
            else if s==24 {12} else if s==36 {18} else if s==42 {18} else {0}))
            by (bit_vector) requires r == 0x0000_4920_0C30_6000u64,
                st == (s==0||s==6||s==12||s==18||s==24||s==30||s==36||s==42||s==48);
        if s==0 {} else if s==6 {} else if s==12 {} else if s==18 {} else if s==24 {}
        else if s==30 {} else if s==36 {} else if s==42 {} else {}
    } else {
        // Lead bytes (`C0..=FF`): only the ACCEPT slot (`>> 6`) is nonzero,
        // and that slot is exactly `spec_step(ACCEPT, b)`.
        assert(st ==> (r>>s)&63 == (if s==6 {(r>>6)&63} else {0})) by (bit_vector)
            requires st == (s==0||s==6||s==12||s==18||s==24||s==30||s==36||s==42||s==48),
                r==0u64 || r==0x300u64 || r==0x480u64 || r==0x600u64
                || r==0x780u64 || r==0x900u64 || r==0xA80u64 || r==0xC00u64;
        assert((0x000u64>>6)&63==0 && (0x300u64>>6)&63==12 && (0x480u64>>6)&63==18
            && (0x600u64>>6)&63==24 && (0x780u64>>6)&63==30 && (0x900u64>>6)&63==36
            && (0xA80u64>>6)&63==42 && (0xC00u64>>6)&63==48) by (bit_vector);
        if s==0 {} else if s==12 {} else if s==18 {} else if s==24 {}
        else if s==30 {} else if s==36 {} else if s==42 {} else if s==48 {}
        else {
            assert(s == 6);
            // Byte sub-split makes both `r` and `spec_step(ACCEPT, b)` concrete.
            if b <= 0xC1 {} else if b <= 0xDF {} else if b == 0xE0 {}
            else if b <= 0xEC {} else if b == 0xED {} else if b <= 0xEF {}
            else if b == 0xF0 {} else if b <= 0xF3 {} else if b == 0xF4 {} else {}
        }
    }
}

// ============================================================================
// DFA run: left fold of `spec_step`
// ============================================================================

/// State after consuming `s` starting from `st`.
pub open spec fn run(st: u64, s: Seq<u8>) -> u64
    decreases s.len()
{
    if s.len() == 0 { st }
    else { run(spec_step(st, s[0]), s.subrange(1, s.len() as int)) }
}

/// `run` is a left fold: `run(st, a ++ c) == run(run(st, a), c)`.
pub proof fn lemma_run_append(st: u64, a: Seq<u8>, c: Seq<u8>)
    ensures run(st, a + c) == run(run(st, a), c)
    decreases a.len()
{
    if a.len() == 0 {
        assert((a + c) =~= c);
    } else {
        assert((a + c)[0] == a[0]);
        assert((a + c).subrange(1, (a + c).len() as int)
               =~= a.subrange(1, a.len() as int) + c);
        lemma_run_append(spec_step(st, a[0]), a.subrange(1, a.len() as int), c);
    }
}

/// `REJECT` is absorbing.
pub proof fn lemma_run_reject(s: Seq<u8>)
    ensures run(ST_REJECT, s) == ST_REJECT
    decreases s.len()
{
    if s.len() > 0 { lemma_run_reject(s.subrange(1, s.len() as int)); }
}

/// `run` over an index range, extended by one byte on the right.
pub proof fn lemma_run_snoc(st: u64, buf: Seq<u8>, i: int, j: int)
    requires 0 <= i <= j < buf.len(),
    ensures run(st, buf.subrange(i, j + 1)) == spec_step(run(st, buf.subrange(i, j)), buf[j])
{
    let one = buf.subrange(j, j + 1);
    assert(buf.subrange(i, j) + one =~= buf.subrange(i, j + 1));
    lemma_run_append(st, buf.subrange(i, j), one);
    assert(one[0] == buf[j]);
    let tail = one.subrange(1, one.len() as int);
    assert(tail.len() == 0);
    let x = run(st, buf.subrange(i, j));
    assert(run(spec_step(x, buf[j]), tail) == spec_step(x, buf[j]));
    assert(run(x, one) == spec_step(x, buf[j]));
}

/// `run` over an index range, split at an interior point.
pub proof fn lemma_run_join(st: u64, buf: Seq<u8>, i: int, j: int, k: int)
    requires 0 <= i <= j <= k <= buf.len(),
    ensures run(st, buf.subrange(i, k)) == run(run(st, buf.subrange(i, j)), buf.subrange(j, k))
{
    assert(buf.subrange(i, j) + buf.subrange(j, k) =~= buf.subrange(i, k));
    lemma_run_append(st, buf.subrange(i, j), buf.subrange(j, k));
}

// ============================================================================
// `run(ACCEPT, s) == ACCEPT` ⟺ `is_valid_utf8(s)`
// ============================================================================

// The proof case-splits on the first byte's lead class and walks the DFA
// through the (at most four) bytes of the first sequence, tracking
// `run(ACCEPT, s[0..k])` via `lemma_run_snoc`. In every leaf, both sides
// either reduce to the same recursive instance on the suffix, or are
// determined (REJECT/non-ACCEPT vs. `table_3_7 == 0`).

/// If the DFA, started at `ACCEPT`, hits `REJECT` or a non-`ACCEPT` end of
/// input within the first sequence, the run over all of `s` is not `ACCEPT`.
proof fn dead(s: Seq<u8>, k: int)
    requires
        0 <= k <= s.len(),
        run(ST_ACCEPT, s.subrange(0, k)) == ST_REJECT
            || (s.len() == k && run(ST_ACCEPT, s.subrange(0, k)) != ST_ACCEPT),
    ensures run(ST_ACCEPT, s) != ST_ACCEPT
{
    assert(s.subrange(0, k) + s.subrange(k, s.len() as int) =~= s);
    lemma_run_append(ST_ACCEPT, s.subrange(0, k), s.subrange(k, s.len() as int));
    if run(ST_ACCEPT, s.subrange(0, k)) == ST_REJECT {
        lemma_run_reject(s.subrange(k, s.len() as int));
    } else {
        assert(s.subrange(k, s.len() as int).len() == 0);
    }
}

/// One Table-3-7 row consumed: the DFA is back at `ACCEPT` after `n` bytes,
/// so the recursive structure of [`run`] and [`is_valid_utf8`] coincide.
proof fn live(s: Seq<u8>, n: int)
    requires
        1 <= n <= s.len(),
        table_3_7(s) == n,
        run(ST_ACCEPT, s.subrange(0, n)) == ST_ACCEPT,
        // Inductive hypothesis on the suffix:
        (run(ST_ACCEPT, s.subrange(n, s.len() as int)) == ST_ACCEPT)
            <==> is_valid_utf8(s.subrange(n, s.len() as int)),
    ensures (run(ST_ACCEPT, s) == ST_ACCEPT) <==> is_valid_utf8(s)
{
    assert(s.subrange(0, n) + s.subrange(n, s.len() as int) =~= s);
    lemma_run_append(ST_ACCEPT, s.subrange(0, n), s.subrange(n, s.len() as int));
}

#[verifier::spinoff_prover]
pub proof fn lemma_run_valid(s: Seq<u8>)
    ensures (run(ST_ACCEPT, s) == ST_ACCEPT) <==> is_valid_utf8(s)
    decreases s.len()
{
    let n = s.len() as int;
    if n == 0 { return; }
    let b0 = s[0];
    assert(s.subrange(0, 0).len() == 0);
    lemma_run_snoc(ST_ACCEPT, s, 0, 0);
    let st1 = spec_step(ST_ACCEPT, b0);
    assert(run(ST_ACCEPT, s.subrange(0, 1)) == st1);

    if b0 <= 0x7F {
        assert(st1 == ST_ACCEPT && table_3_7(s) == 1);
        lemma_run_valid(s.subrange(1, n));
        live(s, 1);
    } else if (0x80 <= b0 && b0 <= 0xC1) || b0 >= 0xF5 {
        assert(st1 == ST_REJECT && table_3_7(s) == 0);
        dead(s, 1);
    } else if 0xC2 <= b0 && b0 <= 0xDF {
        assert(st1 == ST_C1);
        if n < 2 { dead(s, 1); assert(table_3_7(s) == 0); return; }
        let b1 = s[1];
        lemma_run_snoc(ST_ACCEPT, s, 0, 1);
        let st2 = spec_step(st1, b1);
        assert(run(ST_ACCEPT, s.subrange(0, 2)) == st2);
        if cont(b1) {
            assert(st2 == ST_ACCEPT && table_3_7(s) == 2);
            lemma_run_valid(s.subrange(2, n));
            live(s, 2);
        } else {
            assert(st2 == ST_REJECT && table_3_7(s) == 0);
            dead(s, 2);
        }
    } else if 0xE0 <= b0 && b0 <= 0xEF {
        // st1 ∈ {E0, C2, ED} depending on b0.
        if n < 2 { assert(st1 != ST_ACCEPT); dead(s, 1); assert(table_3_7(s) == 0); return; }
        let b1 = s[1];
        lemma_run_snoc(ST_ACCEPT, s, 0, 1);
        let st2 = spec_step(st1, b1);
        assert(run(ST_ACCEPT, s.subrange(0, 2)) == st2);
        // Second-byte range from Table 3-7.
        let ok1 = if b0 == 0xE0 { 0xA0 <= b1 && b1 <= 0xBF }
                  else if b0 == 0xED { 0x80 <= b1 && b1 <= 0x9F }
                  else { cont(b1) };
        if !ok1 {
            assert(st2 == ST_REJECT && table_3_7(s) == 0);
            dead(s, 2);
            return;
        }
        assert(st2 == ST_C1);
        if n < 3 { dead(s, 2); assert(table_3_7(s) == 0); return; }
        let b2 = s[2];
        lemma_run_snoc(ST_ACCEPT, s, 0, 2);
        let st3 = spec_step(st2, b2);
        assert(run(ST_ACCEPT, s.subrange(0, 3)) == st3);
        if cont(b2) {
            assert(st3 == ST_ACCEPT && table_3_7(s) == 3);
            lemma_run_valid(s.subrange(3, n));
            live(s, 3);
        } else {
            assert(st3 == ST_REJECT && table_3_7(s) == 0);
            dead(s, 3);
        }
    } else {
        assert(0xF0 <= b0 && b0 <= 0xF4);
        // st1 ∈ {F0, C3, F4}.
        if n < 2 { assert(st1 != ST_ACCEPT); dead(s, 1); assert(table_3_7(s) == 0); return; }
        let b1 = s[1];
        lemma_run_snoc(ST_ACCEPT, s, 0, 1);
        let st2 = spec_step(st1, b1);
        assert(run(ST_ACCEPT, s.subrange(0, 2)) == st2);
        let ok1 = if b0 == 0xF0 { 0x90 <= b1 && b1 <= 0xBF }
                  else if b0 == 0xF4 { 0x80 <= b1 && b1 <= 0x8F }
                  else { cont(b1) };
        if !ok1 {
            assert(st2 == ST_REJECT && table_3_7(s) == 0);
            dead(s, 2);
            return;
        }
        assert(st2 == ST_C2);
        if n < 3 { dead(s, 2); assert(table_3_7(s) == 0); return; }
        let b2 = s[2];
        lemma_run_snoc(ST_ACCEPT, s, 0, 2);
        let st3 = spec_step(st2, b2);
        assert(run(ST_ACCEPT, s.subrange(0, 3)) == st3);
        if !cont(b2) {
            assert(st3 == ST_REJECT && table_3_7(s) == 0);
            dead(s, 3);
            return;
        }
        assert(st3 == ST_C1);
        if n < 4 { dead(s, 3); assert(table_3_7(s) == 0); return; }
        let b3 = s[3];
        lemma_run_snoc(ST_ACCEPT, s, 0, 3);
        let st4 = spec_step(st3, b3);
        assert(run(ST_ACCEPT, s.subrange(0, 4)) == st4);
        if cont(b3) {
            assert(st4 == ST_ACCEPT && table_3_7(s) == 4);
            lemma_run_valid(s.subrange(4, n));
            live(s, 4);
        } else {
            assert(st4 == ST_REJECT && table_3_7(s) == 0);
            dead(s, 4);
        }
    }
}

/// `(w >> 8k) as u8` is byte `k` of `w` (the form `verify_multibyte` uses).
pub proof fn lemma_trunc_byte64(w: u64, k: int)
    requires 0 <= k < 8,
    ensures (w >> (8 * k) as u64) as u8 == byte64(w, k)
{
    let sh = (8 * k) as u64;
    assert(sh < 64 ==> (w >> sh) as u8 == ((w >> sh) & 0xFF) as u8) by (bit_vector);
}

/// Per-step proof helper for `verify_multibyte`'s eight-way unrolled chunk
/// body: byte `k` of `w` is `buf[p+k]`, and stepping by it extends the run
/// invariant from `[start, p+k)` to `[start, p+k+1)`.
pub proof fn lemma_chunk_snoc(buf: Seq<u8>, start: int, p: int, w: u64, k: int, prev: u64)
    requires
        0 <= start <= p, p + 8 <= buf.len(), 0 <= k < 8,
        w == pack64(buf, p),
        prev == run(ST_ACCEPT, buf.subrange(start, p + k)),
    ensures
        (w >> (8 * k) as u64) as u8 == buf[p + k],
        spec_step(prev, (w >> (8 * k) as u64) as u8)
            == run(ST_ACCEPT, buf.subrange(start, p + k + 1)),
{
    lemma_trunc_byte64(w, k);
    lemma_pack64_byte(buf, p, k);
    lemma_run_snoc(ST_ACCEPT, buf, start, p + k);
}

} // verus!
