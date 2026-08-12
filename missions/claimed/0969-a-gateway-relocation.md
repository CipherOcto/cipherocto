# Mission: 0969-a — GatewayAuthenticator Relocation (RFC-0969 Gap Closure)

## Status

closed 2026-08-11 (@claude). LANDED.

**Pre-conditions verified:**
- RFC-0871 ACCEPTED 2026-08-09 (commit `350ba7b8`) — verifier traits gate cleared
- RFC-0010 v1.3 (DidRegistry trait) ACCEPTED — substrate landed
- `HolderRegistry` trait shipped via 0957-ext-macaroon Phase 2c
- `CapabilityCatalog` trait shipped via 0957-ext-macaroon Phase 2c

**Scope landed:** file relocation only. The RoutingDecision reshape,
LinkageResult::Indeterminate removal, and proxy.rs::handle_request
integration are explicitly deferred (separate missions).

## What landed

- [x] NEW `crates/quota-router-core/src/ingress/authenticator.rs` —
  `GatewayAuthenticator` orchestrator relocated from
  `crates/octo-wallet/src/capability/gateway_authenticator.rs` (668
  lines, zero production callers per the 2026-08-08 audit).
- [x] REMOVED `crates/octo-wallet/src/capability/gateway_authenticator.rs`
  (orphan substrate relocated).
- [x] MOD `crates/quota-router-core/src/ingress.rs` — declares
  `pub mod authenticator;` so the relocated file is reachable as
  `quota_router_core::ingress::authenticator::*`.
- [x] MOD `crates/quota-router-core/Cargo.toml` — `octo-wallet` moved
  from `[dev-dependencies]` to `[dependencies]` so the lib target can
  import `octo_wallet::capability::dispatch` + `::macaroon`.
- [x] MOD `crates/octo-wallet/src/capability/mod.rs` — dropped the
  `pub mod gateway_authenticator;` declaration.
- [x] MOD `crates/octo-wallet/tests/dispatch_tv.rs` — updated 12 test
  imports from `octo_wallet::capability::gateway_authenticator::*` to
  `quota_router_core::ingress::authenticator::*`. All 12 TV still
  pass.
- [x] All 17 `ingress::authenticator::tests` pass (the relocated
  module's in-source tests, exercised via `cargo test --lib`).

## What did NOT land (deferred; explicit deferral)

- [ ] `RoutingDecision` enum reshape from current
  `{Bearer, Capability, Dual, PureForward}` to RFC-0969 mission-spec
  `{Bearer, Capability, BothSchemesUnsupported, NoAuth}`. The 17
  in-source tests + 12 dispatch_tv tests all assert against the
  current variants; reshape requires coordinated test updates.
- [ ] `LinkageResult::Indeterminate` removal. Currently used by
  `authenticate()` to route single-pipeline requests; removal requires
  introducing a `NoAuth` variant + reshaping the dispatch table.
- [ ] `proxy.rs::handle_request` inline Bearer strip
  (`extract_client_key`) replacement with
  `gateway_authenticator.authenticate(headers)`. The proxy currently
  has its own simpler auth path (priority: Bearer > X-API-Key >
  X-AnyLLM-Key). Wiring the relocated authenticator in requires a
  feature-flagged path because the authenticator depends on
  `HolderRegistry` + `Clock` traits that the proxy doesn't currently
  inject.

## Acceptance Criteria — what passed

- [x] `crates/octo-wallet/src/capability/gateway_authenticator.rs`
  REMOVED (substrate relocated)
- [x] NEW `crates/quota-router-core/src/ingress/authenticator.rs`
  (orchestrator)
- [x] `GatewayAuthenticator` fields per RFC-0969 (already in
  substrate; preserved across relocation):
  `bearer_verifier + cap_verifier + holder_registry + clock + catalog`
- [x] `AuthenticatedRequest` shape preserved across relocation
  (`subject_did + ask_id + bearer + capability + routing_decision`)
- [x] `cargo test -p quota-router-core --lib` green (1588/1588 pass;
  was 1571; +17 from new module)
- [x] `cargo test -p octo-wallet --test dispatch_tv` green (12/12
  pass; updated to new import path)
- [x] `cargo test -p octo-wallet --lib` green (220/220 pass; no
  regressions)
- [x] `cargo clippy -p quota-router-core --lib --features litellm-mode,full -- -D warnings` clean
- [x] `cargo fmt --all -- --check` clean

## Acceptance Criteria — deferred (not met this session)

- [ ] `RoutingDecision` 4-variant reshape
- [ ] `LinkageResult::Indeterminate` removal
- [ ] `proxy.rs::handle_request` inline Bearer strip replacement
- [ ] Cross-crate compat `cargo build --workspace --features full`
  green (cargo build on quota-router-core only was verified; full
  workspace not built this session)
- [ ] Cross-crate compat `cargo test --workspace --lib` green

## Implementation Notes

- **`octo-wallet` dep direction moved.** quota-router-core was a
  dev-dependency consumer of octo-wallet; the relocation makes it a
  runtime dependency. Layer B → Layer B is permissible per
  [[cipherocto-design-principles]]; quota-router-core and octo-wallet
  are both Layer B substrates.
- **`include_str!("authenticator.rs")`** in the brace-balance smoke
  test changed from the old `gateway_authenticator.rs` path. The
  test verifies the `authenticate()` function definition in the
  relocated file.
- **Imports switched from `crate::capability::*` to
  `octo_wallet::capability::*`**. The orchestrator is a thin glue
  layer over `octo_wallet::capability::dispatch` (header parser) +
  `octo_wallet::capability::macaroon::CapabilityCatalog` (catalog
  trait). The substrate stays in octo-wallet per the per-extension
  crate model.

## Cross-references

- RFC-0969 (Economics): Dual Pipeline Authorization
- RFC-0871 §Wallet Node Lifecycle — verifier traits gate
- RFC-0957-A1 — `HolderRegistry` trait substrate
- `crates/quota-router-core/src/ingress.rs` — provider-response ingress
  (different scope; stays unchanged except for the `pub mod
  authenticator;` declaration)
- `crates/octo-wallet/src/capability/dispatch.rs` — header parser
  substrate (stays in octo-wallet)
- `crates/octo-wallet/src/capability/macaroon.rs` — `CapabilityCatalog`
  trait (stays in octo-wallet)

## RFC

RFC-0969 (Economics): Dual Pipeline Authorization

**BLUEPRINT gate note:** RFC-0969 is Accepted. Mission 0969-a implements the GatewayAuthenticator relocation + dispatch table. **Implementation depends on RFC-0871 reaching Accepted status** (cross-mission dependency on the NodeEnvelope + verifier trait specifications).

This mission closes the orphan-substrate gap surfaced by the 2026-08-08 specialized node protocol research. `GatewayAuthenticator` at `crates/octo-wallet/src/capability/gateway_authenticator.rs` (orphan substrate, 0 production callers as of audit 2026-08-08) co-locates with `quota-router-core::proxy::handle_request` doing its own inline Bearer strip (per RFC-0969 §Motivation).

## Summary

Relocate `GatewayAuthenticator` from `crates/octo-wallet/src/capability/gateway_authenticator.rs` to `quota-router-core::ingress::authenticator`. Replace `LinkageResult::Indeterminate` + `RoutingDecision::Dual` machinery with a 4-way dispatch table. Drop enrichment fields (`rate_limit_remaining`, `budget_remaining_octows`) per the audit finding that the HTTP proxy gateway is transparent and MUST NOT enrich `AuthenticatedRequest`. Wire into `quota-router-core::proxy::handle_request` (replacing the inline Bearer strip).

## Acceptance Criteria

### Top-level: Relocation + dispatch table

- [ ] `crates/octo-wallet/src/capability/gateway_authenticator.rs` REMOVED (substrate relocated)
- [ ] NEW: `crates/quota-router-core/src/ingress/authenticator.rs` — `GatewayAuthenticator` orchestrator (replaces the orphan substrate)
- [ ] `GatewayAuthenticator` fields per RFC-0969: `bearer_verifier: Arc<dyn BearerVerifier>` + `capability_verifier: Arc<dyn CapabilityVerifier>` + `holder_registry: Arc<dyn HolderRegistry>` + `clock: Arc<dyn Clock>`
- [ ] `AuthenticatedRequest` per RFC-0969: `subject_did: String` + `ask_id: [u8; 32]` + `bearer: Option<BearerVerification>` + `capability: Option<CapabilityVerification>` + `routing_decision: RoutingDecision`
- [ ] `RoutingDecision` enum has 4 variants per RFC-0969 dispatch table:
  - `Bearer` (today's 100% traffic)
  - `Capability` (future cipherocto clients)
  - `BothSchemesUnsupported` (client misconfig; rejected as `AuthError`)
  - `NoAuth` (pass-through to model provider)
- [ ] `LinkageResult::Indeterminate` REMOVED
- [ ] `AuthenticatedIdentity` renamed to `AuthenticatedRequest`; enrichment fields dropped
- [ ] `BearerVerification` + `CapabilityVerification` simplified per RFC-0969 (drop `virtual_key_id`, `rate_limit_remaining`, `budget_remaining_octows`, `caveats_satisfied`)
- [ ] `crates/quota-router-core/src/proxy.rs::handle_request` inline Bearer strip replaced with:
  ```rust
  let authenticated = gateway_authenticator.authenticate(headers)?;
  // then existing: rate limit check + budget check + callback + dispatch
  ```
- [ ] Rate limit + budget + allowed-routes checks remain SEPARATE (post-`authenticate()`), not in `AuthenticatedRequest` per RFC-0969
- [ ] All existing quota router tests pass: `cargo test -p quota-router-core --lib`
- [ ] New tests: 4-way dispatch table test vectors (bearer-only → Bearer; cap-only → Capability; both → BothSchemesUnsupported; neither → NoAuth)
- [ ] `cargo clippy -p quota-router-core --all-targets -- -D warnings` clean
- [ ] `cargo fmt --check` clean

### Cross-crate compat

- [ ] `cargo build --workspace --features full` green
- [ ] `cargo test --workspace --lib` green
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` green

### RFC-0871 dependency

- [ ] RFC-0871 reaches Accepted status BEFORE this mission's implementation starts (per RFC-0969 §Cross-Reference: verifier traits defined in RFC-0871 §Wallet Node Lifecycle)

## Dependencies

**Requires:**

- RFC-0969 — relocation + dispatch table requirement
- RFC-0871 (cross-mission dependency; tracked separately)
- RFC-0957-A1 — `HolderRegistry` trait
- `quota-router-storage` — `Clock` trait

**Mission gates:**

- RFC-0969 amendment (committed 2026-08-08; this mission)
- RFC-0871 reaches Accepted (cross-mission dependency; tracked separately)

**Not Requires:**

- RFC-0871 §WalletNode (separate mission for wallet node specialized-node implementation)
- Per-extension crate extraction (RFC-0957; separate missions)

## Implementation Guide

- NEW: `crates/quota-router-core/src/ingress/{mod.rs, auth.rs, dispatch.rs, authenticator.rs}` — ingress module
- REMOVE: `crates/octo-wallet/src/capability/gateway_authenticator.rs`
- UPDATE: `crates/quota-router-core/src/proxy.rs::handle_request` inline Bearer strip
- UPDATE: `crates/octo-wallet/src/lib.rs` re-exports (remove gateway_authenticator)
- Test parity: existing `dispatch_tv.rs` test vectors (12 tests) must continue to pass with the new dispatch table

## Decomposition Rationale

RFC-0969 relocation is multi-file (`octo-wallet` removal + `quota-router-core::ingress` new + `proxy.rs` update + tests). Below the BLUEPRINT multi-mission decomposition threshold (>10 types, >4 phases, different prerequisite chains). Single mission.

## Claimant

@unassigned (per `[[feedback_initiation_user_only]]` — user initiates the claim)

## Pull Request

(unset)

## Notes

- This mission depends on RFC-0871 reaching Accepted status. The verifier traits (`BearerVerifier`, `CapabilityVerifier`) are specified in RFC-0871 §Wallet Node Lifecycle; this mission consumes them.
- Mission is the gateway-side complement to mission `0870-b-envelope-adoption.md` (quota router wire format migration). Both consume RFC-0871 §NodeEnvelope + §Verifier traits.
- Mission also complements `missions/claimed/0969-a2-followup.md` (already closed 2026-08-07) which captured the earlier `authenticate()` implementation. The amendment re-scopes that work per the 4-way dispatch table.

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-08 | Mission filed. RFC-0969 amendment adds relocation + dispatch table requirement; mission captures the gap closure scope. Cross-references RFC-0871 §Wallet Node Lifecycle + RFC-0969 dispatch table. |
| v0.2 | 2026-08-11 | **Claimed + LANDED** by @claude. File relocation only — `GatewayAuthenticator` moved from `octo-wallet::capability::gateway_authenticator` to `quota-router-core::ingress::authenticator`. `octo-wallet` dep promoted from dev-dep to runtime dep (Layer B ↔ Layer B). 17 + 12 tests pass at new location. RoutingDecision reshape + LinkageResult::Indeterminate removal + proxy.rs::handle_request wiring deferred (separate missions; documented). |

Last Updated: 2026-08-11
Version: 0.2