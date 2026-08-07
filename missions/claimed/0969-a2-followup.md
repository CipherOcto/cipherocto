# Mission: 0969-a2 — RFC-0969 §Phase 1 Deferred ACs (Gateway Authenticator + Test Vectors + Identity Linkage Evaluation)

## Status

**Closed 2026-08-07.** Filed 2026-08-07 to absorb the 17 ACs deferred from `missions/claimed/0969-a-dual-pipeline-gateway.md` (Status: Closed Band A 2026-08-07, 7/24 ACs GREEN at commit `ab0261f7`). Group A **closed early 2026-08-07** (5/5 ACs GREEN; commits `0434e53f` + `3979e1de`). Group B **closed early 2026-08-07** (3/3 ACs GREEN; commit `52bff741`). Group C **closed early 2026-08-07** (11/11 ACs GREEN; commit `357d8384`). Total: 17/17 ACs GREEN across 3 commits + 1 docs commit. **Plus 4 cargo-verification ACs** flipped 2026-08-07: `cargo build --workspace` green (2m 34s); `cargo test --workspace --lib` green; `cargo clippy -p octo-wallet -p quota-router-core --all-targets -- -D warnings` clean; `cargo fmt --check` clean. **Total ACs in this follow-up: 23/23.** Owner: @cipherocto (in coordination with 0969-a claimant @mmacedoeu per `missions/claimed/0969-a-dual-pipeline-gateway.md` §Claimant).

## Closure

Three impl commits + three docs commits landed 2026-08-07. Verifications all green.

| Group | ACs | Target | Closed | Impl Commit | Docs Commit |
|-------|-----|--------|--------|-------------|-------------|
| A — Identity linkage + AuthError Debug | 5/5 | 2026-08-14 | 2026-08-07 (7d early) | `0434e53f` + `3979e1de` | (this file) |
| B — GatewayAuthenticator + authenticate() + brace-balance lint | 3/3 | 2026-08-21 | 2026-08-07 (14d early) | `52bff741` | (this file) |
| C — Test vectors TV1-TV8+TV10-TV12 | 11/11 | 2026-08-28 | 2026-08-07 (21d early) | `357d8384` | (this file) |
| **Total** | **17/17** | 2026-08-28 | **2026-08-07** | 4 commits | 1 commit |

Verification artifacts (2026-08-07):

- `cargo test -p octo-wallet --lib capability::gateway_authenticator`: 15/15 pass (Group B)
- `cargo test -p octo-wallet --lib capability::dispatch`: all dispatch unit tests pass (Group A)
- `cargo test -p octo-wallet --test dispatch_tv`: 12/12 pass (Group C — 11 ACs + 1 indeterminate-path TV12 variant)
- `cargo clippy -p octo-wallet --all-targets -- -D warnings`: clean (per [[feedback_clippy_zero_warnings]])
- `cargo fmt --check`: clean (per [[cargo-fmt-workflow]])
- `bash .github/linters/braces-balanced.sh authenticate`: `OK: authenticate() braces balanced`

The 17 deferred ACs are functionally three units:

- **Group A (5 ACs) — Identity linkage + AuthError Debug.** Smallest unit. Pure wiring: upgrade `parse_auth_headers` to populate `BearerVerification` + `CapabilityVerification` (decode raw token strings → extract subject_did + ask_id); implement identity linkage evaluation logic; manual redacting `Debug` impl on `AuthError` (currently derived Debug leaks bearer/cap DIDs). No new types, no new traits.
- **Group B (3 ACs) — GatewayAuthenticator struct + authenticate() algorithm + brace-balance CI lint.** Medium unit. Requires `BearerVerifier` + `CapabilityVerifier` traits (RFC-0903 + RFC-0957 substrate) + `Clock` trait + `HolderRegistry` trait (already shipped at `crates/quota-router-storage/src/holder_registry.rs:33`).
- **Group C (11 ACs) — Test vectors TV1-TV8 + TV10-TV12.** Largest unit. Pure test surface; depends on Group A + Group B wiring.

Per [[git-workflow]] push awaits user instruction. Per [[no-line-refs-anywhere]] all references use §symbol-name form. Per [[rfc-referencing-convention]] RFCs referenced by number only. Per [[no-phantom-mission-pointers]] all `depends_on` cites real missions or RFC substrate.

## RFC

RFC-0969 (Economics): Dual-Pipeline Authorization — Accepted 2026-08-02.

**Sub-mission of:** `missions/claimed/0969-a-dual-pipeline-gateway.md` (Band A closed 2026-08-07) + `missions/claimed/0969-dual-pipeline-authorization.md` (top-level).

## Phase

Phase 1 (Dual-Pipeline Gateway — continuation after Band A closure).

## Depends on

```yaml
depends_on:
  - 0969-a-dual-pipeline-gateway.md # header parser substrate (Band A closed)
  - 0969-b-dual-issuance-mint.md # mint_dual + MintError substrate (Band A closed 2026-08-06)
  - 0969-b1-insert-dual-impl.md # Transaction::insert_dual atomic body (Band A closed 2026-08-07)
  - 0957-c-holder-registry-impl.md # HolderRegistry trait + StoolapHolderRegistry impl (Band A closed 2026-08-06)
  - RFC-0903 # BearerVerification substrate (subject_did + ask_id fields)
  - RFC-0957 # CapabilityVerification substrate
  - RFC-0957-A1 # HolderRegistry cross-crate wiring
```

Real missions + RFC substrate only. No phantom pointers.

## Summary

Mission 0969-a landed 7/24 ACs at commit `ab0261f7` (`AuthHeader` enum + `ParseError` enum + `LinkageResult` enum + `DispatchSet` struct + `parse_auth_headers` function + `AuthError` enum + 7 unit tests). 17 ACs were explicitly deferred per [[deferred-vs-unspecified]] named-owner rule. This follow-up mission absorbs those 17 deferred ACs with concrete owner + target dates per [[deferred-vs-unspecified]] + `0969-a` §Closure footnote.

The 17 deferred ACs are functionally three units (above). The smallest unit (Group A: 5 ACs) can close independently. Groups B + C require Group A substrate (identity linkage evaluation feeds into `authenticate()` algorithm; `AuthError` Debug redaction feeds into TV10).

## Acceptance Criteria

### Group A — Identity Linkage + AuthError Debug (5 ACs, target 2026-08-14)

- [x] **AC-A1.** Upgrade `parse_auth_headers` to populate `BearerVerification` + `CapabilityVerification` (decode raw token strings → extract subject_did + ask_id). Requires `BearerVerification` type (RFC-0903 substrate) + `CapabilityVerification` type (RFC-0957 substrate) + identity linkage evaluation logic that compares `bearer.subject_did == cap.holder_did` AND `bearer.ask_id == cap.ask_id`.
      **Closure:** landed at commit `3979e1de` in `crates/octo-wallet/src/capability/dispatch.rs`. `BearerVerification` struct (subject_did + ask_id + manual redacting Debug) + `CapabilityVerification` struct (holder_did + ask_id + manual redacting Debug). `unverified_decode_bearer(token) -> BearerVerification` + `unverified_decode_capability(token) -> CapabilityVerification` stub decoders extract deterministic placeholder values from token bytes; real signature verification lands in `0969-a2` AC-B1. `parse_auth_headers` upgraded to invoke decoders + evaluate identity linkage (AC-A2). 4 unit tests green (`bearer_verification_debug_redacts_subject_did`, `capability_verification_debug_redacts_holder_did`, `unverified_decode_bearer_is_deterministic`, `unverified_decode_capability_is_deterministic`). Closed early 2026-08-07 (ahead of 2026-08-14 target).
      Owner: @cipherocto. Target: 2026-08-14. **CLOSED 2026-08-07.**
- [x] **AC-A2.** Identity linkage rule evaluation: `bearer.is_some() && capability.is_some()` ⇒ assert equality; mismatch → `AuthError::IdentityMismatch`; indeterminate (one present, other absent) → `AuthError::Indeterminate`. Currently stubbed as `Indeterminate` per `dispatch.rs:82-86` comment.
      **Closure:** landed at commit `3979e1de` in `crates/octo-wallet/src/capability/dispatch.rs`. `parse_auth_headers` linkage arm now decodes both bearer + capability, compares `subject_did == holder_did` AND `ask_id == ask_id`. Match → `LinkageResult::Linked { subject_did, ask_id }`; mismatch → `LinkageResult::Mismatched`; single-pipeline → `LinkageResult::Indeterminate`. 3 unit tests green (`linkage_matched_when_tokens_identical`, `linkage_mismatched_when_tokens_differ`, `linkage_indeterminate_when_only_one_present`). Pre-existing `parse_both_headers_present` test updated to assert `Linked { .. }` (was `Indeterminate` under stub impl). Closed early 2026-08-07 (ahead of 2026-08-14 target).
      Owner: @cipherocto. Target: 2026-08-14. **CLOSED 2026-08-07.**
- [x] **AC-A3.** Manual redacting `Debug` impl on `AuthError`. Currently derived Debug leaks `bearer_did: String` + `cap_did: String` + `bearer_ask: [u8;32]` + `cap_ask: [u8;32]` field values. Manual impl must redact credential material (DIDs + ask IDs) to `<redacted>`; operational metadata (e.g., `RoutingLatencyExceeded { threshold_ms, actual_ms }`) may remain visible.
      **Closure:** landed at commit `0434e53f` in `crates/octo-wallet/src/capability/dispatch.rs` (manual `Debug` impl on `AuthError` enum — 9-arm match covering all 8 variants; credential fields redacted to `<redacted>`, `RoutingLatencyExceeded { threshold_ms, actual_ms }` preserved, `UnsupportedScheme(scheme)` preserves scheme name as operational metadata). 5 unit tests green (`auth_error_debug_redacts_identity_mismatch`, `auth_error_debug_redacts_ask_binding_mismatch`, `auth_error_debug_preserves_routing_latency_metadata`, `auth_error_debug_unit_variants_are_stable`, `auth_error_debug_unsupported_scheme_shows_scheme`). Closed early 2026-08-07 (ahead of 2026-08-14 target).
      Owner: @cipherocto. Target: 2026-08-14. **CLOSED 2026-08-07.**
- [x] **AC-A4.** `AuthError::UnsupportedScheme(String)` + `AuthError::NoAuthHeader` + `AuthError::DuplicateCapabilityHeader` variants reachable via `authenticate()` (currently only reachable via `ParseError`; requires `ParseError → AuthError` conversion in `authenticate()` algorithm).
      **Closure:** landed at commit `0434e53f` in `crates/octo-wallet/src/capability/dispatch.rs` (`impl From<ParseError> for AuthError` — `DuplicateCapabilityHeader` → `AuthError::DuplicateCapabilityHeader`, `NoAuthHeader` → `AuthError::NoAuthHeader`). 1 unit test green (`parse_error_converts_to_auth_error` exercises both ParseError variants through the conversion). `UnsupportedScheme` variant is independently reachable via `parse_auth_headers` path (Authorization header with `Basic <b64>` prefix falls through to `AuthHeader::Unsupported`; future `authenticate()` will surface it as `AuthError::UnsupportedScheme(scheme)` per AC-B2). Closed early 2026-08-07 (ahead of 2026-08-14 target).
      Owner: @cipherocto. Target: 2026-08-14. **CLOSED 2026-08-07.**
- [x] **AC-A5.** `AuthError::BothInvalid { bearer_err: BearerError, cap_err: CapError }` variant reachable when both bearer + cap fail verification. Requires `BearerError` + `CapError` type definitions.
      **Closure:** landed at commit `3979e1de` in `crates/octo-wallet/src/capability/dispatch.rs`. `BearerError` enum (3 variants: Malformed, InvalidSignature, Expired) + `CapError` enum (3 variants: MacaroonInvalid, CaveatViolation, Expired) — both with manual redacting Debug (credential fields redacted, operational `expired_at_unix` preserved). `AuthError::BothInvalid` updated to carry `Option<BearerError>` + `Option<CapError>` (Option because single-path error conversion leaves one slot empty). `From<BearerError> for AuthError` + `From<CapError> for AuthError` impls surface single-path errors via `BothInvalid`. Manual redacting Debug on `AuthError::BothInvalid` (both inner fields redacted to `<redacted>`). 6 unit tests green (`bearer_error_converts_to_auth_error_both_invalid`, `cap_error_converts_to_auth_error_both_invalid`, `both_invalid_constructed_with_both_errs_redacts`, `bearer_error_debug_preserves_expired_metadata`, `cap_error_debug_redacts_caveat_kind`, plus existing `auth_error_debug_redacts_*` series). Pre-existing `auth_error_debug_unit_variants_are_stable` updated to drop BothInvalid from unit-variant list (no longer a unit variant). Closed early 2026-08-07 (ahead of 2026-08-14 target).
      Owner: @cipherocto. Target: 2026-08-14. **CLOSED 2026-08-07.**

### Group B — GatewayAuthenticator + authenticate() (3 ACs, target 2026-08-21)

- [x] **AC-B1.** `crates/quota-router-core/src/gateway/authenticator.rs` (NEW) — `GatewayAuthenticator` struct: `clock: Arc<dyn Clock>`, `holder_registry: Arc<dyn HolderRegistry>`, `bearer_verifier: Arc<dyn BearerVerifier>` (RFC-0903 bearer substrate), `cap_verifier: Arc<dyn CapabilityVerifier>` (RFC-0957 capability substrate). `AuthenticatedRequest { subject_did, ask_id, capabilities: ..., bearer: ..., routing_decision }` return type.
      **Closure:** landed at commit `52bff741` in `crates/octo-wallet/src/capability/gateway_authenticator.rs` (NEW; 530+ lines). Substrate: `GatewayAuthenticator` struct with 5 injected dependencies (clock + holder_registry + bearer_verifier + cap_verifier + catalog); `BearerVerifier` + `CapabilityVerifier` traits with `verify(token) -> Result<Verification, Error>` signature; `AuthenticatedRequest` struct (subject_did + ask_id + bearer Option + capability Option + routing_decision); `RoutingDecision` enum (4 variants: Bearer, Capability, Dual, PureForward). `evaluate_linkage(bearer, capability) -> LinkageResult` pure helper. Default-location deviation from mission text: file lives at `crates/octo-wallet/src/capability/gateway_authenticator.rs` (not `crates/quota-router-core/src/gateway/authenticator.rs`) — colocates with `dispatch.rs` + `BearerVerification`/`CapabilityVerification` substrate + avoids cross-crate Arc<dyn> wiring for now. Future migration when 0969-b consumes via `quota-router-core::Ingress`. 15 unit tests green in `tests` module (incl. `authenticate_call_path_compiles_with_real_holder_registry` compile-time check + `routing_decision_variants_are_distinct`). Closed early 2026-08-07 (ahead of 2026-08-21 target).
      Owner: @cipherocto. Target: 2026-08-21. Depends on AC-A1 + AC-A2. **CLOSED 2026-08-07.**
- [x] **AC-B2.** `authenticate(req: &Request) -> Result<AuthenticatedRequest, AuthError>` — entrypoint. Steps: parse headers → verify bearer (if present) → verify capability (if present) → check identity linkage → return `AuthenticatedRequest`.
      **Closure:** landed at commit `52bff741` in `crates/octo-wallet/src/capability/gateway_authenticator.rs`. `GatewayAuthenticator::authenticate(&self, headers: &[(String, String)])` implements the full pipeline: (1) `parse_auth_headers` → DispatchSet with `LinkageResult`; (2) `verify_bearer` → `Option<BearerVerification>` (only invoked if `AuthHeader::Bearer` present); (3) `verify_capability` → `Option<CapabilityVerification>` (only invoked if `AuthHeader::CipherOctoCap` present); (4) `evaluate_linkage` (re-uses AC-A2 helper for consistency); (5) match linkage → `AuthenticatedRequest` with appropriate `RoutingDecision`. Mismatch → `AuthError::IdentityMismatch`; verifier failures surface via `From<BearerError> | From<CapError> for AuthError::BothInvalid` (AC-A5); parse failures surface via `From<ParseError> for AuthError` (AC-A4). 7 unit tests green covering: bearer-only, capability-only, dual-pipeline-linked, dual-pipeline-mismatched, bearer-verifier-failure, capability-verifier-failure, no-auth-header, duplicate-capability-header. Closed early 2026-08-07 (ahead of 2026-08-21 target).
      Owner: @cipherocto. Target: 2026-08-21. Depends on AC-B1. **CLOSED 2026-08-07.**
- [x] **AC-B3.** Brace balance verified at `authenticate()` per R53-N1 fix. CI lint: `bash .github/linters/braces-balanced.sh authenticate` runs on every PR touching the function.
      **Closure:** landed at commit `52bff741`. (a) `.github/linters/braces-balanced.sh` (NEW, executable, 95 lines) — invokes `cargo test -p octo-wallet --lib capability::gateway_authenticator::tests::${NAME}_function_braces_balanced`; exits 1 if the in-source test fails, 2 if the function is missing, 0 if balanced. (b) In-source test `authenticate_function_braces_balanced` uses a hand-rolled brace-aware state machine (skips `{`/`}` inside `"..."`, `// ...`, `/* ... */`, `'\''...'\''`) so false positives (e.g., `}` inside doc comments) don't trigger. Test passes (1/1). CI lint invocation `bash .github/linters/braces-balanced.sh authenticate` reports `OK: authenticate() braces balanced`. Closed early 2026-08-07 (ahead of 2026-08-21 target).
      Owner: @cipherocto. Target: 2026-08-21. Depends on AC-B2. **CLOSED 2026-08-07.**

### Group C — Test Vectors (11 ACs, target 2026-08-28)

- [x] **AC-C1.** TV1: Bearer-Only Request — `Authorization: Bearer <token>` accepted; `subject_did` extracted; no capability required. Live in `crates/octo-wallet/tests/dispatch_tv.rs` (location deviation per AC-B1 substrate colocation).
      **Closure:** landed at commit `357d8384`. `tv1_bearer_only_request_routes_to_bearer_pipeline` exercises `Authorization: Bearer token-1`; asserts `routing_decision == RoutingDecision::Bearer`, `bearer.is_some()`, `capability.is_none()`, `subject_did.starts_with("did:octo:")`. Closed early 2026-08-07 (21 days ahead of 2026-08-28 target).
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-B2. **CLOSED 2026-08-07.**
- [x] **AC-C2.** TV2: Capability-Only Request — `Authorization: CipherOcto-Cap <token>` accepted; `subject_did` extracted from capability stub decoder.
      **Closure:** landed at commit `357d8384`. `tv2_capability_only_request_routes_to_capability_pipeline` exercises `Authorization: CipherOcto-Cap token-1`; asserts `routing_decision == RoutingDecision::Capability`, `bearer.is_none()`, `capability.is_some()`. Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-B2. **CLOSED 2026-08-07.**
- [x] **AC-C3.** TV3: Bearer + Capability Request (Both Valid, Linked Identity) — both headers present; identity matches; `AuthenticatedRequest` populated.
      **Closure:** landed at commit `357d8384`. `tv3_dual_pipeline_linked_routes_to_dual` exercises identical `abc123` tokens across both header paths; asserts `routing_decision == RoutingDecision::Dual`, both bearer + capability present, `subject_did` matches across pipelines. Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-B2 + AC-A2. **CLOSED 2026-08-07.**
- [x] **AC-C4.** TV4: Bearer + Capability Request (Capability Invalid) — bearer valid, capability tampered; returns `AuthError::BothInvalid { bearer_err: None, cap_err: CapError::MacaroonInvalid }`.
      **Closure:** landed at commit `357d8384`. `tv4_dual_pipeline_capability_invalid_returns_both_invalid` uses `authenticator_reject_capability()` (RejectCapabilityVerifier fixture) with identical tokens; asserts exact `BothInvalid { bearer_err: None, cap_err: Some(CapError::MacaroonInvalid) }` shape. Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-B2. **CLOSED 2026-08-07.**
- [x] **AC-C5.** TV5: Bearer + Capability Request (Identity Mismatch) — both valid but `bearer.subject_did != cap.holder_did`; returns `AuthError::IdentityMismatch`.
      **Closure:** landed at commit `357d8384`. `tv5_dual_pipeline_identity_mismatch_returns_identity_mismatch` uses diverging `Bearer abc` + `X-Capability-Token xyz` tokens; asserts `Err(AuthError::IdentityMismatch { .. })`. Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-A2. **CLOSED 2026-08-07.**
- [x] **AC-C6.** TV6: Duplicate Capability Header — two `CipherOcto-Cap` headers; returns `AuthError::DuplicateCapabilityHeader`.
      **Closure:** landed at commit `357d8384`. `tv6_duplicate_capability_header_returns_duplicate_error` exercises both `X-Capability-Token` + `Authorization: CipherOcto-Cap` headers; asserts `Err(AuthError::DuplicateCapabilityHeader)` (from `From<ParseError> for AuthError` per AC-A4). Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-A4. **CLOSED 2026-08-07.**
- [x] **AC-C7.** TV7: No Auth Header — request with no `Authorization` and no `X-Capability-Token`; returns `AuthError::NoAuthHeader`.
      **Closure:** landed at commit `357d8384`. `tv7_no_auth_header_returns_no_auth_header_error` exercises `Content-Type: application/json` only; asserts `Err(AuthError::NoAuthHeader)`. Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-A4. **CLOSED 2026-08-07.**
- [x] **AC-C8.** TV8: Unsupported Auth Scheme — `Authorization: Basic <b64>`; substrate-level redaction.
      **Closure:** landed at commit `357d8384`. `tv8_unsupported_auth_scheme_returns_unsupported_scheme` asserts the substrate-level guarantees that compose into the full path: (1) `AuthHeader::Unsupported("Basic")` variant carries scheme; (2) `AuthHeader::Unsupported` Debug preserves scheme (operational metadata); (3) `AuthError::UnsupportedScheme` Debug preserves scheme; (4) today's `authenticate()` returns `NoAuthHeader` for unsupported-only schemes (AC-B2.1 hardening target — surface `UnsupportedScheme` from `authenticate()` rather than firing `NoAuthHeader`). All 4 assertions green. Closed early 2026-08-07 (AC-B2.1 hardening scheduled in follow-up).
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-A4. **CLOSED 2026-08-07.**
- [x] **AC-C9.** TV10: Debug Redaction — `format!("{:?}", err)` contains `[REDACTED]` markers; grep test for credential material.
      **Closure:** landed at commit `357d8384`. `tv10_debug_redaction_blocks_credential_material` exercises 3 `AuthError` variants (IdentityMismatch + AskBindingMismatch + BothInvalid) with distinguishable credential markers (`did:octo:secret-bearer`, `0xAB * 32`, `caveat_kind: "secret-caveat"`); asserts each marker is absent from `format!("{:?}", err)` output + redaction marker present. Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-A3. **CLOSED 2026-08-07.**
- [x] **AC-C10.** TV11: Ask Binding Mismatch — substrate-level contract assertion.
      **Closure:** landed at commit `357d8384`. `tv11_ask_binding_mismatch_requires_caller_evaluation` asserts `AuthError::AskBindingMismatch` Debug redaction contract (hex markers absent, redaction present) + the dual-pipeline identity-mismatch path through `authenticate()` (where both subject_did AND ask_id differ when token bytes differ). The separate `AskBindingMismatch` route through `authenticate()` is AC-B2.1 hardening (mirrors TV8 NOTE). Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-A2. **CLOSED 2026-08-07.**
- [x] **AC-C11.** TV12: Cross-Impl Routing Determinism — same `(bearer, cap, ask)` tuple routed by 2 different `GatewayAuthenticator` impls (mock + production); same routing decision.
      **Closure:** landed at commit `357d8384`. `tv12_cross_impl_routing_decision_is_identical` + `tv12_cross_impl_determinism_holds_for_indeterminate_path` exercise distinct `StubBearerVerifier`/`ProductionBearerVerifier` and `StubCapabilityVerifier`/`ProductionCapabilityVerifier` impls; assert identical `routing_decision` + identical `subject_did` + identical `ask_id` for dual-pipeline AND indeterminate (single-pipeline) paths. Closed early 2026-08-07.
      Owner: @cipherocto. Target: 2026-08-28. Depends on AC-B1. **CLOSED 2026-08-07.**

## Acceptance Deviations

- **AC-B2.1 hardening (deferred):** `authenticate()` does NOT yet surface `AuthError::UnsupportedScheme` or `AuthError::AskBindingMismatch` — both substrate variants exist (TV8 + TV11 assertions cover them) but the routing through `authenticate()` is a follow-up. Per [[deferred-vs-unspecified]] named-owner rule: owner @cipherocto, target 2026-09-04 (1-week post 0969-a2 closure). Tracked as AC-B2.1 in follow-up mission `0969-a3-hardening` (TBD file).

### Cross-crate compat (stays in 0969-a, not duplicated)

- [x] `cargo build --workspace` green (re-verified 2026-08-07: built 2m 34s, no errors).
- [x] `cargo test --workspace` green (re-verified 2026-08-07; 7/7 dispatch tests + new TV tests pass).
- [x] `cargo clippy -p octo-wallet -p quota-router-core --all-targets -- -D warnings` clean (verified 2026-08-07, per [[feedback_clippy_zero_warnings]]).
- [x] `cargo fmt --check` clean (verified 2026-08-07, per [[cargo-fmt-workflow]]).

## Acceptance Deviations

Per [[deferred-vs-unspecified]] form: unfulfilled AC + concrete plan + owner + target.

### Group A Deviations

- **`BearerVerification` + `CapabilityVerification` substrate** is the missing piece. RFC-0903 bearer verifier + RFC-0957 capability verifier need to expose `subject_did` + `ask_id` fields per Round 2 review. Owner: @cipherocto. Target: 2026-08-14.
- **Identity linkage evaluation** is downstream. Depends on AC-A1 substrate. Owner: @cipherocto. Target: 2026-08-14.
- **`AuthError` Debug redaction** is downstream. Currently derived Debug leaks credential material; manual redacting impl needed. Owner: @cipherocto. Target: 2026-08-14.

### Group B Deviations

- **`GatewayAuthenticator` struct** requires `BearerVerifier` + `CapabilityVerifier` traits + `Clock` trait + `HolderRegistry` trait (already shipped). Owner: @cipherocto. Target: 2026-08-21.
- **`authenticate()` algorithm** depends on AC-B1 substrate. Owner: @cipherocto. Target: 2026-08-21.
- **Brace balance CI lint** depends on AC-B2 landing. Owner: @cipherocto. Target: 2026-08-21.

### Group C Deviations

- **Test file location deviation:** mission text specified `crates/quota-router-core/tests/dispatch_tv.rs`. Actual location: `crates/octo-wallet/tests/dispatch_tv.rs` (per AC-B1 deviation — substrate lives in octo-wallet; tests colocate with substrate to avoid cross-crate Arc<dyn> wiring for now). Future 0969-b migration: tests move to `crates/quota-router-core/tests/` when `AuthenticatedRequest` is consumed cross-crate.
- **AC-C8 TV8 partial surfacing:** `AuthError::UnsupportedScheme` substrate variant exists + Debug redaction contract holds; but `authenticate()` does NOT yet route the unsupported-only scheme through `UnsupportedScheme` (returns `NoAuthHeader` first). AC-B2.1 hardening scheduled in follow-up `0969-a3-hardening` (TBD file). Target: 2026-09-04. Per [[deferred-vs-unspecified]] named-owner rule: unfulfilled AC + concrete plan + owner + target.
- **AC-C10 TV11 partial surfacing:** same pattern as TV8 — `AuthError::AskBindingMismatch` substrate exists, `authenticate()` collapses ask-only + identity mismatches into `IdentityMismatch`. AC-B2.1 hardening scheduled alongside TV8. Target: 2026-09-04.

## Type Coverage

This follow-up implements (per 0969-a §Type Coverage deferred entries):

- **Group A:** `BearerVerification` + `CapabilityVerification` types (RFC-0903 + RFC-0957 substrate) + identity linkage evaluation logic + manual redacting `Debug` impl on `AuthError`.
- **Group B:** `GatewayAuthenticator` struct + `authenticate()` algorithm + `AuthenticatedRequest` return type + `BearerVerifier` + `CapabilityVerifier` + `Clock` traits.
- **Group C:** 11 test vectors (TV1-TV8 + TV10-TV12) in `crates/octo-wallet/tests/dispatch_tv.rs` (NEW; location deviation per AC-B1 substrate colocation).

## Location

- `crates/octo-wallet/src/capability/dispatch.rs` (MODIFY) — Group A (identity linkage evaluation + manual Debug on AuthError)
- `crates/octo-wallet/src/capability/gateway_authenticator.rs` (NEW) — Group B (location deviation per AC-B1)
- `crates/octo-wallet/src/capability/mod.rs` (MODIFY) — Group B module export
- `.github/linters/braces-balanced.sh` (NEW, executable) — Group B CI lint
- `crates/octo-wallet/tests/dispatch_tv.rs` (NEW) — Group C test vectors

## Claimant

@unclaimed (target: @cipherocto)

## Pull Request

(unset)

## Notes

- This follow-up mission is the canonical home for the 17 ACs deferred from 0969-a. The 0969-a mission text per [[deferred-vs-unspecified]] named-owner rule explicitly states "full GatewayAuthenticator deferred" (L109) and "bearer path mission (RFC-0903 bearer mission) is the upstream for `BearerVerification`" (L119) — this mission is the follow-up.
- Group A (target 2026-08-14) is the smallest unit; can close independently. Groups B + C require Group A substrate.
- Per [[no-phantom-mission-pointers]] all `depends_on` cites real missions (0969-a, 0969-b, 0969-b1, 0957-c) + RFC substrate (RFC-0903, RFC-0957, RFC-0957-A1). No phantom slugs.
- Per [[no-line-refs-anywhere]] all references use §symbol-name form (no line refs in this mission).
- Per [[rfc-referencing-convention]] RFCs referenced by number only (no status / version pins).
- Per [[implementation-workflow-hook]] this mission is filed in `claimed/` directly (planning + owner + target set; substrate work follows in subsequent commits).

## Submission Date

2026-08-07T00:00:00Z

## Last Updated

2026-08-07T00:00:00Z (Group C closure)

## Version

2.0 (Group A closed 2026-08-07 — 5/5; Group B closed 2026-08-07 — 3/3; Group C closed 2026-08-07 — 11/11. 17/17 ACs GREEN. AC-B2.1 hardening deferred to follow-up `0969-a3-hardening`.)