# Mission: Market Delivery Envelope Implementation (RFC-0959-A1 §Phase 1 + §Phase 2)

## Status

Open

## RFC

RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02

**Sub-mission of:** `missions/open/0959-a1-market-delivery.md` (top-level decomposition mission)

## Summary

Implement RFC-0959-A1 §Phase 1 (Data Structures + Algorithms) and §Phase 2 (Stoolap Transaction Wrapper). Author `BearerCapsule` (typed struct per RFC-0959-A1 §Out of Scope; NOT RFC-0903 virtual keys), `DealSettled` (4th settlement chain artifact joining `Ask` + `SettlementEvent` + `SettlementReceipt`), `DealSettledPayload`, `MarketDeliveryEnvelope` (3-segment wire: `envelope_id || bearer_capsule || capability_token`), `MarketDeliveryEnvelopePreimage` (signed bytes), `EnvelopeId` (32-byte BLAKE3 newtype, `Hash` impl), `DeliveryError` enum (6 variants). Author `deliver_at_settlement` algorithm + `stoolap_txn` atomic transaction wrapper + `append_deal_settled` + chain hash update. Implement `chain_tip_lock` CAS primitive to break the TOCTOU race (Finding A10).

Manual redacting `Debug` impls on all 6 security-bearing structs (MarketDeliveryEnvelopePreimage, EnvelopeId, DealSettled, DealSettledPayload, BearerCapsule, MarketDeliveryEnvelope).

## Acceptance Criteria

### Type definitions

- [ ] `crates/octo-wallet/src/capability/market_delivery.rs` (NEW) — `BearerCapsule` + `DealSettled` + `DealSettledPayload` + `MarketDeliveryEnvelope` + `MarketDeliveryEnvelopePreimage` + `EnvelopeId`. All 6 have manual redacting Debug impls.
- [ ] `EnvelopeId` wraps `[u8; 32]`, derives `Hash` + `Eq` + `Clone` + `Copy`. Manual Debug impl displays `[u8; 32]` as hex with `[REDACTED envelope_id]` marker.
- [ ] `BearerCapsule` is a typed struct with explicit fields (NOT a virtual key per RFC-0903; NOT a string literal). Field set per RFC-0959-A1 §Data Structures.

### Error type

- [ ] `DeliveryError` enum (6 variants): `ChainTipMismatch { expected: ChainTip, actual: ChainTip }`, `BearerInsertFailed { ask_id: AskId, reason: String }`, `CapabilityInsertFailed { ask_id: AskId, reason: String }`, `GossipFailed { attempts: u32 }` (reserved for 0959-c; variant exists now), `ReplayDetected { envelope_id: EnvelopeId }`, `ChainHashBroken { expected: [u8;32], actual: [u8;32] }`. Manual redacting Debug.

### Atomic transaction wrapper

- [ ] `crates/octo-wallet/src/capability/stoolap_txn.rs` (NEW) — `stoolap_txn` atomic transaction wrapper. Wraps a `stoolap::Transaction` with `chain_tip_lock` CAS primitive.
- [ ] `chain_tip_lock(expected_prev_hash: [u8;32]) -> Result<ChainTipGuard, DeliveryError>` — compare-and-swap on `(chain_tip_hash, expected_prev_hash)`. Guard auto-releases on drop.
- [ ] All-or-nothing: if any insert fails, all prior inserts in the txn are rolled back.

### `deliver_at_settlement` algorithm

- [ ] `crates/octo-wallet/src/capability/deliver.rs` (NEW) — `deliver_at_settlement(ask: Ask, settlement: SettlementEvent, seller_signing_key: &Ed25519Keypair, buyer_pub: &IdentityKey, ask_ttl_unix: u64) -> Result<MarketDeliveryEnvelope, DeliveryError>`.
- [ ] Steps: acquire `chain_tip_lock`; build `DealSettled` + `MarketDeliveryEnvelopePreimage`; seller signs preimage; mint bearer via RFC-0957 mint + mint capability; `txn.insert_dual(bearer_record, capability_record)` atomic; build `MarketDeliveryEnvelope`; return.
- [ ] Phantom call site: `IdentityKey::from_public_bytes(&buyer_pub_bytes)` — uses working stub from top-level mission 0957-a1 (`crates/octo-wallet/src/capability/identity_stub.rs`).

### Settlement chain extension

- [ ] `crates/quota-router-core/src/settlement/chain.rs` (MODIFY) — `append_deal_settled(deal_settled: DealSettled, signer: &Ed25519Keypair) -> Result<ChainTip, SettlementError>` + chain hash update logic.
- [ ] `settled_at_unix` derived from prior `SettlementEvent::settled_at_unix` field. NOT a separate timestamp.

### Test vectors (RFC-0959-A1 §Test Vectors, this sub-mission owns TV1, TV2, TV3, TV5, TV6, TV8, TV9, TV10, TV11, TV12)

- [ ] TV1: Minimal Delivery — full happy path; verify envelope structure + chain hash.
- [ ] TV2: Atomicity Rollback — Bearer Insert Fails — forced failure on bearer insert; capability MUST NOT persist.
- [ ] TV3: Atomicity Rollback — Capability Insert Fails — forced failure on capability insert; bearer MUST NOT persist.
- [ ] TV5: Backward Compat — Legacy Verifier — pre-RFC-0959-A1 verifier (no envelope support) processes `SettlementEvent` + `SettlementReceipt` without crashing; envelope is opaque.
- [ ] TV6: Replay Defense — duplicate `EnvelopeId` insert returns `DeliveryError::ReplayDetected`.
- [ ] TV8: Chain Hash Continuity — `chain_tip_hash` after 100 sequential `append_deal_settled` matches the expected hash.
- [ ] TV9: Debug Redaction — `format!("{:?}", envelope)` contains `[REDACTED]` markers; grep test for credential material.
- [ ] TV10: Chain-Tip TOCTOU Race — two concurrent `deliver_at_settlement` calls on the same ask; one wins via `chain_tip_lock` CAS, the other returns `DeliveryError::ChainTipMismatch`.
- [ ] TV11: Idempotency via UNIQUE — duplicate `DealSettled.ask_id` insert returns idempotency error (UNIQUE on `ask_id` column).
- [ ] TV12: Buyer Identity Binding — envelope's `buyer_did` matches the buyer's `IdentityKey::did()`; tampering with `buyer_did` invalidates seller signature.

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `cargo fmt --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0009 — seller signature substrate
- RFC-0126 — canonical_ser for `DealSettled` + envelope
- RFC-0853 — BLAKE3 for `EnvelopeId`
- RFC-0862 — `Transaction` + gossip substrate
- RFC-0903 — virtual keys (sibling; BearerCapsule is NOT here)
- RFC-0957 — CapabilityToken format
- RFC-0957-A1 — `Transaction::insert_dual` + `HolderRegistry` trait
- RFC-0959 — base settlement chain

**Requires (mission gates):**

- `missions/open/0959-a1-market-delivery.md` (top-level)
- `missions/claimed/0959-a-ask-pricing-stoolap.md` (in progress) — base `Ask` + `SettlementEvent` + `SettlementReceipt`
- `missions/open/0957-c-holder-registry-impl.md` — `HolderRegistry` + `Transaction::insert_dual` substrate
- `missions/open/0957-e-mint-txn-parameter.md` — `CapabilityCatalog` extensions (specifically `holder_registry`, `root_secret_for_ask`, `settlement_chain_tip` consumed here; `gossip_to_buyer` consumed by 0959-c)

```yaml
depends_on:
  - mission-0959-a-ask-pricing-stoolap # base settlement chain
  - mission-0957-c-holder-registry-impl # Transaction + HolderRegistry
  - mission-0957-e-mint-txn-parameter # CapabilityCatalog extensions
```

## Type Coverage

This sub-mission implements (per top-level Type Coverage table):

- `BearerCapsule` struct
- `DealSettled` struct (4th settlement chain artifact)
- `DealSettledPayload` struct
- `MarketDeliveryEnvelope` struct (3-segment wire)
- `MarketDeliveryEnvelopePreimage` struct (signed bytes)
- `EnvelopeId` newtype
- `DeliveryError` enum (6 variants; `GossipFailed` reserved for 0959-c)
- `chain_tip_lock` CAS primitive
- `deliver_at_settlement` algorithm
- `stoolap_txn` atomic transaction wrapper
- `append_deal_settled` + chain hash update
- `settled_at_unix` field derivation
- Manual redacting Debug impls

`CapabilityCatalog::gossip_to_buyer` lives in sub-mission 0957-e (cross-mission dependency).

## Location

- `crates/octo-wallet/src/capability/market_delivery.rs` (NEW)
- `crates/octo-wallet/src/capability/deliver.rs` (NEW)
- `crates/octo-wallet/src/capability/stoolap_txn.rs` (NEW)
- `crates/quota-router-core/src/settlement/chain.rs` (MODIFY) — `append_deal_settled` + chain hash
- `crates/octo-wallet/src/capability/mod.rs` (MODIFY) — module exports

## Claimant

@unclaimed

## Pull Request

(unset)

## Notes

- TV4 (Gossip Retry) and TV7 (Cross-Node Delivery) live in sub-mission 0959-c. This sub-mission owns TV1, TV2, TV3, TV5, TV6, TV8, TV9, TV10, TV11, TV12 (10 of 12 vectors).
- The `GossipFailed { attempts }` variant is RESERVED in this sub-mission (variant exists; no code path emits it). Sub-mission 0959-c adds the retry loop that emits it.
- `MarketDeliveryEnvelope` wire format is 3-segment per RFC-0959-A1 §Wire Format: `envelope_id || bearer_capsule || capability_token` (base64url-no-pad). NOT 4-segment (unlike RFC-0970 hop envelope which has `wrap_for_hop` 4-segment).
- `BearerCapsule` is a typed struct (NOT RFC-0903 virtual keys). Field set per RFC-0959-A1 §Data Structures: `bearer_id`, `ask_id`, `holder_pub`, `caveats_canonical`, `signed_at_unix`, `signature`.
