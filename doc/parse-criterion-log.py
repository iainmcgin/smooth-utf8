#!/usr/bin/env python3
"""Parse criterion's stdout (e.g. from bench-on-metal.sh) into CSV.

Reads one or more `<baseline>=<logfile>` pairs and emits
`baseline,impl,size,gibps` rows on stdout, taking the median from each
`time: [low unit mid unit high unit]` line.

Usage:
  python3 doc/parse-criterion-log.py \
      portable=metal-portable.log simd_avx2=metal-simd_avx2.log \
      > doc/throughput-data.csv
"""
import re
import sys

UNIT_NS = {"ps": 1e-3, "ns": 1.0, "us": 1e3, "µs": 1e3, "ms": 1e6, "s": 1e9}
GIB = 1024 ** 3

bench_re = re.compile(r"^verify/(\w+)/([\w_]+)/(\d+)\b")
time_re = re.compile(
    r"time:\s+\[\s*\S+\s+\S+\s+([\d.]+)\s+(\S+)\s+\S+\s+\S+\s*\]"
)

print("baseline,shape,impl,size,gibps")
for spec in sys.argv[1:]:
    baseline, path = spec.split("=", 1)
    impl = size = None
    for line in open(path, encoding="utf-8", errors="replace"):
        # The bench id and the `time:` field may be on the same line (when
        # the id is short enough for criterion's column layout) or on
        # consecutive lines; handle both by checking for each on every line.
        m = bench_re.match(line.strip())
        if m:
            shape, impl, size = m.group(1), m.group(2), int(m.group(3))
        m = time_re.search(line)
        if m and impl is not None:
            mid, unit = float(m.group(1)), m.group(2)
            ns = mid * UNIT_NS[unit]
            gibps = size / ns * 1e9 / GIB
            print(f"{baseline},{shape},{impl},{size},{gibps:.5f}")
            impl = size = None
