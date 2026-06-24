#!/usr/bin/env python3
"""Generate doc/throughput-relative.svg from criterion's saved baselines.

Same data as gen-throughput-svg.py, but each series is normalized to
core::str::from_utf8 at the same input size (1.0 = parity). This makes
the speedup/slowdown vs the standard library explicit across the size
spectrum, where a log-log throughput plot can obscure ratios.

Usage: python3 doc/gen-throughput-relative-svg.py doc/throughput-data.csv > doc/throughput-relative.svg
"""
import csv
import math
import sys

SERIES = {
    ("portable", "std_from_utf8"): ("stdlib (= 1×)", "#9ca3af"),
    ("simd_avx2", "simdutf8"): ("simdutf8", "#f59e0b"),
    ("portable", "smoothutf8_slack"): ("smoothutf8 (default)", "#2563eb"),
    ("simd_avx2", "smoothutf8_slack"): ("smoothutf8 (+simdutf8)", "#dc2626"),
}

shape = sys.argv[2] if len(sys.argv) > 2 else "ascii"
raw = {k: {} for k in SERIES}
for r in csv.DictReader(open(sys.argv[1])):
    if r.get("shape", shape) != shape:
        continue
    k = (r["baseline"], r["impl"])
    if k in raw:
        raw[k][int(r["size"])] = float(r["gibps"])
std = raw[("portable", "std_from_utf8")]
data = {
    k: sorted((sz, v / std[sz]) for sz, v in raw[k].items() if sz in std)
    for k in SERIES
}

W, H, ML, MR, MT, MB = 720, 320, 56, 200, 20, 50
PW, PH = W - ML - MR, H - MT - MB
xmin, xmax = 1, 128 * 1024 * 1024
rs = [r for v in data.values() for _, r in v]
ymin, ymax = min(rs) * 0.9, max(rs) * 1.1


def xpos(x):
    return ML + PW * (math.log2(x) - math.log2(xmin)) / (math.log2(xmax) - math.log2(xmin))


def ypos(y):
    return MT + PH * (1 - (math.log2(y) - math.log2(ymin)) / (math.log2(ymax) - math.log2(ymin)))


out = [
    f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
    f'viewBox="0 0 {W} {H}" font-family="system-ui,sans-serif" font-size="11">',
    f'<rect width="{W}" height="{H}" fill="#ffffff"/>',
]
# y gridlines at clean ratio stops
for v in (0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4, 6, 8):
    if v < ymin or v > ymax:
        continue
    y = ypos(v)
    is_unity = abs(v - 1.0) < 1e-9
    out.append(
        f'<line x1="{ML}" y1="{y:.1f}" x2="{ML+PW}" y2="{y:.1f}" '
        f'stroke="{"#374151" if is_unity else "#ececec"}" '
        f'stroke-width="{1.2 if is_unity else 1}"/>'
    )
    out.append(
        f'<text x="{ML-6}" y="{y+3:.1f}" text-anchor="end" fill="#666">{v}×</text>'
    )
# x ticks
for xb, lbl in [
    (1, "1B"), (16, "16B"), (256, "256B"), (4096, "4K"),
    (65536, "64K"), (1 << 20, "1M"), (1 << 24, "16M"), (1 << 27, "128M"),
]:
    x = xpos(xb)
    out.append(
        f'<line x1="{x:.1f}" y1="{MT}" x2="{x:.1f}" y2="{MT+PH}" '
        f'stroke="#f5f5f5" stroke-width="1"/>'
    )
    out.append(
        f'<text x="{x:.1f}" y="{MT+PH+16}" text-anchor="middle" fill="#666">{lbl}</text>'
    )
# axis labels
out.append(
    f'<text x="{ML+PW/2}" y="{H-8}" text-anchor="middle" fill="#333" '
    f'font-size="12">input size (bytes, log scale)</text>'
)
out.append(
    f'<text x="14" y="{MT+PH/2}" text-anchor="middle" fill="#333" font-size="12" '
    f'transform="rotate(-90 14 {MT+PH/2})">speedup vs core::str::from_utf8</text>'
)
# series
ly = MT + 8
for k, (label, colour) in SERIES.items():
    pts = " ".join(f"{xpos(x):.1f},{ypos(r):.1f}" for x, r in data[k])
    out.append(
        f'<polyline points="{pts}" fill="none" stroke="{colour}" stroke-width="2.2"/>'
    )
    out.append(
        f'<line x1="{ML+PW+10}" y1="{ly}" x2="{ML+PW+28}" y2="{ly}" '
        f'stroke="{colour}" stroke-width="2.2"/>'
    )
    out.append(f'<text x="{ML+PW+34}" y="{ly+4}" fill="#333">{label}</text>')
    ly += 18
# Methodology footnote moved to README text.
out.append("</svg>")
sys.stdout.write("\n".join(out) + "\n")
