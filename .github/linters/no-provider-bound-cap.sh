#!/usr/bin/env bash
# CI lint: provider-boundary egress discipline (mission 0957-a AC #29 +
# mission 0957-b R3 AC-1).
#
# Two invariants:
#
# (1) `X-Capability-Token` / `Authorization: CipherOcto-Cap` presence on
#     outbound provider-bound requests is forbidden outside the canonical
#     egress module + boundary tests. (capability-token strip)
#
# (2) CipherOcto-internal key prefixes (`sk-virtual-`, `sk-cipherocto-`,
#     `sk-cto-`, `CipherOcto-`) MUST NEVER be the value of an outbound
#     provider `Authorization` header. The single canonical egress
#     helper is `egress::key_swap::attach_bearer` — bypasses are
#     bounded by the runtime denylist in that helper + this structural
#     lint. (provider key-swap)
#
# Rationale: capability tokens + cipherocto-internal keys are scoped to
# the cipherocto trust boundary; they MUST NOT cross to upstream
# providers. Real outbound HTTP happens in `proxy.rs` + per-provider
# code under `native_http/`. The lint catches:
#
#   - direct `format!("Bearer …")` patterns attached to any builder /
#     client / request (`reqwest`, `hyper`, `ureq`, `isahc` agnostic)
#   - `.bearer_auth(...)` invocations (reqwest's built-in Bearer helper)
#   - direct interpolation of a cipherocto-internal-prefix literal into
#     any `Authorization` header expression in `crates/`
#
# Allowed surface (verified by allowlist grep below):
#   - `crates/quota-router-core/src/egress.rs` + `src/egress/`
#     (canonical strip + key-swap helper)
#   - `crates/quota-router-core/tests/egress_boundary.rs` (boundary test)
#   - `crates/quota-router-core/tests/eleven_step.rs` (orchestration)
#   - `crates/quota-router-core/tests/key_swap_boundary.rs` (boundary test)
#   - `crates/quota-router-core/src/proxy.rs` (egress-shape; the 8 sites
#     ARE wired through `attach_bearer` per commit `da83d8cd` + R3 415)
#   - `crates/octo-core/src/capability.rs` (canonical pub const declaration)
#
# Intentional non-provider bypasses (KEY-SWAP portion only):
#   - `crates/quota-router-core/src/secret_manager.rs` — AWS SigV4
#     signed headers (`AWS4-HMAC-SHA256 Credential=...`), NOT `Bearer …`.
#     `attach_bearer` would prepend `Bearer ` and mangle the wire
#     format. Documented here because the bypass is structural; runtime
#     audit: `grep -n 'Authorization' crates/quota-router-core/src/secret_manager.rs`.
#   - `crates/quota-router-core/src/auth/sso/{scim,oauth2,jwt}.rs` — operator
#     IdP routes (SCIM/OAuth2/JWT mint). NOT model provider traffic.
#     `.bearer_auth(&self.scim_token)` etc. carry operator-issued IdP
#     tokens, not cipherocto-internal keys.

set -euo pipefail

cd "$(dirname "$0")/../.."  # repo root

# Allowed surface (substring match against path):
ALLOWLIST_PATHS=(
  'crates/quota-router-core/src/egress.rs'
  'crates/quota-router-core/src/egress/'
  'crates/quota-router-core/tests/egress_boundary.rs'
  'crates/quota-router-core/tests/key_swap_boundary.rs'
  'crates/quota-router-core/tests/eleven_step.rs'
  'crates/quota-router-core/src/proxy.rs'
  'crates/octo-core/src/capability.rs'
)
# Key-swap allowlist extensions (structural non-provider routes):
ALLOWLIST_KEY_SWAP=(
  'crates/quota-router-core/src/secret_manager.rs'
  'crates/quota-router-core/src/auth/sso/scim.rs'
  'crates/quota-router-core/src/auth/sso/oauth2.rs'
  'crates/quota-router-core/src/auth/sso/jwt.rs'
)

# ============================================================================
# (1) Capability-token strip
# ============================================================================
hits=$(grep -rn --include="*.rs" \
  -e 'X-Capability-Token' \
  -e 'CAPABILITY_HEADER' \
  -e 'CAPABILITY_HEADER_ALT_PREFIX' \
  crates/ \
  | grep -vE "$(printf '%s|' "${ALLOWLIST_PATHS[@]}")" \
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

# ============================================================================
# (2) Provider key-swap boundary
# ============================================================================
#
# Three structural shapes that are NOT acceptable on the egress path:
#
# (2a) any `format!("Bearer …")` literal attached to an Authorization
#      header value via any builder / client / request method call. The
#      canonical helper `attach_bearer` accepts a `&str` raw key and
#      renders the `Bearer …` value internally so the cipherocto-internal
#      denylist fires. Bypassing the helper defeats that denylist.
#
# (2b) any `.bearer_auth(...)` invocation outside the helper. reqwest's
#      built-in bearer helper produces `"Bearer {…}"` directly on the
#      wire — same bypass surface as (2a) but without `format!`.
#
# (2c) any code line that mentions a cipherocto-internal key prefix
#      (`sk-virtual-`, `sk-cipherocto-`, `sk-cto-`, `CipherOcto-`) AND
#      that line is doing Authorization-header attachment (same-line)
#      OR an Authorization-header name is on a nearby line within the
#      same function (multi-line bypass detector: any cipherocto-prefix
#      literal outside the helper module triggers a manual-review
#      flag; the runtime guard at `attach_bearer` is the safety net).

fail=0

# (2a) format!("Bearer …") outside the helper. Scan for any
# `format!("Bearer` substring within `crates/` excluding docs/comments +
# excluding the key-swap allowlist + the helper itself.
violations_2a=$(grep -rn --include="*.rs" \
  -e 'format!("Bearer' \
  crates/ \
  | grep -vE 'crates/quota-router-core/src/egress/key_swap\.rs' \
  | grep -vE "$(printf '%s|' "${ALLOWLIST_PATHS[@]}" "${ALLOWLIST_KEY_SWAP[@]}")" \
  | grep -vE ':\s*(///|//|/\*|!\s|^\s*\*\s)' \
  || true)

if [ -n "$violations_2a" ]; then
  echo "ERROR: direct format!(\"Bearer …\") outside canonical key-swap helper:"
  echo "$violations_2a"
  echo
  echo "The canonical egress Authorization wire value MUST be produced by"
  echo "crate::egress::key_swap::attach_bearer() so the cipherocto-internal"
  echo "denylist fires at construction time. Direct format!(\"Bearer …\")"
  echo "attachment outside the helper (and outside the canonical egress"
  echo "wired sites in proxy.rs / native_http/*.rs) is a key-swap bypass."
  fail=1
fi

# (2b) Any `.bearer_auth(...)` invocation outside the helper. reqwest
# exposes a built-in Bearer attach helper that produces
# `"Bearer {…}"` on the wire — same bypass surface as (2a), different
# syntax. Catches e.g.
#   .bearer_auth(&self.token)
#   .bearer_auth(token)
# even if the builder is `.client.bearer_auth(...)` with no `req_` prefix.
violations_2b=$(grep -rn --include="*.rs" \
  -E '\.bearer_auth\(' \
  crates/ \
  | grep -vE 'crates/quota-router-core/src/egress/key_swap\.rs' \
  | grep -vE "$(printf '%s|' "${ALLOWLIST_PATHS[@]}" "${ALLOWLIST_KEY_SWAP[@]}")" \
  | grep -vE ':\s*(///|//|/\*|!\s|^\s*\*\s)' \
  || true)

if [ -n "$violations_2b" ]; then
  echo "ERROR: .bearer_auth(...) outside canonical key-swap helper:"
  echo "$violations_2b"
  echo
  echo "reqwest::RequestBuilder::bearer_auth() produces \"Bearer {…}\" on"
  echo "the wire without going through the cipherocto-internal denylist."
  echo "Route the underlying key value through attach_bearer() instead."
  fail=1
fi

# (2c) Any literal cipherocto-internal key prefix appearing as a string
# literal in non-test, non-doc crate code. The runtime guard at
# attach_bearer is the safety net for keys that DO reach it; this scan
# catches the keys that escape the boundary surface entirely (e.g.,
# embedded in a config struct, used as a probe value in a unit test
# outside the boundary test, baked into a constant).
#
# Allowed: key_swap.rs (the denylist definition itself), the explicit
# allowlist, comments + tests.
violations_2c=$(grep -rn --include="*.rs" \
  -e '"sk-virtual-' \
  -e '"sk-cipherocto-' \
  -e '"sk-cto-' \
  -e '"CipherOcto-' \
  crates/ \
  | grep -vE 'crates/quota-router-core/src/egress/key_swap\.rs' \
  | grep -vE 'crates/quota-router-core/tests/' \
  | grep -vE "$(printf '%s|' "${ALLOWLIST_PATHS[@]}")" \
  | grep -vE ':\s*(///|//|/\*|!\s|^\s*\*\s)' \
  || true)

if [ -n "$violations_2c" ]; then
  echo "ERROR: cipherocto-internal key prefix literal embedded in non-test source:"
  echo "$violations_2c"
  echo
  echo "Literal cipherocto-internal key strings in source code are a"
  echo "key-swap hazard: any path that interpolates them into an"
  echo "Authorization header bypasses the runtime denylist guard."
  echo "Move them behind attach_bearer() or guarded test fixtures."
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi

echo "OK: capability-header + key-swap egress lint clean."
