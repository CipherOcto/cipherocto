#!/usr/bin/env bash
# cairo/build.sh — compile cairo/capability_zk.cairo to CASM (RFC-0958 Phase B.2).
#
# Crypto home: cipherocto workspace (Phase B.2 per
# [[stoolap-general-purpose-db]], 2026-07-22 extraction). NOT the stoolap fork.
#
# Compiles `cairo/capability_zk.cairo` → `cairo/capability_zk.casm` and
# prints the BLAKE3 hash. CI installs scarb/asdf with `cairo-compile 2.6.0`
# pinned per master plan §8 Risk #6; this script fails loudly if the
# toolchain is missing (no silent skip — R1 H12 fix + S1 risk mitigation).
#
# The Rust crate `crates/zk-circuit::compile_from_source` also shells out to
# `cairo-compile` at runtime (memoized via `OnceLock`); this shell script
# is the manual / CI entry point for producing the check-in CASM hash
# (`EXPECTED_CASM_BLAKE3_HASH` in `crates/zk-circuit/tests/casm_snapshot.rs`).

set -euo pipefail

CAIRO_FILE="capability_zk.cairo"
CASM_FILE="capability_zk.casm"
CASM_HASH_FILE="capability_zk.casm.blake3"

if ! command -v cairo-compile >/dev/null 2>&1; then
    echo "ERROR: cairo-compile not found in PATH" >&2
    echo "Install via scarb (https://github.com/starkware-libs/cairo) or asdf" >&2
    echo "Pin: cairo-compile = 2.6.0 (master plan §8 Risk #6)" >&2
    echo "CI installs: .github/workflows/zk-capability-circuit.yml" >&2
    exit 1
fi

if [[ ! -f "$CAIRO_FILE" ]]; then
    echo "ERROR: missing $CAIRO_FILE" >&2
    exit 1
fi

cairo-compile "$CAIRO_FILE" --output "$CASM_FILE" --cairo_path /usr/local/lib/cairo
blake3 "$CASM_FILE" | awk '{printf $1}' > "$CASM_HASH_FILE"

echo "compiled $CAIRO_FILE -> $CASM_FILE"
echo "casm hash: $(cat $CASM_HASH_FILE)"