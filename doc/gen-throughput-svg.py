#!/usr/bin/env python3
"""Generate doc/throughput.svg from criterion's saved baselines.

Reads target/criterion/verify_ascii/<impl>/<size>/<baseline>/estimates.json
(median.point_estimate, nanoseconds) for the named baselines produced by:

  cargo bench --bench throughput -- 'ascii/' --save-baseline portable
  RUSTFLAGS="-Cllvm-args=-align-all-nofallthru-blocks=6 -Ctarget-feature=+avx2" \
    cargo bench --bench throughput --features simdutf8 -- 'ascii/' \
    --save-baseline simd_avx2

Three series (log-x, log-y):
  - core::str::from_utf8
  - simdutf8::basic::from_utf8
  - smoothutf8 (verify_with_slack, +simdutf8 +avx2)

Usage: python3 doc/gen-throughput-svg.py doc/throughput-data.csv > doc/throughput.svg
"""
import csv
import math
import sys

# (baseline, criterion bench id) -> (legend label, colour)
# Solid lines: complementary dash phases were tried but the two polylines
# have different arc lengths through the log-y region, so the dash periods
# drift out of phase and beat. The relative plot (gen-throughput-relative-svg)
# keeps the dash treatment since its near-flat coincident stretch holds phase.
SERIES = {
    ("portable", "std_from_utf8"): ("stdlib", "#9ca3af"),
    ("simd_avx2", "simdutf8"): ("simdutf8", "#f59e0b"),
    ("portable", "smoothutf8_slack"): ("smoothutf8 (default)", "#2563eb"),
    ("simd_avx2", "smoothutf8_slack"): ("smoothutf8 (+simdutf8)", "#dc2626"),
}

shape = sys.argv[2] if len(sys.argv) > 2 else "ascii"
data = {k: [] for k in SERIES}
for r in csv.DictReader(open(sys.argv[1])):
    if r.get("shape", shape) != shape:
        continue
    k = (r["baseline"], r["impl"])
    if k in data:
        data[k].append((int(r["size"]), float(r["gibps"])))
for v in data.values():
    v.sort()

W, H, ML, MR, MT, MB = 720, 380, 56, 200, 20, 50
PW, PH = W - ML - MR, H - MT - MB
xmin, xmax = 1, 128 * 1024 * 1024
ys = [y for v in data.values() for _, y in v if y > 0]
ymin, ymax = min(ys) * 0.8, max(ys) * 1.2


def xpos(x):
    return ML + PW * (math.log2(x) - math.log2(xmin)) / (math.log2(xmax) - math.log2(xmin))


def ypos(y):
    return MT + PH * (1 - (math.log10(y) - math.log10(ymin)) / (math.log10(ymax) - math.log10(ymin)))


out = [
    f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
    f'viewBox="0 0 {W} {H}" font-family="system-ui,sans-serif" font-size="11">',
    f'<rect width="{W}" height="{H}" fill="#ffffff"/>',
]
# y gridlines (log: 1-2-5 per decade)
yt = 10 ** math.floor(math.log10(ymin))
while yt <= ymax:
    for m in (1, 2, 5):
        v = yt * m
        if v < ymin or v > ymax:
            continue
        y = ypos(v)
        out.append(
            f'<line x1="{ML}" y1="{y:.1f}" x2="{ML+PW}" y2="{y:.1f}" '
            f'stroke="{"#d4d4d4" if m == 1 else "#f0f0f0"}" stroke-width="1"/>'
        )
        lbl = f"{v:g}" if v >= 1 else f"{v:.1f}"
        out.append(
            f'<text x="{ML-6}" y="{y+3:.1f}" text-anchor="end" fill="#666">{lbl}</text>'
        )
    yt *= 10
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
    f'transform="rotate(-90 14 {MT+PH/2})">throughput (GiB/s, log scale)</text>'
)
# series
ly = MT + 8
for k, (label, colour) in SERIES.items():
    pts = " ".join(f"{xpos(x):.1f},{ypos(y):.1f}" for x, y in data[k] if y > 0)
    out.append(
        f'<polyline points="{pts}" fill="none" stroke="{colour}" stroke-width="2.2"/>'
    )
    out.append(
        f'<line x1="{ML+PW+10}" y1="{ly}" x2="{ML+PW+28}" y2="{ly}" '
        f'stroke="{colour}" stroke-width="2.2"/>'
    )
    out.append(f'<text x="{ML+PW+34}" y="{ly+4}" fill="#333">{label}</text>')
    ly += 18
# Methodology (sample count, host, build flags) is stated in the README text
# around the plot rather than baked into the SVG legend.
out.append("</svg>")
sys.stdout.write("\n".join(out) + "\n")
