# Mission: 0870-b — Quota Router NodeEnvelope Adoption (RFC-0870 v2.0 Gap Closure)

## Status

Open (2026-08-08). RFC-0870 v2.0 amendment adds the NodeEnvelope adoption requirement; this mission implements the wire format migration.

## RFC

RFC-0870 (Networking): Distributed Quota Router Network — Accepted v2.0 (2026-08-08 amendment)

**BLUEPRINT gate note:** RFC-0870 is Accepted. Mission 0870-b implements the v2.0 NodeEnvelope adoption mandate. **Implementation depends on RFC-0871 reaching Accepted status** (cross-mission dependency on the NodeEnvelope specification itself).

This mission closes the wire format heterogeneity gap surfaced by the 2026-08-08 specialized node protocol research. Today, quota router uses bespoke wire format (0xC3-0xCB discriminator + DCS-encoded payload + OCrypt AEAD). The amendment mandates encapsulation in the unified `NodeEnvelope` (RFC-0871) while preserving the existing AEAD encryption layer.

## Summary

Migrate `QuotaRouterNode` outbound + inbound payloads to use the unified `NodeEnvelope` from RFC-0871. Existing wire bytes (DCS-encoded structs + OCrypt AEAD) are preserved as the **payload body** of `NodeEnvelope`. Add a 6-month backward-compatibility window during which legacy discriminant-byte envelopes AND new `NodeEnvelope` envelopes are both accepted. After window expiry, legacy path deprecates.

## Acceptance Criteria

### Top-level: NodeEnvelope adoption

- [ ] `QuotaRouterNode` outbound payloads wrapped in `NodeEnvelope` per RFC-0871 §Data Structures
- [ ] Existing per-payload-type discriminators (0xC3-0xCB) mapped to RFC-0870-namespaced UUIDs per RFC-0870 v2.0 §NodeEnvelope Adoption table:
  - `RouterAnnouncePayload` → UUID `0x87,0x00,...`
  - `RouterWithdrawPayload` → UUID `0x87,0x01,...`
  - `CapacityGossipPayload` → UUID `0x87,0x02,...`
  - `CapacityRequestPayload` → UUID `0x87,0x03,...`
  - `ForwardRequestPayload` → UUID `0x87,0x10,...`
  - `ForwardResponsePayload` → UUID `0x87,0x11,...`
  - `ForwardRejectPayload` → UUID `0x87,0x12,...`
- [ ] Each `NodeEnvelope` carries at least one `Authorization` per RFC-0871 §Authorization (existing signature/HMAC patterns preserved as `Authorization::Signature`)
- [ ] `QuotaRouterHandler::on_receive` dispatches on `NodeEnvelope.payload_kind` UUID lookup (not the legacy 0xC3-0xCB wire-byte parsing)
- [ ] Backward-compatibility: transitional phase accepts BOTH legacy discriminant-byte envelopes AND new `NodeEnvelope` envelopes (6-month deprecation window per RFC-0870 v2.0)
- [ ] AEAD encryption layer preserved (RFC-0853 channel binding); `NodeEnvelope` adds `authorization` field layered above AEAD
- [ ] All existing quota router tests pass: `cargo test -p quota-router-core --lib`
- [ ] New tests: legacy envelope parsing → ok during compat window; legacy envelope parsing → fail after window expiry
- [ ] `cargo clippy -p quota-router-core --all-targets -- -D warnings` clean
- [ ] `cargo fmt --check` clean

### Cross-crate compat

- [ ] `cargo build --workspace --features full` green
- [ ] `cargo test --workspace --lib` green
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` green

### RFC-0871 dependency

- [ ] RFC-0871 reaches Accepted status BEFORE this mission's implementation starts (per BLUEPRINT §Mission Dependency Model + RFC-0870 v2.0 §NodeEnvelope Adoption)

## Dependencies

**Requires:**

- RFC-0870 (Accepted v2.0) — NodeEnvelope adoption requirement
- RFC-0871 (Planned; MUST be Accepted before implementation starts)
- RFC-0126 (Accepted) — Canonical serialization for payload bodies
- RFC-0853 (Accepted) — AEAD encryption layer preserved

**Mission gates:**

- RFC-0870 v2.0 amendment (committed 2026-08-08; this mission)
- RFC-0871 reaches Accepted (cross-mission dependency; tracked separately)

**Not Requires:**

- Production `WalletNode` implementation (RFC-0871 Phase 2; separate mission)
- Per-extension crate extraction (RFC-0957 v2.0; separate missions)

## Implementation Guide

- `crates/quota-router-core/src/node/handler.rs` — update `on_receive` dispatch
- `crates/quota-router-core/src/node/mod.rs` — update outbound `broadcast_gossip` + `broadcast_announce`
- `crates/quota-router-core/src/node/forward.rs` — update `ForwardRequestPayload` outbound
- `crates/quota-router-core/src/node/gossip.rs` — update `CapacityGossipPayload` outbound
- `crates/octo-protocol/` (NEW from RFC-0871) — `NodeEnvelope` + `PayloadKindId` + `Authorization` types
- Backward-compat: feature flag `legacy-wire-compat` (default ON during window)

## Decomposition Rationale

RFC-0870 v2.0 NodeEnvelope adoption is multi-file (`node/{handler,mod,forward,gossip}.rs` + new `octo-protocol` crate from RFC-0871). Below the BLUEPRINT §Multi-Mission Decomposition threshold (>10 types, >4 phases, different prerequisite chains). Single mission.

## Claimant

@unassigned (per `[[feedback_initiation_user_only]]` — user initiates the claim)

## Pull Request

(unset)

## Notes

- This mission depends on RFC-0871 reaching Accepted status. If RFC-0871 is rejected or significantly changed, this mission's scope may shift.
- 6-month backward-compat window per RFC-0870 v2.0; documented in production rollout plan.
- Mission is the quota-side complement to mission `0969-a-gateway-relocation.md` (gateway-side envelope adoption).

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-08 | Mission filed. RFC-0870 v2.0 amendment adds NodeEnvelope Adoption requirement; mission captures wire format migration scope. Cross-references RFC-0871 §Implementation Phase 3 + RFC-0870 v2.0 §NodeEnvelope Adoption. |

Last Updated: 2026-08-08
Version: 0.1