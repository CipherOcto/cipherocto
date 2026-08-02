# Mission: Dual-Pipeline Gateway Authenticator (RFC-0969 §Phase 1)

## Status

Open

## RFC

RFC-0969 (Economics): Dual-Pipeline Authorization — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0969-dual-pipeline-authorization.md` (top-level decomposition mission)

## Summary

Implement RFC-0969 §Phase 1: header parser + gateway authenticator. Author `AuthHeader` enum (Bearer / CipherOcto-Cap / None / Unsupported), `DispatchSet` struct (parsed headers + identity linkage validation result), `GatewayAuthenticator` struct (authenticate + route), `authenticate(req: &Request) -> Result<AuthenticatedRequest, AuthError>` algorithm. Implement identity linkage rule (bearer.subject_did == cap.holder_did AND bearer.ask_id == cap.ask_id) — canonical cross-holder credential mixing defense (Finding A21). Implement bearer path with `BearerVerification::subject_did` + `ask_id` fields per Round 2.

Manual redacting `Debug` impls on `ParseError`, `AuthError`. Brace balance verified at `authenticate()` (R53-N1 fix).

## Acceptance Criteria

### Header parser

- [ ] `crates/octo-wallet/src/capability/dispatch.rs` (NEW) — `AuthHeader` enum: `Bearer(String)`, `CipherOctoCap(String)`, `None`, `Unsupported(String)`. Manual Debug impl (redact token strings).
- [ ] `parse_auth_headers(req: &Request) -> Result<DispatchSet, ParseError>` — multi-scheme parser. Bearer from `Authorization: Bearer <token>`; CipherOcto-Cap from `Authorization: CipherOcto-Cap <token>` OR `X-Capability-Token: <token>`. Duplicate `CipherOcto-Cap` headers return `ParseError::DuplicateCapabilityHeader`.
- [ ] Manual redacting Debug on `ParseError`.

### Identity linkage

- [ ] `DispatchSet` struct: parsed `bearer: Option<BearerVerification>`, `capability: Option<CapabilityVerification>`, `identity_linkage: LinkageResult { Linked { subject_did: Did, ask_id: AskId } | Mismatched | Indeterminate }`.
- [ ] Identity linkage rule: `bearer.is_some() && capability.is_some()` ⇒ assert `bearer.subject_did == cap.holder_did` AND `bearer.ask_id == cap.ask_id`. Mismatch returns `AuthError::IdentityMismatch`. Indeterminate (one present, other absent) returns `AuthError::Indeterminate`.

### Gateway authenticator

- [ ] `crates/quota-router-core/src/gateway/authenticator.rs` (NEW) — `GatewayAuthenticator` struct: `clock: Arc<dyn Clock>`, `holder_registry: Arc<dyn HolderRegistry>`, `bearer_verifier: Arc<dyn BearerVerifier>` (bearer substrate), `cap_verifier: Arc<dyn CapabilityVerifier>` (capability substrate).
- [ ] `authenticate(req: &Request) -> Result<AuthenticatedRequest, AuthError>` — entrypoint. Steps: parse headers → verify bearer (if present) → verify capability (if present) → check identity linkage → return `AuthenticatedRequest { subject_did, ask_id, capabilities: ..., bearer: ..., routing_decision }`.
- [ ] Brace balance verified at `authenticate()` (R53-N1 fix). CI lint enforces.

### Error types

- [ ] `AuthError` enum: `IdentityMismatch { bearer_did: Did, cap_did: Did }`, `AskBindingMismatch { bearer_ask: AskId, cap_ask: AskId }`, `BothInvalid { bearer_err: BearerError, cap_err: CapError }`, `RoutingLatencyExceeded { threshold_ms: u64, actual_ms: u64 }`, `DuplicateCapabilityHeader`, `NoAuthHeader`, `UnsupportedScheme(String)`, `Indeterminate`. All manual redacting Debug.

### Test vectors (RFC-0969 §Test Vectors, this sub-mission owns TV1, TV2, TV3, TV4, TV5, TV6, TV7, TV8, TV10, TV11, TV12)

- [ ] TV1: Bearer-Only Request — `Authorization: Bearer <token>` accepted; `subject_did` extracted; no capability required.
- [ ] TV2: Capability-Only Request — `Authorization: CipherOcto-Cap <token>` accepted; `subject_did` extracted from registry lookup.
- [ ] TV3: Bearer + Capability Request (Both Valid, Linked Identity) — both headers present; identity matches; `AuthenticatedRequest` populated.
- [ ] TV4: Bearer + Capability Request (Capability Invalid) — bearer valid, capability tampered; returns `AuthError::BothInvalid { bearer_err: None, cap_err: CapError::MacaroonInvalid }`.
- [ ] TV5: Bearer + Capability Request (Identity Mismatch) — both valid but `bearer.subject_did != cap.holder_did`; returns `AuthError::IdentityMismatch`.
- [ ] TV6: Duplicate Capability Header — two `CipherOcto-Cap` headers; returns `AuthError::DuplicateCapabilityHeader`.
- [ ] TV7: No Auth Header — request with no `Authorization` and no `X-Capability-Token`; returns `AuthError::NoAuthHeader`.
- [ ] TV8: Unsupported Auth Scheme — `Authorization: Basic <b64>`; returns `AuthError::UnsupportedScheme("Basic")`.
- [ ] TV10: Debug Redaction — `format!("{:?}", err)` contains `[REDACTED]` markers; grep test for credential material.
- [ ] TV11: Ask Binding Mismatch — both valid, identities linked, but `bearer.ask_id != cap.ask_id`; returns `AuthError::AskBindingMismatch`.
- [ ] TV12: Cross-Impl Routing Determinism — same `(bearer, cap, ask)` tuple routed by 2 different `GatewayAuthenticator` impls (mock + production); same routing decision.

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0903 — bearer path substrate (BearerVerification with subject_did + ask_id fields)
- RFC-0949 — SSO forward-compat hook
- RFC-0957 — capability path substrate (CapabilityVerification)
- RFC-0957-A1 — unified catalog (`HolderRegistry` consumed here via `Arc<dyn HolderRegistry>`)

**Requires (mission gates):**

- `missions/open/0969-dual-pipeline-authorization.md` (top-level)
- `missions/claimed/0957-a-capability-token-macaroon.md` (in progress) — base capability path
- `missions/open/0957-c-holder-registry-impl.md` — `HolderRegistry` consumed via `Arc<dyn HolderRegistry>` slot
- `missions/open/0957-d-wire-resolver-update.md` — `VerifyContext::holder_registry` slot shared

```yaml
depends_on:
  - mission-0957-c-holder-registry-impl # HolderRegistry consumed via Arc<dyn>
  - mission-0957-d-wire-resolver-update # VerifyContext slot shared
  - mission-0957-a-capability-token-macaroon # capability path substrate
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `AuthHeader` enum (Bearer / CipherOcto-Cap / None / Unsupported)
- `DispatchSet` struct
- `GatewayAuthenticator` struct
- `authenticate` algorithm
- `ParseError` enum
- `AuthError` enum (IdentityMismatch, AskBindingMismatch, BothInvalid, RoutingLatencyExceeded, DuplicateCapabilityHeader, NoAuthHeader, UnsupportedScheme, Indeterminate)
- `BearerVerification` extensions (subject_did, ask_id fields)
- `CipherOcto-Cap` auth scheme constant
- Identity linkage rule
- Manual redacting Debug impls on `ParseError`, `AuthError`

## Location

- `crates/octo-wallet/src/capability/dispatch.rs` (NEW)
- `crates/quota-router-core/src/gateway/authenticator.rs` (NEW)
- `crates/quota-router-core/src/gateway/mod.rs` (MODIFY) — module exports

## Claimant

@unclaimed

## Pull Request

(unset)

## Notes

- Brace balance verified at `authenticate()` (R53-N1 fix). CI lint: `bash .github/linters/braces-balanced.sh authenticate` runs on every PR touching the function.
- TV9 (Dual-Issuance Atomicity) and the `mint_dual` algorithm + `MintError` live in sub-mission 0969-b. This sub-mission owns 11 of 12 vectors.
- The bearer path mission (RFC-0903 bearer mission) is the upstream for `BearerVerification` with `subject_did` + `ask_id` fields. If that mission is incomplete, this sub-mission's TV1/TV3/TV4/TV5/TV11 are `[ ]` until then.
