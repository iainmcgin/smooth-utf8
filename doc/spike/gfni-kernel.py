#!/usr/bin/env python3
"""
Spike for issue #8 step 1: can byte -> class be a GF(2) affine map?

A GF(2) affine map f(x) = A*x XOR b (A: 8x8 bit-matrix) is linear in x up to
the constant b. For f to be constant on a byte class C, we need
A*(x XOR y) = 0 for every x, y in C — i.e., the within-class XOR span must
lie in ker(A). The output then has at most 2^rank(A) distinct values, and
rank(A) <= 8 - dim(span of all within-class XOR differences).

This script computes that span and the resulting upper bound on the number
of distinguishable classes.
"""

# CLASS table from src/lib.rs:705-714.
CLASS = (
    [0]*128 +
    [1]*16 + [9]*16 + [7]*32 +
    [8]*2 + [2]*30 +
    [10] + [3]*12 + [4] + [3]*2 +
    [11] + [6]*3 + [5] + [8]*11
)
assert len(CLASS) == 256

def basis(vecs):
    """Return a GF(2) basis (as a sorted list of ints) for the span of vecs."""
    b = []
    for v in vecs:
        for p in b:
            v = min(v, v ^ p)
        if v:
            b.append(v)
            b.sort(reverse=True)
    return b

# For each class, collect the XOR differences from its first member.
by_class = {}
for x in range(256):
    by_class.setdefault(CLASS[x], []).append(x)

print(f"Classes: {len(by_class)} ({sorted(by_class)})")
print(f"{'cls':>3} {'size':>5} {'range':>12} {'span-basis':>24}")

all_diffs = set()
for c in sorted(by_class):
    xs = by_class[c]
    diffs = {xs[0] ^ x for x in xs}
    all_diffs |= diffs
    b = basis(diffs)
    print(f"{c:>3} {len(xs):>5} {min(xs):#04x}-{max(xs):#04x}  basis={b} dim={len(b)}")

union_basis = basis(all_diffs)
print(f"\nUnion of within-class XOR spans: basis={union_basis} dim={len(union_basis)}")
print(f"=> Any affine map constant on every class has rank(A) <= 8 - {len(union_basis)} = {8-len(union_basis)}")
print(f"=> At most 2^{8-len(union_basis)} = {2**(8-len(union_basis))} distinct outputs.")
print(f"=> Need 12 distinct outputs. {'POSSIBLE' if 2**(8-len(union_basis)) >= 12 else 'IMPOSSIBLE'}.")

# What about GF2P8AFFINEINVQB (A * inv(x) XOR b)? Check the span of
# {inv(x) XOR inv(y) : x, y same class}.

# AES GF(2^8) inverse with reduction poly x^8+x^4+x^3+x+1 (0x11B).
def gf_mul(a, b):
    r = 0
    while b:
        if b & 1: r ^= a
        a <<= 1
        if a & 0x100: a ^= 0x11B
        b >>= 1
    return r

inv = [0]*256
for x in range(1, 256):
    for y in range(1, 256):
        if gf_mul(x, y) == 1:
            inv[x] = y
            break

inv_diffs = set()
for c in sorted(by_class):
    xs = by_class[c]
    inv_diffs |= {inv[xs[0]] ^ inv[x] for x in xs}

inv_basis = basis(inv_diffs)
print(f"\nWith GF2P8AFFINEINVQB (apply inverse first):")
print(f"Union of within-class XOR spans of inv(x): dim={len(inv_basis)}")
print(f"=> At most 2^{8-len(inv_basis)} = {2**(8-len(inv_basis))} distinct outputs. "
      f"{'POSSIBLE' if 2**(8-len(inv_basis)) >= 12 else 'IMPOSSIBLE'}.")

# Two affine maps (f, g) are still affine in x (just 16-bit output);
# same kernel constraint. Stacking doesn't help.
print(f"\nTwo affine maps (f(x), g(x)) form a single affine map to 16 bits;")
print(f"same kernel constraint applies: still at most {2**(8-len(union_basis))} outputs.")

# --- Restricted to non-ASCII (0x80-0xFF), which is what verify_multibyte sees ---
print("\n" + "="*60)
print("Restricted to byte >= 0x80 (verify_multibyte's domain):")
nonascii_diffs = set()
for c in sorted(by_class):
    if c == 0:
        continue
    xs = by_class[c]
    nonascii_diffs |= {xs[0] ^ x for x in xs}
na_basis = basis(nonascii_diffs)
print(f"Within-class XOR span (non-ASCII classes only): dim={len(na_basis)} basis={na_basis}")
print(f"=> At most 2^{8-len(na_basis)} = {2**(8-len(na_basis))} distinct outputs for 11 classes. "
      f"{'POSSIBLE' if 2**(8-len(na_basis)) >= 11 else 'IMPOSSIBLE'}.")

# What if we further coarsen: merge the singleton classes (E0/ED/F0/F4) into
# their neighbours and handle them with a separate equality test?
# Coarse classes: cont-lo(80-8F), cont-mid(90-9F), cont-hi(A0-BF),
#                 invalid(C0-C1,F5-FF), 2B-lead(C2-DF), 3B-lead(E0-EF), 4B-lead(F0-F4)
# = 7 classes, determined entirely by high nibble + (low nibble >= threshold).
COARSE = {}
for x in range(0x80, 0x100):
    hi = x >> 4
    if hi == 0x8: COARSE[x] = 0
    elif hi == 0x9: COARSE[x] = 1
    elif hi in (0xA, 0xB): COARSE[x] = 2
    elif hi in (0xC, 0xD): COARSE[x] = 3 if x >= 0xC2 else 6
    elif hi == 0xE: COARSE[x] = 4
    elif hi == 0xF: COARSE[x] = 5 if x <= 0xF4 else 6
coarse_by = {}
for x, c in COARSE.items():
    coarse_by.setdefault(c, []).append(x)
coarse_diffs = set()
for c, xs in coarse_by.items():
    coarse_diffs |= {xs[0] ^ x for x in xs}
cb = basis(coarse_diffs)
print(f"\n7-class coarsening (singletons merged, handled by separate eq-test):")
print(f"Within-class XOR span: dim={len(cb)} => at most {2**(8-len(cb))} outputs for 7 classes. "
      f"{'POSSIBLE' if 2**(8-len(cb)) >= 7 else 'IMPOSSIBLE'}.")
