#!/usr/bin/env bash
set -uo pipefail
RR=$HOME/src/external/refinedrust
OUT="$(cd "$(dirname "$0")/.." && pwd)/rr_out/smoothutf8"
ROCQ=/nix/store/98afvvhxnvpv1hawb2d6qvn8949bygsw-rocq-9.1.0/bin
export PATH="$ROCQ:$PATH"
# OCaml plugins (Equations etc.) for findlib
OCAMLPATH=""
for d in /nix/store/*-rocq-core9.1-*/lib/ocaml/4.14.2/site-lib /nix/store/*-rocq-9.1.0/lib/ocaml/4.14.2/site-lib; do
  [ -d "$d" ] && OCAMLPATH="$d:$OCAMLPATH"
done
export OCAMLPATH

QFLAGS=()
# Installed core theories from nix store (stdpp, iris, Equations, Stdlib, lrust, etc.)
for d in /nix/store/*-rocq-core9.1-*/lib/coq/9.1/user-contrib; do
  for t in "$d"/*/; do
    name=$(basename "$t")
    [ "$name" = "rrstd" ] && continue   # rrstd handled below via -R
    QFLAGS+=(-Q "${t%/}" "$name")
  done
done
# RefinedRust core type system (radium, lithium, refinedrust)
for t in "$RR"/result/lib/coq/9.1/user-contrib/*/; do
  QFLAGS+=(-Q "${t%/}" "$(basename "$t")")
done
# rrstd compiled theories: each stdlib-*-dev package contributes a subtree
# under user-contrib/rrstd/; map them all recursively under the rrstd prefix.
for d in /nix/store/*-rocq-core9.1-refinedrust-stdlib-*-dev/lib/coq/9.1/user-contrib/rrstd; do
  [ -d "$d" ] && QFLAGS+=(-R "$d" rrstd)
done
# Our generated tree
QFLAGS+=(-Q "$OUT/generated" smooth_utf8.smoothutf8.generated)
QFLAGS+=(-Q "$OUT/proofs"    smooth_utf8.smoothutf8.proofs)

echo "[mappings: $(( ${#QFLAGS[@]} / 2 ))]"

compile() {
  local f="$1"
  echo ">>> $(basename "$f")"
  rocq compile -w -notation-overridden "${QFLAGS[@]}" "$f" 2>&1 | sed 's/^/    /'
  return ${PIPESTATUS[0]}
}

# Our chain (rrstd deps are pre-compiled now)
compile "$OUT/generated/generated_code_smoothutf8.v"                  || exit 1
compile "$OUT/generated/generated_specs_smoothutf8.v"                 || exit 1
compile "$OUT/generated/generated_template_raw_load64_raw.v"         || exit 1
compile "$OUT/generated/generated_template_raw_load32_raw.v"         || exit 1
compile "$OUT/generated/generated_template_raw_load16_raw.v"         || exit 1
compile "$OUT/generated/generated_template_raw_load8_raw.v"          || exit 1

echo "=== templates compiled; running proofs ==="
ok=0; fail=0
for p in "$OUT"/proofs/proof_raw_*.v; do
  if compile "$p"; then ok=$((ok+1)); else fail=$((fail+1)); fi
done
echo "=== proofs: $ok Qed, $fail failed ==="
[ "$fail" -eq 0 ]
