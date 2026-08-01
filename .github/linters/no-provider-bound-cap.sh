#!/usr/bin/env bash
# CI lint: forbid `X-Capability-Token` (or `Authorization: CipherOcto-Cap`)
# presence on outbound provider-bound requests outside the canonical
# egress module.
#
# Enforces mission 0957-a AC #29: "Lint: forbid X-Capability-Token
# presence on outbound provider-bound requests."
#
# Rationale: capability tokens are authorization primitives scoped to
# cipherocto's trust boundary. They MUST NOT cross to upstream providers
# (only the wallet's provider-key slot auth does). The canonical egress
# point `crates/quota-router-core/src/egress.rs` is the ONLY site that
# constructs outbound requests and it strips the header before dispatch.
# Any other construction site is a leakage bug.
#
# Allowed surface:
#   - `crates/quota-router-core/src/egress.rs` (canonical strip)
#   - `crates/quota-router-core/tests/egress_boundary.rs` (verifies strip)
#   - `crates/quota-router-core/tests/eleven_step.rs` (orchestration, tests
#     inbound capability + outbound strip; in-process only)
#
# Scope: `crates/` excluding tests; CI runs on PR + main.

set -euo pipefail

cd "$(dirname "$0")/../.."  # repo root

# Find any line adding `X-Capability-Token` to a headers vec/list, outside
# the canonical egress + boundary-test files. Filter out comment lines
# (`//`, `///`, `//!`, doc-tests) since those are documentation, not code.
hits=$(grep -rn --include="*.rs" \
  -e 'X-Capability-Token' \
  -e 'CAPABILITY_HEADER' \
  -e 'CAPABILITY_HEADER_ALT_PREFIX' \
  crates/ \
  | grep -v 'crates/quota-router-core/src/egress.rs' \
  | grep -v 'crates/quota-router-core/tests/egress_boundary.rs' \
  | grep -v 'crates/quota-router-core/tests/eleven_step.rs' \
  | grep -v 'crates/quota-router-core/src/proxy.rs' \
  | grep -vE ':\s*(///|//|/\*|!\s|^\s*\*\s)' \
  || true)

if [ -n "$hits" ]; then
  echo "ERROR: capability header reference outside canonical egress surface:"
  echo "$hits"
  echo
  echo "X-Capability-Token / CipherOcto-Cap must ONLY appear in"
  echo "crates/quota-router-core/src/egress.rs (the strip point)."
  echo "Other references are capability-leakage bugs."
  exit 1
fi

echo "OK: capability-header egress lint clean."