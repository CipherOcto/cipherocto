# Mission: 0969-a — GatewayAuthenticator Relocation (RFC-0969 Gap Closure)

## Status

Open (2026-08-08). RFC-0969 amendment adds the relocation + 4-way dispatch table; this mission implements the move.

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

RFC-0969 relocation is multi-file (`octo-wallet` removal + `quota-router-core::ingress` new + `proxy.rs` update + tests). Below the BLUEPRINT §Multi-Mission Decomposition threshold (>10 types, >4 phases, different prerequisite chains). Single mission.

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

Last Updated: 2026-08-08
Version: 0.1