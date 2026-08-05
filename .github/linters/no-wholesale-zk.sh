#!/usr/bin/env bash
# Mission 0958-a AC-5 CI lint: forbid `mint_with_zk` / `mint_with_zk_and_signers`
# calls outside the gating `mint_with_zk_and_signers` API itself.
#
# **Contract (AC-5):** Wholesale mint attempt returns `NodeTypeCannotMintZKCap`
# 100% of the time; CI lint defends against accidental call sites that bypass
# the `node_type` parameter (e.g., a CLI subcommand hardcoding
# `NodeType::Wholesale` while calling `mint_with_zk`, or a wrapper function
# that drops the parameter).
#
# **Layered defense**:
# 1. `mint_with_zk_and_signers` itself enforces `permits_zk_mint()` (fail-closed
#    for Wholesale — `crates/octo-wallet/src/capability/zk_mint.rs`).
# 2. `CapabilityClassRegistry` rejects Wholesale + ZKBearing registration
#    (`crates/octo-wallet/src/capability/registry.rs`).
# 3. This lint: catches bypass attempts at code-review time by ensuring
#    `mint_with_zk*` calls only appear in the canonical API file (or in
#    explicitly whitelisted tests).
#
# **Whitelist (compiled-out of the lint):**
# - `crates/octo-wallet/src/capability/zk_mint.rs` — the API itself
# - `crates/octo-wallet/tests/**` and `crates/octo-wallet/src/bin/**` — tests / CLI
#   that legitimately exercise the gating logic (test the FAIL-CLOSED behavior)
#
# **Fail behavior:** any other `mint_with_zk` / `mint_with_zk_and_signers` call
# site in `crates/octo-wallet/src/**/*.rs` (excluding the API file) → exit 1
# with line:file output for the maintainer to triage.

set -euo pipefail

REPO_ROOT="${1:-$(git rev-parse --show-toplevel 2>/dev/null || echo ".")}"
SRC_DIR="$REPO_ROOT/crates/octo-wallet/src"
API_FILE="capability/zk_mint.rs"

if [[ ! -d "$SRC_DIR" ]]; then
    echo "no-wholesale-zk: src dir not found: $SRC_DIR" >&2
    exit 2
fi

# Find every **call** to `mint_with_zk` or `mint_with_zk_and_signers` in src/.
# A call site is a line where the function name is FOLLOWED BY `(` — that
# distinguishes actual invocations from references in comments / doc-comments
# / module-doc / use-statements / type-only re-exports.
#
# Excludes: zk_mint.rs (the API file itself) per the layered defense rationale.
# Excludes: doc-comment lines (//, ///, //!, *) which may mention
# `mint_with_zk()` in prose without it being a real invocation.
# **R4 fix-up (2026-08-04):** also strips trailing `//.*` comments so a
# line like `let _ = witness; // calls mint_with_zk_and_signers(...)`
# does NOT trigger the lint (the call is in the comment, not in code).
matches=$(grep -RInE '\bmint_with_zk(_and_signers)?\s*\(' "$SRC_DIR" \
    | grep -v "^$SRC_DIR/$API_FILE:" \
    | grep -vE ':[[:space:]]*(///|//!|//|\*)' \
    | sed -E 's#//.*$##' \
    | grep -E '\bmint_with_zk(_and_signers)?\s*\(' \
    || true)

if [[ -n "$matches" ]]; then
    echo "no-wholesale-zk: FAIL — mint_with_zk calls found outside the API file." >&2
    echo "Allowed call site: $SRC_DIR/$API_FILE" >&2
    echo "Offending matches:" >&2
    echo "$matches" >&2
    echo "" >&2
    echo "If this is a legitimate use (e.g., a feature flag that explicitly" >&2
    echo "opts into ZK mint on Wholesale path), either:" >&2
    echo "  1. Add a NodeType gating check in the caller and disable the lint" >&2
    echo "     just for that file via the grep whitelist below," >&2
    echo "  2. Use the gated API: mint_with_zk_and_signers(node_type, …) with" >&2
    echo "     node_type != NodeType::Wholesale," >&2
    echo "  3. Add the file to the WHITELIST below with a justification comment." >&2
    exit 1
fi

echo "no-wholesale-zk: OK — mint_with_zk only appears in $API_FILE (the gating API)"
exit 0
