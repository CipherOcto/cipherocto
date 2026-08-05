#!/usr/bin/env bash
# cairo/build.sh — compile cairo/src/lib.cairo to Sierra IR + CASM
# (RFC-0958 mission 0958-a Phase B.2 — LANDED 2026-08-04 per
# commit ae4dc4f8 Session 1 + commit 9c996fba Sessions 2 + 3 redo).
#
# Crypto home: cipherocto workspace (Phase B.2 per
# [[stoolap-general-purpose-db]], 2026-07-22 extraction). NOT the stoolap fork.
#
# This script is the manual / CI entry point for producing the Sierra IR
# that the downstream Rust in-process Sierra→CASM pass
# (`crates/zk-circuit/src/lib.rs::compile_source_inner`) consumes. It uses
# scarb (Cairo 2.x build orchestrator) — `cairo-compile` as a standalone
# binary does NOT exist on Cairo 2.x toolchains.
#
# **R4 fix-up (2026-08-04):** the prior script stopped at Sierra IR
# emission; the downstream Sierra→CASM pass was deferred to a "Session 2"
# that has since landed (commits `ae4dc4f8` + `9c996fba`). This script now
# produces the Sierra IR AND the downstream crate produces the CASM via the
# in-process `cairo-lang-sierra-to-casm` 2.20.0 pass.
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
echo "Sierra IR ready for downstream CASM compilation."
echo "The in-process Sierra→CASM pass runs at runtime via"
echo "crates/zk-circuit/src/lib.rs::compile_source_inner."
echo "BLAKE3 hash of the resulting CASM bytecode is exposed via"
echo "octo_wallet::capability::zk_mint::bundled_casm_hash()."
