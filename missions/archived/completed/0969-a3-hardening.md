# Mission 0969-a3: Gateway Authenticator — Phase 1 Hardening

## Status

**Closed 2026-08-07.** Absorbs the 2 AC-B2.1 hardening items deferred from
`missions/claimed/0969-a2-followup.md` (Status: Closed 2026-08-07, 17/17 ACs
GREEN). Substrate work landed at commit `2ec21e01`. Total: 2/2 ACs GREEN.
Owner: @cipherocto.

Per [[git-workflow]] push awaits user instruction. Per [[no-line-refs-anywhere]]
all references use §symbol-name form. Per [[rfc-referencing-convention]] RFCs
referenced by number only. Per [[no-phantom-mission-pointers]] all `depends_on`
cites real missions or RFC substrate.

## RFC

RFC-0969 (Economics): Dual-Pipeline Authorization — Accepted 2026-08-02.

**Sub-mission of:** `missions/claimed/0969-a2-followup.md` (Band A closed 2026-08-07).

## Phase

Phase 1 hardening (post-Band-A closure): complete the substrate
routes that the 0969-a2 AC-B2 (`authenticate()` algorithm) deferred to
post-closure follow-up.

## Depends on

```yaml
depends_on:
  - 0969-a2-followup.md # Phase 1 dispatch + GatewayAuthenticator substrate (closed)
  - RFC-0969 # Dual-pipeline authorization spec
```

Real missions + RFC substrate only. No phantom pointers.

## Summary

Mission 0969-a2 closed the 17 deferred ACs from 0969-a in 3 commits
(`0434e53f` + `3979e1de` + `52bff741`) + 1 test commit
(`357d8384`) + 4 docs commits. AC-B2 (`authenticate()` algorithm) noted
2 substrate gaps in the AC-C8 / AC-C10 closures:

- `parse_auth_headers` silently discards `AuthHeader::Unsupported(s)` and
  fires `NoAuthHeader` when only an unsupported scheme is present —
  AC-B2.1.a: route `ParseError::UnsupportedScheme(scheme)` through
  `From<ParseError> for AuthError` as `AuthError::UnsupportedScheme(scheme)`.
- `evaluate_linkage` collapses "same subject + different ask" into
  full `IdentityMismatch` — AC-B2.1.b: add `LinkageResult::AskBindingMismatch`
  variant + route through `authenticate()` as `AuthError::AskBindingMismatch`.

Both gaps now closed at commit `2ec21e01`.

## Acceptance Criteria

- [x] **AC-B2.1.a.** `parse_auth_headers` returns `Err(ParseError::UnsupportedScheme(scheme))`
      when the `Authorization` header carries an unrecognized scheme (e.g.,
      `Basic <b64>`, `Digest ...`). `From<ParseError> for AuthError` routes the
      variant to `AuthError::UnsupportedScheme(scheme)`. `GatewayAuthenticator::authenticate()`
      surfaces `AuthError::UnsupportedScheme("Basic")` (not `NoAuthHeader`).
      **Closure:** landed at commit `2ec21e01`. (a) New `ParseError::UnsupportedScheme(String)`
      variant in `crates/octo-wallet/src/capability/dispatch.rs` (carries scheme
      name as operational metadata, not credential material). (b) `parse_auth_headers`
      returns `Err(ParseError::UnsupportedScheme(scheme))` instead of silently
      discarding `AuthHeader::Unsupported(s)`; `scheme = value.split_whitespace().next()`.
      (c) `From<ParseError> for AuthError` updated: `UnsupportedScheme(s)` →
      `AuthError::UnsupportedScheme(s)`. (d) `GatewayAuthenticator::authenticate()`
      automatically surfaces via `.map_err(AuthError::from)?` (no change needed).
      3 unit tests green: `parse_error_unsupported_scheme_carries_scheme_name`,
      `parse_error_unsupported_scheme_converts_to_auth_error`,
      `unsupported_auth_scheme_returns_unsupported_scheme_error` (in
      `gateway_authenticator::tests`). `tv8_unsupported_auth_scheme_returns_unsupported_scheme`
      in `tests/dispatch_tv.rs` updated to assert `UnsupportedScheme("Basic")` (was
      `NoAuthHeader` pre-B2.1.a). Closed early 2026-08-07 (ahead of 2026-09-04 target).
      Owner: @cipherocto. Target: 2026-09-04. **CLOSED 2026-08-07.**
- [x] **AC-B2.1.b.** `evaluate_linkage` distinguishes "same subject DID + different
      ask ID" from full "different subject DID + different ask ID". New
      `LinkageResult::AskBindingMismatch { bearer_ask: [u8;32], cap_ask: [u8;32] }`
      variant fires when `subject_did == holder_did` but `ask_id != ask_id`.
      `GatewayAuthenticator::authenticate()` surfaces `AuthError::AskBindingMismatch`.
      **Closure:** landed at commit `2ec21e01`. (a) New
      `LinkageResult::AskBindingMismatch { bearer_ask, cap_ask }` variant in
      `dispatch.rs` (4-arm enum: Linked / Mismatched / AskBindingMismatch /
      Indeterminate). (b) `evaluate_linkage` moved from `gateway_authenticator.rs`
      to `dispatch.rs` (canonical home next to the enum) + re-exported at
      `gateway_authenticator::evaluate_linkage`. (c) `evaluate_linkage` 3-arm
      logic: subject+ask both match → `Linked`; subject match + ask differ →
      `AskBindingMismatch { bearer_ask, cap_ask }`; subject differ →
      `Mismatched`; one absent → `Indeterminate`. (d) `GatewayAuthenticator::authenticate()`
      4-arm match updated to route `LinkageResult::AskBindingMismatch { .. }` to
      `AuthError::AskBindingMismatch { bearer_ask, cap_ask }`. 3 unit tests green:
      `evaluate_linkage_ask_binding_mismatch_when_subject_match_ask_differ`,
      `evaluate_linkage_mismatched_when_subject_differ`,
      `evaluate_linkage_ask_binding_mismatch_routes_to_auth_error` (in
      `gateway_authenticator::tests`). Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-09-04. **CLOSED 2026-08-07.**

## Acceptance Deviations

None — both ACs closed within this mission. No external blockers hit.

## Type Coverage

This hardening adds (per 0969-a2 AC-B2 §Deviations deferred items):

- **AC-B2.1.a:** `ParseError::UnsupportedScheme(String)` variant + `From<ParseError> for AuthError::UnsupportedScheme(scheme)` conversion path.
- **AC-B2.1.b:** `LinkageResult::AskBindingMismatch { bearer_ask, cap_ask }` variant + `evaluate_linkage` 4-arm decision logic + `authenticate()` routing to `AuthError::AskBindingMismatch`.

## Location

- `crates/octo-wallet/src/capability/dispatch.rs` (MODIFY) — `ParseError::UnsupportedScheme` variant + `LinkageResult::AskBindingMismatch` variant + `evaluate_linkage` function (moved from gateway_authenticator.rs) + `From<ParseError> for AuthError` updated
- `crates/octo-wallet/src/capability/gateway_authenticator.rs` (MODIFY) — `pub use evaluate_linkage` re-export + `authenticate()` 4-arm match arm for `LinkageResult::AskBindingMismatch`
- `crates/octo-wallet/tests/dispatch_tv.rs` (MODIFY) — `tv8_unsupported_auth_scheme_returns_unsupported_scheme` updated to assert `UnsupportedScheme("Basic")`

## Claimant

@cipherocto

## Pull Request

(unset)

## Closure

One impl commit + this docs commit landed 2026-08-07.

| AC | Target | Closed | Impl Commit |
|----|--------|--------|-------------|
| AC-B2.1.a | 2026-09-04 | 2026-08-07 (28d early) | `2ec21e01` |
| AC-B2.1.b | 2026-09-04 | 2026-08-07 (28d early) | `2ec21e01` |
| **Total** | 2026-09-04 | **2026-08-07** | 1 commit |

Verification artifacts (2026-08-07):

- `cargo test -p octo-wallet --lib capability::dispatch`: 25/25 pass (existing 22 + 3 new: `parse_error_unsupported_scheme_carries_scheme_name` + `parse_error_unsupported_scheme_converts_to_auth_error` + `evaluate_linkage_ask_binding_mismatch_when_subject_match_ask_differ` + `evaluate_linkage_mismatched_when_subject_differ`)
- `cargo test -p octo-wallet --lib capability::gateway_authenticator`: 18/18 pass (existing 15 + 3 new: `evaluate_linkage_ask_binding_mismatch_routes_to_auth_error` + `unsupported_auth_scheme_returns_unsupported_scheme_error`)
- `cargo test -p octo-wallet --test dispatch_tv`: 12/12 pass (TV8 updated)
- `cargo clippy -p octo-wallet --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- `bash .github/linters/braces-balanced.sh authenticate`: OK

## Notes

- Per [[implementation-workflow-hook]] this mission is filed in `claimed/`
  directly (planning + owner + target set; substrate work follows in
  subsequent commits). Closed in same session.
- `evaluate_linkage` move (gateway_authenticator.rs → dispatch.rs) is a
  pure refactor; no semantic change. The function was always a pure
  function on `LinkageResult` types, so dispatch.rs (which owns the
  types) is the canonical home. `gateway_authenticator::evaluate_linkage`
  is preserved as a re-export so existing call sites + tests don't need
  to update their `use` statements.

## Submission Date

2026-08-07T00:00:00Z

## Last Updated

2026-08-07T00:00:00Z

## Version

1.0 (Closed 2026-08-07 — 2/2 ACs GREEN)
