# RefinedRust foundational proof

The CI gate for this crate is Verus (`cargo verus verify --features verus`, runs in seconds). RefinedRust is **not** run in CI; this document records how to reproduce the foundational Rocq proof of the raw-pointer leaf loads, for anyone who wants the stronger claim.

## What it proves

Verus treats `load64` and `load8` as `external_body` — it proves every call site satisfies `at + N ≤ buf.len()` but trusts the bodies. RefinedRust closes that gap: it verifies the bodies of `raw::load64_raw` and `raw::load8_raw` (the only place this crate dereferences a raw pointer) against separating ownership of the buffer. The annotated specs are in `src/raw.rs` and `src/raw/shims.rs`, gated by `cfg(rr)` so they are inert in normal builds.

The trusted base is: the `read_unaligned` shim contract in `src/raw/shims.rs` (sound by inspection — owning `n` initialized bytes and reading `size_of::<T>()` of them at offset `i` with `i + size ≤ n` does not change them), the `from_le` shim (existential return, vacuously sound), and the `&[u8]::as_ptr()` → "pointer valid for `len()` initialized bytes" step that bridges Verus's slice domain to RefinedRust's raw-pointer domain (the standard-library slice contract).

## Reproducing `2 Qed, 0 failed`

Requires nix with flakes. The toolchain (Rocq 9.1, Iris, the RefinedRust theories, the rrstd shim libraries, and the `cargo-refinedrust` frontend) is built from a pinned commit of <https://gitlab.mpi-sws.org/lgaeher/refinedrust-dev>.

```sh
# 1. Clone RefinedRust and build the core theories + stdlib shims via nix.
git clone https://gitlab.mpi-sws.org/lgaeher/refinedrust-dev.git refinedrust
cd refinedrust
git checkout 7f2e23d08f0a0db796053851c7f739b6c9e2e738
nix --extra-experimental-features 'nix-command flakes' build .#refinedrust .#stdlib

# 2. Build and install the frontend (cargo-refinedrust).
cd rr_frontend
rustup toolchain install nightly-2026-05-18 --profile minimal -c rustc-dev -c llvm-tools
./refinedrust build && ./refinedrust install

# 3. From this crate's root, generate the Rocq:
cd /path/to/smooth-utf8
RUSTFLAGS="--cfg rr" RR_CONFIG=RefinedRust.toml \
  cargo +nightly-2026-05-18 refinedrust
# → rr_out/smoothutf8/{generated,proofs}/

# 4. Compile the proof chain. verify/rr-check.sh wires the -Q/-R mappings
#    against the nix-built theories and runs rocq compile over the chain.
#    It hard-codes nix store paths from the build above; adjust if your
#    store hashes differ.
bash verify/rr-check.sh
# → ... === proofs: 2 Qed, 0 failed ===
```

Both proofs close via the Lithium automation alone; no manual Rocq is needed.

## Known limitations

RefinedRust does not currently model Rust slice types, which is why only the raw-pointer leaves are verified there and the slice-typed core is Verus's domain. RefinedRust (like Verus) also does not encode Rust's pointer-aliasing model (Stacked/Tree Borrows); for this crate that is moot — a single `*const u8` derived once from a `&[u8]`, no mutation.
