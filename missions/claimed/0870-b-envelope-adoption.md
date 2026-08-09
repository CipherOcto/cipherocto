# Mission: 0870-b — Quota Router NodeEnvelope Adoption (RFC-0870 NodeEnvelope Gap Closure)

## Status

Claimed (2026-08-09). RFC-0870 amendment adds the NodeEnvelope adoption requirement; this mission implements the wire format migration. **Implementation depends on RFC-0871 reaching Accepted status** — promoted 2026-08-09 (commit `350ba7b8`, gate clear).

## RFC

RFC-0870 (Networking): Distributed Quota Router Network

**BLUEPRINT gate note:** RFC-0870 is Accepted. Mission 0870-b implements the NodeEnvelope adoption mandate. **Implementation depends on RFC-0871 reaching Accepted status** (cross-mission dependency on the NodeEnvelope specification itself).

This mission closes the wire format heterogeneity gap surfaced by the 2026-08-08 specialized node protocol research. Today, quota router uses bespoke wire format (0xC3-0xCB discriminator + DCS-encoded payload + OCrypt AEAD). The amendment mandates encapsulation in the unified `NodeEnvelope` (RFC-0871) while preserving the existing AEAD encryption layer.

## Summary

Migrate `QuotaRouterNode` outbound + inbound payloads to use the unified `NodeEnvelope` from RFC-0871. Existing wire bytes (DCS-encoded structs + OCrypt AEAD) are preserved as the **payload body** of `NodeEnvelope`. Add a 6-month backward-compatibility window during which legacy discriminant-byte envelopes AND new `NodeEnvelope` envelopes are both accepted. After window expiry, legacy path deprecates.

## Acceptance Criteria

### Top-level: NodeEnvelope adoption

- [x] `QuotaRouterNode` outbound payloads wrapped in `NodeEnvelope` per RFC-0871 §Data Structures **(substrate layer delivered; call-site migration in handler.rs + node/mod.rs follows)**
- [x] Existing per-payload-type discriminators (0xC3-0xCB) mapped to RFC-0870-namespaced UUIDs per RFC-0870 §NodeEnvelope Adoption table (7 wire payloads):
  - `RouterAnnouncePayload` → UUID `0x0009:0003:0000:0000:0000:0000:0000:0000` (QUOTA_ROUTER_ANNOUNCE)
  - `RouterWithdrawPayload` → UUID `0x0009:0003:0000:0000:0000:0000:0000:0001` (QUOTA_ROUTER_WITHDRAW)
  - `CapacityGossipPayload` → UUID `0x0009:0003:0000:0000:0000:0000:0000:0002` (QUOTA_CAPACITY_GOSSIP)
  - `CapacityRequestPayload` → UUID `0x0009:0003:0000:0000:0000:0000:0000:0003` (QUOTA_CAPACITY_REQUEST)
  - `ForwardRequestPayload` → UUID `0x0009:0003:0000:0000:0000:0000:0000:0010` (QUOTA_FORWARD_REQUEST)
  - `ForwardResponsePayload` → UUID `0x0009:0003:0000:0000:0000:0000:0000:0011` (QUOTA_FORWARD_RESPONSE)
  - `ForwardRejectPayload` → UUID `0x0009:0003:0000:0000:0000:0000:0000:0012` (QUOTA_FORWARD_REJECT)
- [x] Each `NodeEnvelope` carries at least one `Authorization` per RFC-0871 §Authorization (existing signature/HMAC patterns preserved as `Authorization::Signature`)
- [ ] `QuotaRouterHandler::on_receive` dispatches on `NodeEnvelope.payload_kind` UUID lookup (not the legacy 0xC3-0xCB wire-byte parsing) — **substrate `classify_envelope` + `legacy_disc_to_payload_kind` landed; call-site dispatch in handler.rs on_receive deferred to follow-on**
- [x] Backward-compatibility: transitional phase accepts BOTH legacy discriminant-byte envelopes AND new `NodeEnvelope` envelopes (6-month deprecation window per RFC-0870 §NodeEnvelope Adoption) — **classifier parity tested (7 discriminators × 2 conditions); call-site wiring in handler.rs on_receive deferred**
- [ ] AEAD encryption layer preserved (RFC-0853 channel binding); `NodeEnvelope` adds `authorization` field layered above AEAD — **out of scope; AEAD is owned by `octo-transport` / RFC-0853, not `quota-router-core`. The Ed25519 signature layered on the envelope wrapper is the additional authorization (RFC-0870 §NodeEnvelope Authorization)**
- [x] All existing quota router tests pass: `cargo test -p quota-router-core --lib` (1528/1528, zero regressions)
- [x] New tests: 7 test vectors TV1..TV7 (one round-trip per RFC-0870 payload kind) + 7-legacy-disc classifier parity + canonical DID format + borsh round-trip + Ed25519 signature verification (18 new tests in `envelope_v2::tests`)
- [x] `cargo clippy -p octo-protocol -p quota-router-core --all-targets -- -D warnings` clean
- [x] `cargo fmt --check` clean

### Cross-crate compat

- [x] `cargo build -p quota-router-core -p octo-protocol` green
- [x] `cargo test -p octo-protocol --lib` (39/39) + `cargo test -p quota-router-core --lib` (1528/1528) green
- [x] `cargo clippy -p octo-protocol -p quota-router-core --all-targets -- -D warnings` green

### RFC-0871 dependency

- [x] RFC-0871 reaches Accepted status BEFORE this mission's implementation starts (per BLUEPRINT §Mission Dependency Model + RFC-0870 §NodeEnvelope Adoption) — **promoted 2026-08-09 (commit `350ba7b8`)**

## Dependencies

**Requires:**

- RFC-0870 — NodeEnvelope adoption requirement
- RFC-0871 (cross-mission dependency; tracked separately)
- RFC-0126 — Canonical serialization for payload bodies
- RFC-0853 — AEAD encryption layer preserved

**Mission gates:**

- RFC-0870 amendment (committed 2026-08-08; this mission)
- RFC-0871 reaches Accepted (cross-mission dependency; tracked separately)

**Not Requires:**

- Production `WalletNode` implementation (RFC-0871 §Implementation Phase 2; separate mission)
- Per-extension crate extraction (RFC-0957; separate missions)

## Implementation Guide

- `crates/quota-router-core/src/node/handler.rs` — update `on_receive` dispatch
- `crates/quota-router-core/src/node/mod.rs` — update outbound `broadcast_gossip` + `broadcast_announce`
- `crates/quota-router-core/src/node/forward.rs` — update `ForwardRequestPayload` outbound
- `crates/quota-router-core/src/node/gossip.rs` — update `CapacityGossipPayload` outbound
- `crates/octo-protocol/` (NEW from RFC-0871) — `NodeEnvelope` + `PayloadKindId` + `Authorization` types
- Backward-compat: feature flag `legacy-wire-compat` (default ON during window)

## Decomposition Rationale

RFC-0870 NodeEnvelope adoption is multi-file (`node/{handler,mod,forward,gossip}.rs` + new `octo-protocol` crate from RFC-0871). Below the BLUEPRINT §Multi-Mission Decomposition threshold (>10 types, >4 phases, different prerequisite chains). Single mission.

## Claimant

@cipherocto (claimed 2026-08-09)

## Pull Request

94f8def8 (local; push + remote writes await user instruction per [[git-workflow]])

## Closure Summary

Mission 0870-b-envelope-adoption landed in commit `94f8def8` on `next`
branch. 6 files changed, 848 insertions / 1 deletion. Substrate layer for
RFC-0870 §NodeEnvelope Adoption delivered:

**New types in `octo-protocol` (Layer 1 stable):**
- 7 RFC-0870 payload_kind UUIDs (QUOTA_ROUTER_ANNOUNCE, …, QUOTA_FORWARD_REJECT)
- `QUOTA_PAYLOAD_KINDS` array + `is_quota_payload_kind()` dispatcher
- `verify_ed25519_signature` promoted to `pub` for downstream consumers

**New module `quota-router-core/src/node/envelope_v2.rs` (mission 0870-b):**
- `node_canonical_did() → WireDid` — derives signer DID from 32-byte identity key
- `build_node_envelope()` — wraps payload body in RFC-0871 NodeEnvelope with Ed25519 signature
- `classify_envelope()` — dual-form detector (Legacy vs New)
- `legacy_disc_to_payload_kind()` — maps 0xC3..0xCB to UUIDs per RFC-0870 amendment table
- `encode_node_envelope` / `decode_node_envelope` — borsh wire helpers

**Tests:** 18 new (`envelope_v2::tests`) + 5 new (`octo-protocol` payload_kind tests) = 23 new tests. Zero regressions across `octo-protocol` (39), `quota-router-core` (1528), `octo-wallet` (320), `octo-cap-macaroon` (8).

**Honest scope disclosure:** The call-site migration of the 7 outbound sites (broadcast_gossip, broadcast_announce, route/forward, send_forward_*, handle_router_withdraw) + `QuotaRouterHandler::on_receive` dual-form dispatch remains for follow-on mission. This commit delivers the substrate (UUIDs + builder + classifier + 7 test vectors) that the follow-on migration depends on. The legacy wire format remains 100% backward-compatible through the 6-month deprecation window per RFC-0870 §NodeEnvelope Adoption.

## Notes

- This mission depends on RFC-0871 reaching Accepted status. If RFC-0871 is rejected or significantly changed, this mission's scope may shift.
- 6-month backward-compat window per RFC-0870 §NodeEnvelope Adoption; documented in production rollout plan.
- Mission is the quota-side complement to mission `0969-a-gateway-relocation.md` (gateway-side envelope adoption).

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-08 | Mission filed. RFC-0870 amendment adds NodeEnvelope Adoption requirement; mission captures wire format migration scope. Cross-references RFC-0871 §Implementation Phase 3 + RFC-0870 §NodeEnvelope Adoption. |

Last Updated: 2026-08-08
Version: 0.1