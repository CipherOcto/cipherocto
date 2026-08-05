#!/usr/bin/env bash
# cairo/build.sh — compile cairo/src/lib.cairo to Sierra IR (RFC-0958 Session 1).
#
# Crypto home: cipherocto workspace (Phase B.2 per
# [[stoolap-general-purpose-db]], 2026-07-22 extraction). NOT the stoolap fork.
#
# This script is the manual / CI entry point for producing the Sierra IR
# that the downstream Rust smoke test (crates/zk-circuit/tests/casm_snapshot.rs)
# consumes. It uses scarb (Cairo 2.x build orchestrator) — `cairo-compile`
# as a standalone binary does NOT exist on Cairo 2.x toolchains.
#
# Toolchain pin: scarb 2.16.0 / cairo 2.16.0.
# CI installs scarb via asdf per master plan §8 Risk #6.

set -euo pipefail

if ! command -v scarb >/dev/null 2>&1; then
    echo "ERROR: scarb not found in PATH" >&2
    echo "Install scarb 2.16.0: https://docs.swmansion.com/scarb/download.html" >&2
    echo "Or via asdf: asdf plugin add scarb && asdf install scarb 2.16.0" >&2
    exit 1
fi

if [[ ! -f "Scarb.toml" ]]; then
    echo "ERROR: Scarb.toml missing (run from cairo/ directory)" >&2
    exit 1
fi

scarb build

SIERRA_FILE="target/dev/capability_zk.sierra.json"
if [[ ! -f "$SIERRA_FILE" ]]; then
    echo "ERROR: scarb build did not produce $SIERRA_FILE" >&2
    exit 1
fi

echo "compiled cairo/src/lib.cairo -> $SIERRA_FILE"
echo "size: $(wc -c < "$SIERRA_FILE") bytes"
echo
echo "Note: CASM emission is Session 2 (cairo-lang-sierra-to-casm). Session 1"
echo "      stops at Sierra IR. The downstream CASM BLAKE3 hash ships with Session 2."
