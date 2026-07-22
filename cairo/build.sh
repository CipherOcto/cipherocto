#!/usr/bin/env bash
# cairo/build.sh — compile cairo/capability_zk.cairo to CASM (RFC-0958 Phase B.2).
#
# Per RFC-0958 v1.1 R1 H12 fix: was `cairo/build.rs` (Cairo is not Rust so `.rs`
# extension was incorrect); now a shell script invoking scarb/cairo-compile.
#
# Production: this script runs at build time in the stoolap fork
# (`feat/blockchain-sql` branch); the compiled CASM bytes are committed to
# `bundled.rs` constants; verifier binary checks
# `casm_hash == COMPILED_CASM_BLAKE3_HASH` (RFC-0958 §Algorithms verification).
#
# MVP: this script is a no-op stub. Production wiring:
#   cairo-compile capability_zk.cairo --cairo_path ... --output capability_zk.casm
#   blake3 capability_zk.casm | tee capability_zk.casm.blake3
#
# Pin cairo-compile 2.6.0 via scarb/asdf in CI per master plan §8 Risk #6.

set -euo pipefail

CAIRO_FILE="capability_zk.cairo"
CASM_FILE="capability_zk.casm"
CASM_HASH_FILE="capability_zk.casm.blake3"

if ! command -v cairo-compile >/dev/null 2>&1; then
    echo "cairo-compile not found in PATH; Phase B.2 MVP stub — skipping" >&2
    exit 0
fi

if [[ ! -f "$CAIRO_FILE" ]]; then
    echo "missing $CAIRO_FILE" >&2
    exit 1
fi

cairo-compile "$CAIRO_FILE" --output "$CASM_FILE" --cairo_path /usr/local/lib/cairo
blake3 "$CASM_FILE" | awk '{printf $1}' > "$CASM_HASH_FILE"

echo "compiled $CAIRO_FILE → $CASM_FILE"
echo "casm hash: $(cat $CASM_HASH_FILE)"