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
  | grep -v 'crates/quota-router-core/src/egress/' \
  | grep -v 'crates/quota-router-core/tests/egress_boundary.rs' \
  | grep -v 'crates/quota-router-core/tests/key_swap_boundary.rs' \
  | grep -v 'crates/quota-router-core/tests/eleven_step.rs' \
  | grep -v 'crates/quota-router-core/src/proxy.rs' \
  | grep -v 'crates/octo-core/src/capability.rs' \
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

# ----------------------------------------------------------------------------
# Mission 0957-b Round-2: key-swap boundary (RFC-0957 §Adversary A5 + AC-1).
#
# CipherOcto-internal key prefixes (`sk-virtual-`, `sk-cipherocto-`,
# `sk-cto-`, `CipherOcto-`) MUST NEVER reach a provider as the
# `Authorization` header value. The canonical egress helper at
# `crates/quota-router-core/src/egress/key_swap.rs::attach_bearer` is the
# ONLY allowed attach site for outbound provider `Authorization` headers
# carrying a `Bearer` token. Direct `req_builder.header("Authorization",
# format!("Bearer {}", <local-var>))` patterns anywhere else are
# key-swap boundary violations.
#
# This scan complements the runtime denylist guard in
# `egress::key_swap::attach_bearer`: the runtime guard rejects cipherocto-
# shaped keys at construction time; this lint catches the structural bypass
# where a future contributor wires up a new outbound site that does not
# route through the helper.
# ----------------------------------------------------------------------------
key_swap_violations=$(grep -rn --include="*.rs" \
  -e 'req_builder\.header("Authorization", format!("Bearer {}"' \
  -e 'req_builder\.bearer_auth(' \
  -e '\.header("Authorization", format!("Bearer "' \
  crates/ \
  | grep -v 'crates/quota-router-core/src/egress/key_swap.rs' \
  | grep -v '^crates/quota-router-core/tests/egress_boundary.rs' \
  || true)

if [ -n "$key_swap_violations" ]; then
  echo "ERROR: provider-bound Authorization attached outside canonical key-swap helper:"
  echo "$key_swap_violations"
  echo
  echo "Outbound `Authorization: Bearer ...` headers MUST go through"
  echo "crate::egress::key_swap::attach_bearer() so the cipherocto-internal"
  echo "denylist fires at construction time. Direct format!/bearer_auth"
  echo "attachment outside the helper is a key-swap boundary violation."
  exit 1
fi

# Also catch any raw cipherocto-internal key prefix being interpolated
# into an Authorization header value.
cipherocto_egress_leak=$(grep -rn --include="*.rs" \
  -E '(req_builder|builder|request)\.header\(\s*"Authorization"\s*,\s*[^)]*(sk-virtual-|sk-cipherocto-|sk-cto-|CipherOcto-)' \
  crates/ \
  || true)

if [ -n "$cipherocto_egress_leak" ]; then
  echo "ERROR: cipherocto-internal key prefix referenced inside an outbound Authorization literal:"
  echo "$cipherocto_egress_leak"
  exit 1
fi

echo "OK: capability-header egress lint clean."