# GFNI tableless DFA spike — result

**Verdict: GF(2) affine maps cannot compute the UTF-8 byte class. Issue #8 step 1 is no.**

## Argument

`GF2P8AFFINEQB` computes `f(x) = A·x ⊕ b` per byte, with `A` an 8×8 GF(2) matrix. For `f` to be constant on a byte class `C`, every within-class XOR difference `x ⊕ y` (for `x, y ∈ C`) must lie in `ker(A)`. The number of distinct outputs `f` can produce is then at most `2^rank(A) ≤ 2^(8 − dim(span of all within-class differences))`.

The `CLASS` table has 12 classes. The union of their within-class XOR spans has dimension **7** (the ASCII class alone — 128 bytes with bit 7 = 0 — contributes bits 0–6). So any affine map constant on every class has `rank(A) ≤ 1`, giving at most **2** distinct outputs. Twelve are needed.

Restricting to `byte ≥ 0x80` (the domain `verify_multibyte` actually sees, since the ASCII fast path runs first) drops the span to dimension **6** — still at most 4 outputs for 11 classes. Coarsening further by merging the four singleton classes (E0/ED/F0/F4) into their neighbours leaves 7 classes but the span is still dimension 6 (driven by the 30-byte 2-byte-lead class and the 32-byte high-continuation class). Still at most 4.

`GF2P8AFFINEINVQB` (`A·x⁻¹ ⊕ b`) is worse: the GF(2⁸) inverses of the ASCII bytes have full-dimension XOR span, giving at most 1 output.

Stacking two affine maps (`f(x), g(x)`) is itself an affine map to 16 bits; the same kernel constraint applies. Composition `f(g(x))` is also affine. Neither helps.

## What this means

The non-linearity has to come from `vpshufb` (16-entry arbitrary lookup). One `vpshufb` after an affine pre-permute is still bounded by the affine kernel argument (the 4-bit `vpshufb` index is an affine function of `x`). Two `vpshufb` on the high and low nibbles is the standard Keiser–Lemire approach, which `simdutf8` already implements and does not need GFNI.

So GFNI offers no path to a tableless byte classification that two `vpshufb` doesn't already give, and no reduction in op count below it. The "first ISA-accelerated path that narrows the trusted base" hypothesis is dead: any in-register classifier needs `vpshufb`, and a 32-byte `vpshufb` table is the same `external_body` shape as the current `ROW` table — just smaller.

The smaller-table angle (12 classes → 12 distinct `ROW` values = 96 B instead of 2 KB) is achievable with two `vpshufb` and no GFNI, but that's a different exploration (cache-footprint reduction, not "tableless") and the ≥64 B `simdutf8` delegation already covers the regime where the 2 KB table contends for L1d.

## Reproducer

`doc/spike/gfni-kernel.py` computes the span dimensions and bounds exhaustively from `CLASS`.
