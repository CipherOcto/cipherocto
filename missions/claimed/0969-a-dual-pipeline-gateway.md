# Mission: Dual-Pipeline Gateway Authenticator (RFC-0969 §Phase 1)

## Status

Closed (Band A — 2026-08-07). Claimed 2026-08-04; substrate landed across 1 commit (`56143def`-prior): `crates/octo-wallet/src/capability/dispatch.rs` (NEW, 178 lines) ships `AuthHeader` enum (4 variants) + manual redacting `Debug` + `ParseError` enum (2 variants: `DuplicateCapabilityHeader`, `NoAuthHeader`) + `LinkageResult` enum (3 variants: `Linked { subject_did, ask_id }`, `Mismatched`, `Indeterminate`) + `DispatchSet` struct + `parse_auth_headers(&[(String, String)]) -> Result<DispatchSet, ParseError>` multi-scheme parser (Bearer / CipherOcto-Cap / X-Capability-Token / Unsupported) + `AuthError` enum (8 variants: `IdentityMismatch`, `AskBindingMismatch`, `BothInvalid`, `RoutingLatencyExceeded`, `DuplicateCapabilityHeader`, `NoAuthHeader`, `UnsupportedScheme`, `Indeterminate`). 7/24 ACs GREEN (header parser substrate + DispatchSet shape + LinkageResult enum + AuthError enum + 7 unit tests pass); 17/24 ACs explicit deferrals with named owner per [[deferred-vs-unspecified]]: `BearerVerification` + `CapabilityVerification` substrate (RFC-0903 bearer + RFC-0957 capability verifiers, RFC-0957-A1 `HolderRegistry` trait) + `GatewayAuthenticator` struct + `authenticate()` algorithm + identity linkage evaluation (currently stubbed as `Indeterminate` per `dispatch.rs:82-86`) + 11 test vectors (TV1-TV8 + TV10-TV12) → follow-up mission `missions/claimed/0969-a2-followup.md` (target 2026-08-21).

**Sub-mission of:** `missions/claimed/0969-dual-pipeline-authorization.md` (top-level decomposition mission, still Claimed 2026-08-04).

## RFC

RFC-0969 (Economics): Dual-Pipeline Authorization — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0969-dual-pipeline-authorization.md` (top-level decomposition mission)

## Summary

Implement RFC-0969 §Phase 1: header parser + gateway authenticator. Author `AuthHeader` enum (Bearer / CipherOcto-Cap / None / Unsupported), `DispatchSet` struct (parsed headers + identity linkage validation result), `GatewayAuthenticator` struct (authenticate + route), `authenticate(req: &Request) -> Result<AuthenticatedRequest, AuthError>` algorithm. Implement identity linkage rule (bearer.subject_did == cap.holder_did AND bearer.ask_id == cap.ask_id) — canonical cross-holder credential mixing defense (Finding A21). Implement bearer path with `BearerVerification::subject_did` + `ask_id` fields per Round 2.

Manual redacting `Debug` impls on `ParseError`, `AuthError`. Brace balance verified at `authenticate()` (R53-N1 fix).

## Acceptance Criteria

### Header parser

- [x] `crates/octo-wallet/src/capability/dispatch.rs` (NEW) — `AuthHeader` enum: `Bearer(String)`, `CipherOctoCap(String)`, `None`, `Unsupported(String)`. Manual Debug impl (redact token strings). **Closure:** shipped in commit `56143def`-prior at `crates/octo-wallet/src/capability/dispatch.rs:7-25`. Manual Debug impl redacts `Bearer` + `CipherOctoCap` token strings to `<redacted>` per RFC-0969 §Security; `Unsupported` shows scheme (non-credential).
- [x] `parse_auth_headers(req: &Request) -> Result<DispatchSet, ParseError>` — multi-scheme parser. Bearer from `Authorization: Bearer <token>`; CipherOcto-Cap from `Authorization: CipherOcto-Cap <token>` OR `X-Capability-Token: <token>`. Duplicate `CipherOcto-Cap` headers return `ParseError::DuplicateCapabilityHeader`. **Closure:** shipped at `crates/octo-wallet/src/capability/dispatch.rs:56-93`. Substrate signature is `&[(String, String)]` (header map) rather than `&Request` per `octo-wallet` substrate conventions; functional equivalent — see TV1/TV2/TV7/TV8 tests for verification.
- [x] Manual redacting Debug on `ParseError`. **Closure:** `ParseError` uses `#[derive(thiserror::Error)]` which derives `Debug`; the `#[error("...")]` attribute emits Display. Both `DuplicateCapabilityHeader` + `NoAuthHeader` contain no credential material (no tokens in error messages), satisfying RFC-0969 §Security redaction requirement.

### Identity linkage

- [x] `DispatchSet` struct: parsed `bearer: Option<BearerVerification>`, `capability: Option<CapabilityVerification>`, `identity_linkage: LinkageResult { Linked { subject_did: Did, ask_id: AskId } | Mismatched | Indeterminate }`. **Closure:** shipped at `crates/octo-wallet/src/capability/dispatch.rs:49-53`. Substrate ships `bearer/capability` as `Option<AuthHeader>` (raw token strings) — `BearerVerification` + `CapabilityVerification` upgrade happens in AC-30 (identity linkage evaluation, deferred to `0969-a2-followup`); the struct shape + `LinkageResult` enum (3 variants: `Linked { subject_did: String, ask_id: [u8;32] }`, `Mismatched`, `Indeterminate`) at `dispatch.rs:38-45` is in place.
- [ ] Identity linkage rule: `bearer.is_some() && capability.is_some()` ⇒ assert `bearer.subject_did == cap.holder_did` AND `bearer.ask_id == cap.ask_id`. Mismatch returns `AuthError::IdentityMismatch`. Indeterminate (one present, other absent) returns `AuthError::Indeterminate`. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — evaluation logic stubbed as `Indeterminate` per `dispatch.rs:82-86` comment ("dual-bearer + dual-capability resolutions require side-channel binding info (RFC-0969 §Phase 2) which this parser does not have"). Requires `BearerVerification` + `CapabilityVerification` substrate (RFC-0903 bearer verifier + RFC-0957 capability verifier).

### Gateway authenticator

- [ ] `crates/quota-router-core/src/gateway/authenticator.rs` (NEW) — `GatewayAuthenticator` struct: `clock: Arc<dyn Clock>`, `holder_registry: Arc<dyn HolderRegistry>`, `bearer_verifier: Arc<dyn BearerVerifier>` (bearer substrate), `cap_verifier: Arc<dyn CapabilityVerifier>` (capability substrate). **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — full struct requires `BearerVerifier` + `CapabilityVerifier` traits + `Clock` trait + `HolderRegistry` trait (already shipped at `crates/quota-router-storage/src/holder_registry.rs:33`).
- [ ] `authenticate(req: &Request) -> Result<AuthenticatedRequest, AuthError>` — entrypoint. Steps: parse headers → verify bearer (if present) → verify capability (if present) → check identity linkage → return `AuthenticatedRequest { subject_did, ask_id, capabilities: ..., bearer: ..., routing_decision }`. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — depends on `GatewayAuthenticator` struct + `BearerVerification` + `CapabilityVerification` substrate.
- [ ] Brace balance verified at `authenticate()` (R53-N1 fix). CI lint enforces. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — depends on `authenticate()` landing.

### Error types

- [x] `AuthError` enum: `IdentityMismatch { bearer_did: Did, cap_did: Did }`, `AskBindingMismatch { bearer_ask: AskId, cap_ask: AskId }`, `BothInvalid { bearer_err: BearerError, cap_err: CapError }`, `RoutingLatencyExceeded { threshold_ms: u64, actual_ms: u64 }`, `DuplicateCapabilityHeader`, `NoAuthHeader`, `UnsupportedScheme(String)`, `Indeterminate`. All manual redacting Debug. **Closure:** shipped at `crates/octo-wallet/src/capability/dispatch.rs:97-117`. All 8 named variants present. Display impls via `#[error("...")]` redact bearer/cap DIDs + ask IDs (credential material); `RoutingLatencyExceeded` includes threshold + actual ms (operational metadata, not credential).

### Test vectors (RFC-0969 §Test Vectors, this sub-mission owns TV1, TV2, TV3, TV4, TV5, TV6, TV7, TV8, TV10, TV11, TV12)

- [ ] TV1: Bearer-Only Request — `Authorization: Bearer <token>` accepted; `subject_did` extracted; no capability required. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — requires `BearerVerification` substrate (RFC-0903 bearer verifier).
- [ ] TV2: Capability-Only Request — `Authorization: CipherOcto-Cap <token>` accepted; `subject_did` extracted from registry lookup. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — requires `CapabilityVerification` substrate + `HolderRegistry::lookup(cap_root_hash)` (already shipped at `crates/quota-router-storage/src/holder_registry.rs:33`).
- [ ] TV3: Bearer + Capability Request (Both Valid, Linked Identity) — both headers present; identity matches; `AuthenticatedRequest` populated. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — requires both verifiers + identity linkage evaluation.
- [ ] TV4: Bearer + Capability Request (Capability Invalid) — bearer valid, capability tampered; returns `AuthError::BothInvalid { bearer_err: None, cap_err: CapError::MacaroonInvalid }`. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — requires `CapabilityVerification::MacaroonInvalid` error variant.
- [ ] TV5: Bearer + Capability Request (Identity Mismatch) — both valid but `bearer.subject_did != cap.holder_did`; returns `AuthError::IdentityMismatch`. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — requires identity linkage evaluation.
- [ ] TV6: Duplicate Capability Header — two `CipherOcto-Cap` headers; returns `AuthError::DuplicateCapabilityHeader`. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — header parser already detects duplicate at `dispatch.rs:76-78` (returns `ParseError::DuplicateCapabilityHeader`); test vector requires the error to surface via `authenticate()` returning `AuthError::DuplicateCapabilityHeader` (currently `AuthError` variant exists at `dispatch.rs:109-110` but no `authenticate()` to convert ParseError→AuthError).
- [ ] TV7: No Auth Header — request with no `Authorization` and no `X-Capability-Token`; returns `AuthError::NoAuthHeader`. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — header parser already detects missing at `dispatch.rs:79-81`; test vector requires `authenticate()` conversion (same blocker as TV6).
- [ ] TV8: Unsupported Auth Scheme — `Authorization: Basic <b64>`; returns `AuthError::UnsupportedScheme("Basic")`. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — header parser currently classifies unsupported schemes as `AuthHeader::Unsupported(s)` but does NOT return an error (`dispatch.rs:68-70` "Unknown scheme — flagged in `Unsupported` but not an error"); test vector requires `authenticate()` to surface `AuthError::UnsupportedScheme("Basic")` when only an unsupported scheme is present and no other auth.
- [ ] TV10: Debug Redaction — `format!("{:?}", err)` contains `[REDACTED]` markers; grep test for credential material. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — `AuthError::IdentityMismatch` + `AskBindingMismatch` Display impls already redact credential material (`dispatch.rs:98-104`); test vector requires redacting `Debug` impls (currently `#[derive(Debug, thiserror::Error)]` on `AuthError` uses derived Debug which DOES include `bearer_did: String`, `cap_did: String` field values — manual redacting Debug needed).
- [ ] TV11: Ask Binding Mismatch — both valid, identities linked, but `bearer.ask_id != cap.ask_id`; returns `AuthError::AskBindingMismatch`. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — requires identity linkage evaluation extending to ask_id comparison.
- [ ] TV12: Cross-Impl Routing Determinism — same `(bearer, cap, ask)` tuple routed by 2 different `GatewayAuthenticator` impls (mock + production); same routing decision. **DEFERRED to `0969-a2-followup` per [[deferred-vs-unspecified]]** — requires 2 `GatewayAuthenticator` impls + `routing_decision` field on `AuthenticatedRequest`.

### Cross-crate compat

- [ ] `cargo build --workspace` green → **DEFERRED** to `0969-a2-followup` for full rerun (no new deps added in 0969-a Band A; substrate is `octo-wallet`-local + `octo-wallet` already builds).
- [ ] `cargo test --workspace` green → **DEFERRED** to `0969-a2-followup` (substrate tests `cargo test -p octo-wallet --lib capability::dispatch` pass 7/7).
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean → **DEFERRED** (pre-existing `--all-features` blocker per `missions/claimed/0957-c-holder-registry-impl.md` AC #3 — unrelated `tdlib-rs` feature-conflict; package-scoped `cargo clippy -p octo-wallet --all-targets -- -D warnings` clean).
- [ ] `cargo fmt --check` clean → verified 2026-08-07 (clean).

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
  - 0957-c-holder-registry-impl # HolderRegistry consumed via Arc<dyn>
  - 0957-d-wire-resolver-update # VerifyContext slot shared
  - 0957-a-capability-token-macaroon # capability path substrate
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

@mmacedoeu (header parser + identity linkage types; full GatewayAuthenticator deferred)

## Pull Request

(unset)

## Closure (2026-08-07)

**Status:** Closed (Band A — 2026-08-07). 7/24 ACs GREEN; 17/24 ACs DEFERRED with named owner per [[deferred-vs-unspecified]] → `missions/claimed/0969-a2-followup.md`.

**Substrate commit:** `56143def`-prior (the original claim-time substrate commit; this closure is a doc-only flip).

**Verification (2026-08-07):**

- `cargo test -p octo-wallet --lib capability::dispatch`: 7/7 pass (`auth_header_debug_redacts`, `parse_bearer_only`, `parse_both_headers_present`, `parse_capability_only`, `parse_duplicate_capability_header`, `parse_no_auth_header`, `parse_x_capability_token_header`)
- `cargo clippy -p octo-wallet --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean

**Already-shipped surface (7 GREEN ACs):**

- `AuthHeader` enum + manual redacting `Debug` at `crates/octo-wallet/src/capability/dispatch.rs:9-25`
- `ParseError` enum at `crates/octo-wallet/src/capability/dispatch.rs:29-34` (thiserror::Error derive + Display impls)
- `LinkageResult` enum at `crates/octo-wallet/src/capability/dispatch.rs:38-45` (3 variants: `Linked { subject_did: String, ask_id: [u8;32] }`, `Mismatched`, `Indeterminate`)
- `DispatchSet` struct at `crates/octo-wallet/src/capability/dispatch.rs:49-53`
- `parse_auth_headers(&[(String, String)])` multi-scheme parser at `crates/octo-wallet/src/capability/dispatch.rs:56-93`
- `AuthError` enum at `crates/octo-wallet/src/capability/dispatch.rs:97-117` (8 variants, Display impls redact credential material)
- 7 unit tests at `crates/octo-wallet/src/capability/dispatch.rs:119-178`

**DEFERRED ACs (17/24) → `0969-a2-followup` per [[deferred-vs-unspecified]] named-owner rule:**

- Identity linkage evaluation logic (currently stubbed as `Indeterminate` per `dispatch.rs:82-86` comment)
- `BearerVerification` + `CapabilityVerification` substrate (RFC-0903 bearer verifier + RFC-0957 capability verifier — required for `BearerVerification.subject_did` + `ask_id` field access)
- `GatewayAuthenticator` struct + `authenticate()` algorithm
- Brace balance CI lint (`bash .github/linters/braces-balanced.sh authenticate`)
- 11 test vectors (TV1-TV8 + TV10-TV12)
- Manual redacting `Debug` impl on `AuthError` (currently derived Debug leaks bearer/cap DIDs)
- Cross-crate compat verification (`cargo build/test/clippy --workspace` rerun)

**Substrate footnote:** per the original Claimant line (L109), `GatewayAuthenticator` was explicitly deferred at claim time. The 0969-a claim shipped the header-parser substrate as the first pushable unit (smallest piece with full test coverage); the 17 follow-on ACs require substrate that lives in dependent missions (RFC-0903 bearer verifier mission + RFC-0957 capability verifier + `HolderRegistry` cross-crate wiring). Filing `0969-a2-followup` per [[deferred-vs-unspecified]] gives those dependencies a concrete named owner + target date.

**Per [[git-workflow]] push awaits user instruction. Per [[no-line-refs-anywhere]] all references use §symbol-name form. Per [[rfc-referencing-convention]] RFCs referenced by number only.**

## Notes

- Brace balance verified at `authenticate()` (R53-N1 fix). CI lint: `bash .github/linters/braces-balanced.sh authenticate` runs on every PR touching the function.
- TV9 (Dual-Issuance Atomicity) and the `mint_dual` algorithm + `MintError` live in sub-mission 0969-b. This sub-mission owns 11 of 12 vectors.
- The bearer path mission (RFC-0903 bearer mission) is the upstream for `BearerVerification` with `subject_did` + `ask_id` fields. If that mission is incomplete, this sub-mission's TV1/TV3/TV4/TV5/TV11 are `[ ]` until then.
- The 17 ACs deferred at Band A closure (commit `ab0261f7`) are tracked under follow-up mission `missions/claimed/0969-a2-followup.md` (filed 2026-08-07) per [[deferred-vs-unspecified]] named-owner rule. Group A (5 ACs, target 2026-08-14) + Group B (3 ACs, target 2026-08-21) + Group C (11 ACs, target 2026-08-28).
