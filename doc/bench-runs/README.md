# Curated benchmark run logs

Raw criterion stdout from the bare-metal benchmark runs that `BENCHMARKS.md` cites by run ID, kept in-tree so every published number is reproducible from a tracked input. `throughput-data.csv` (and from it, the SVG plots) regenerates byte-identically from `run-20260629T192815Z-4`:

```
python3 doc/parse-criterion-log.py \
    portable=doc/bench-runs/run-20260629T192815Z-4/00-portable.stdout.txt \
    simd_avx2=doc/bench-runs/run-20260629T192815Z-4/01-simd-fixed.stdout.txt \
    > doc/throughput-data.csv
```

Each file starts with a sysinfo header (CPU model, core pinning, turbo/governor state) recorded on the run host; see the methodology notes at the top of `BENCHMARKS.md` for what that quieting buys. The only edit relative to the raw capture is that transient EC2 hostnames are replaced with `bench-host`.

Superseded intermediate runs are not retained here; the working directory for new raw drops is `metal-bench-results/`, which is gitignored.
