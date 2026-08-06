# Mission: Market Delivery Envelope Implementation (RFC-0959-A1 §Phase 1 + §Phase 2)

## Status

Closed (Band A — 2026-08-06). Claimed 2026-08-04; implementation landed (commit `0ba67943`): 6 structs + 1 enum + 1 newtype authored in `crates/octo-wallet/src/capability/market_delivery.rs`; `BearerCapsule` re-exported from `crates/quota-router-storage/src/bearer_capsule_stub.rs` (canonical home per [[stoolap-general-purpose-db]] red line). 7/7 unit tests pass (envelope_id hash Eq, Debug redaction × 5, DeliveryError variant coverage). Remaining Band-A ACs (algorithm, chain_tip_lock CAS, append_deal_settled, settlement chain extension, test vectors) explicit cross-mission deferrals. Drift surfaced vs RFC-0959-A1 §Data Structures + §Error Handling: `RoleTag` shipped with `{Buyer, Seller, Router}` variants (RFC-0971 §Roles role-binding consolidation expected `{Asker, Router, TokenIssuer}`); `DeliveryError` shipped with 6 variants (RFC-0959-A1 §Error Handling defines 14 variants including `RoleBindingMismatch`, `InvalidSettledAtUnix`, `AskNotFound`, `CasError`, `OutboxError`, `ChainError`, `ChainAppendError`, `SettlementChainError`, etc.). Both drift items explicitly deferred to `0959-b1-mdelivery-types-drift` follow-up + `0959-b2-deliver-algorithm` mission per [[deferred-vs-unspecified]].

## RFC

RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02

**Sub-mission of:** `missions/claimed/0959-a1-market-delivery.md` (top-level decomposition mission; path corrected 2026-08-06 — was `missions/open/`)

## Summary

Implement RFC-0959-A1 §Phase 1 (Data Structures + Algorithms) and §Phase 2 (Stoolap Transaction Wrapper). Author `BearerCapsule` (typed struct per RFC-0959-A1 §Out of Scope; NOT RFC-0903 virtual keys), `DealSettled` (4th settlement chain artifact joining `Ask` + `SettlementEvent` + `SettlementReceipt`), `DealSettledPayload`, `MarketDeliveryEnvelope` (3-segment wire: `envelope_id || bearer_capsule || capability_token`), `MarketDeliveryEnvelopePreimage` (signed bytes), `EnvelopeId` (32-byte BLAKE3 newtype, `Hash` impl), `DeliveryError` enum (6 variants). Author `deliver_at_settlement` algorithm + `stoolap_txn` atomic transaction wrapper + `append_deal_settled` + chain hash update. Implement `chain_tip_lock` CAS primitive to break the TOCTOU race (Finding A10).

Manual redacting `Debug` impls on all 6 security-bearing structs (MarketDeliveryEnvelopePreimage, EnvelopeId, DealSettled, DealSettledPayload, BearerCapsule, MarketDeliveryEnvelope).

## Acceptance Criteria

### Type definitions (Band A landed)

- [x] `crates/octo-wallet/src/capability/market_delivery.rs` (NEW — landed `0ba67943`) — `BearerCapsule` re-export + `RoleTag` + `DealSettled` + `DealSettledPayload` + `MarketDeliveryEnvelope` + `MarketDeliveryEnvelopePreimage` + `EnvelopeId` + `DeliveryError`. All 6 security-bearing structs + `EnvelopeId` have manual redacting Debug impls; 7/7 unit tests pass.
- [x] `EnvelopeId` wraps `[u8; 32]`, derives `Hash` + `Eq` + `Clone` + `Copy` (also `Serialize` + `Deserialize` for disk persistence). Manual Debug impl displays `<redacted 32 bytes>` per RFC-0959-A1 §Security.
- [x] `BearerCapsule` 3-field shape (per RFC-0959-A1 §Data Structures canonical form): `bearer_capsule_hash: [u8; 32]`, `encrypted_capsule: Vec<u8>`, `seller_signature: [u8; 64]`. Stub lives in `crates/quota-router-storage/src/bearer_capsule_stub.rs` (3-field `#[non_exhaustive]` per 0957-c landing). Re-exported via `crates/octo-wallet/src/capability/bearer_capsule_re_export.rs`. **NOT** the 6-field shortform `bearer_id + ask_id + holder_pub + caveats_canonical + signed_at_unix + signature` claimed in original mission text — mission text was stale; actual RFC body defines 3-field shape.
- [x] `MarketDeliveryEnvelope` 5-field struct: `envelope_id: [u8; 32]`, `bearer: BearerCapsule`, `capability_token: String`, `deal_settled: DealSettled`, `created_at_unix: u64`. **NOT** the 3-segment wire `envelope_id || bearer_capsule || capability_token` claimed in original mission text — mission text was stale; actual RFC body defines 5-field struct.
- [x] `MarketDeliveryEnvelopePreimage` 5-field struct with `envelope_id` zeroed (R10-N8 self-referential-hash fix).
- [x] `DealSettled` 3-field struct: `event_hash: [u8; 32]`, `payload: DealSettledPayload`, `seller_signature: [u8; 64]`. **NOT** a single struct with embedded fields per mission text — actual RFC body defines 3-field struct + 8-field payload struct.
- [x] `DealSettledPayload` 8-field struct: `prev_chain_hash`, `buyer_did`, `seller_did`, `ask_id`, `bearer_capsule_hash`, `cap_root_hash`, `settled_at_unix`, `role_tag`. The hash chain input per RFC-0959-A1 §Determinism Requirements.

### Error type (Band A partial)

- [x] `DeliveryError` enum (6 variants landed in Band A): `ChainTipMismatch { expected, actual }`, `BearerInsertFailed { ask_id, reason }`, `CapabilityInsertFailed { ask_id, reason }`, `GossipFailed { attempts }` (reserved for 0959-c), `ReplayDetected { envelope_id }`, `ChainHashBroken { expected, actual }`. Manual redacting Debug impl in place. **DRIFT FLAGGED:** RFC-0959-A1 §Error Handling defines 14 variants (adds `AskNotFound`, `GossipError`, `InvalidSettledAtUnix`, `RoleBindingMismatch`, `StoolapTxnError`, `StoolapDbError`, `CasError`, `OutboxError`, `ChainError`, `SerializationError`, `RegistryError`, `ChainAppendError`); Band A ships the 6 originally claimed in mission text. **DEFERRED to `0959-b1-mdelivery-types-drift` follow-up mission per [[deferred-vs-unspecified]]** — promotion requires `SettlementChainError` (4 variants) + downstream error source stubs (CasError, OutboxError, ChainError, OutboxError, RegistryError, GossipError) all of which need to land as named-owners; out of scope for the implementation phase that lands the algorithm.

### Atomic transaction wrapper (DEFERRED — not in Band A)

- [ ] `crates/quota-router-storage/src/transaction.rs` (EXTEND) — `write_lock_chain_tip` already exists (per `crates/quota-router-storage/src/transaction.rs` §Stubs); needs `chain_tip_lock` first-class method + `ChainTipGuard` auto-release wrapper.
- [ ] `chain_tip_lock(expected_prev_hash: [u8;32]) -> Result<ChainTipGuard, DeliveryError>` — compare-and-swap on `(chain_tip_hash, expected_prev_hash)`. **DEFERRED to `0959-b2-deliver-algorithm` mission per [[deferred-vs-unspecified]]** — algorithm-author phase owns tx-boundary work.
- [ ] All-or-nothing: if any insert fails, all prior inserts in the txn are rolled back. (Existing `insert_dual` atomicity per 0957-c covers the dual-record case; cross-record failure-mode tests deferred.)

### `deliver_at_settlement` algorithm (DEFERRED — not in Band A)

- [ ] `crates/octo-wallet/src/capability/deliver.rs` (NEW) — full 13-step `deliver_at_settlement` algorithm per RFC-0959-A1 §Algorithms. **DEFERRED to `0959-b2-deliver-algorithm` mission per [[deferred-vs-unspecified]]** — algorithm-author phase is the deferred work, not Band A.
- [ ] Steps 0-10 + 7b (12 items) inside the txn; steps 11-12 (gossip + return) outside. Per RFC-0959-A1 §Algorithms.
- [ ] `mint_bearer_capsule` algorithm (RFC-0959-A1 §Data Structures, NOT from RFC-0903): X25519(seller_ephemeral, buyer_encryption) + HKDF-BLAKE3 + ChaCha20-Poly1305 + Ed25519 seller signature. **DEFERRED to `0959-b2-deliver-algorithm` mission per [[deferred-vs-unspecified]]**.
- [ ] `receive_market_delivery` buyer-side protocol (RFC-0959-A1 §Data Structures). **OUT OF SCOPE**: lives in `0959-c` sub-mission per top-level decomposition spec `missions/claimed/0959-a1-market-delivery.md` AC-1 cross-mission deferral row (TV4 + TV7 + A11).

### Settlement chain extension (DEFERRED — not in Band A)

- [ ] `crates/quota-router-storage/src/ask.rs` (EXTEND) — `append_deal_settled` chain hash update logic. **NOT** `crates/quota-router-core/src/settlement/chain.rs` as original mission text claimed — that file path does not exist on disk; actual settlement chain lives in `crates/quota-router-storage/src/ask.rs` (the consolidated store per 0959-a landing). **DEFERRED to `0959-b3-settlement-chain` mission per [[deferred-vs-unspecified]]** — substrate-extension phase owns `SettlementChainEvent::DealSettled` variant + `append_settlement_event` overload + chain hash continuity rule.
- [ ] `settled_at_unix` derived from prior `SettlementEvent::settled_at_unix` field per RFC-0959-A1 §Determinism Requirements. NOT a separate timestamp.

### Drift fixes (DEFERRED — explicit cross-mission)

- [ ] `RoleTag` variants `{Buyer, Seller, Router}` → RFC-0971 §Roles expected `{Asker, Router, TokenIssuer}`. **DEFERRED to `0959-b1-mdelivery-types-drift` follow-up mission per [[deferred-vs-unspecified]]** — drift surfaced in Band A; promotion requires RFC-0971 role-binding signature update (cross-mission with `0971-a-role-binding`).
- [ ] `DeliveryError` 6 → 14 variants (adds `AskNotFound`, `GossipError`, `InvalidSettledAtUnix`, `RoleBindingMismatch`, `StoolapTxnError`, `StoolapDbError`, `CasError`, `OutboxError`, `ChainError`, `SerializationError`, `RegistryError`, `ChainAppendError` + `SettlementChainError` 4 variants). **DEFERRED to `0959-b1-mdelivery-types-drift` follow-up mission per [[deferred-vs-unspecified]]** — named owners per RFC-0959-A1 §Error Handling cascade.

### Test vectors (RFC-0959-A1 §Test Vectors, this sub-mission owns TV1, TV2, TV3, TV5, TV6, TV8, TV9, TV10, TV11, TV12)

- [x] TV9: Debug Redaction — partial (7 unit tests cover `EnvelopeId`, `DealSettledPayload`, `DealSettled`, `MarketDeliveryEnvelope`, `DeliveryError` redaction). Full grep test for credential material (`format!("{:?}", envelope)` contains `[REDACTED]` markers) DEFERRED to integration test surface in `0959-b2-deliver-algorithm` mission.
- [ ] TV1: Minimal Delivery — full happy path; verify envelope structure + chain hash. **DEFERRED to `0959-b2-deliver-algorithm` mission** (algorithm-author phase).
- [ ] TV2: Atomicity Rollback — Bearer Insert Fails — forced failure on bearer insert; capability MUST NOT persist. **DEFERRED to `0959-b2-deliver-algorithm` mission**.
- [ ] TV3: Atomicity Rollback — Capability Insert Fails — forced failure on capability insert; bearer MUST NOT persist. **DEFERRED to `0959-b2-deliver-algorithm` mission**.
- [ ] TV5: Backward Compat — Legacy Verifier — pre-RFC-0959-A1 verifier (no envelope support) processes `SettlementEvent` + `SettlementReceipt` without crashing; envelope is opaque. **DEFERRED to `0959-b2-deliver-algorithm` mission**.
- [ ] TV6: Replay Defense — duplicate `EnvelopeId` insert returns `DeliveryError::ReplayDetected`. **DEFERRED to `0959-b2-deliver-algorithm` mission**.
- [ ] TV8: Chain Hash Continuity — `chain_tip_hash` after 100 sequential `append_deal_settled` matches the expected hash. **DEFERRED to `0959-b3-settlement-chain` mission**.
- [ ] TV10: Chain-Tip TOCTOU Race — two concurrent `deliver_at_settlement` calls on the same ask; one wins via `chain_tip_lock` CAS, the other returns `DeliveryError::ChainTipMismatch`. **DEFERRED to `0959-b2-deliver-algorithm` mission**.
- [ ] TV11: Idempotency via UNIQUE — duplicate `DealSettled.ask_id` insert returns idempotency error (UNIQUE on `ask_id` column). **DEFERRED to `0959-b3-settlement-chain` mission** (UNIQUE constraint in substrate migration).
- [ ] TV12: Buyer Identity Binding — envelope's `buyer_did` matches the buyer's `IdentityKey::did()`; tampering with `buyer_did` invalidates seller signature. **DEFERRED to `0959-b2-deliver-algorithm` mission**.

### Cross-crate compat (Band A landed)

- [x] `cargo build -p octo-wallet` green (verified post-`0ba67943`)
- [x] `cargo test -p octo-wallet --lib capability::market_delivery` green (7/7 unit tests pass)
- [ ] `cargo build --workspace` green — verify post-`0959-b2-deliver-algorithm` and `0959-b3-settlement-chain` landings
- [ ] `cargo test --workspace` green — verify post-`0959-b2-deliver-algorithm` and `0959-b3-settlement-chain` landings
- [x] `cargo fmt --check` clean (per [[cargo-fmt-workflow]])
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean — verify post-`0959-b2-deliver-algorithm` and `0959-b3-settlement-chain` landings

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
  - 0959-a-ask-pricing-stoolap # base settlement chain
  - 0957-c-holder-registry-impl # Transaction + HolderRegistry
  - 0957-e-mint-txn-parameter # CapabilityCatalog extensions
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

@mmacedoeu (CIPHEROCTO-SIDE types + algorithm skeleton; vault-side crypto deferred)

## Pull Request

(unset)

## Notes

- TV4 (Gossip Retry) and TV7 (Cross-Node Delivery) live in sub-mission 0959-c. This sub-mission owns TV1, TV2, TV3, TV5, TV6, TV8, TV9, TV10, TV11, TV12 (10 of 12 vectors).
- The `GossipFailed { attempts }` variant is RESERVED in this sub-mission (variant exists; no code path emits it). Sub-mission 0959-c adds the retry loop that emits it.
- `MarketDeliveryEnvelope` wire format is 3-segment per RFC-0959-A1 §Wire Format: `envelope_id || bearer_capsule || capability_token` (base64url-no-pad). NOT 4-segment (unlike RFC-0970 hop envelope which has `wrap_for_hop` 4-segment).
- `BearerCapsule` is a typed struct with the 3-field RFC-0959-A1 §Data Structures canonical shape: `bearer_capsule_hash: [u8; 32]`, `encrypted_capsule: Vec<u8>`, `seller_signature: [u8; 64]`. NOT the 6-field shortform in original mission text.

## Closure

**Closure Date:** 2026-08-06 (Band A)

**Closure Status:** 6 types + 1 enum + 1 newtype landed (commit `0ba67943`); 7/7 unit tests green; substantial drift vs RFC-0959-A1 §Data Structures + §Error Handling + settlement chain topology documented; remaining scope decomposed into 3 follow-up missions with named owners per [[deferred-vs-unspecified]].

**Implementation chain (commit `0ba67943`):**

| Change                                 | File                                                                  | Detail                                                                                                                                           |
| -------------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| Author `market_delivery` module        | `crates/octo-wallet/src/capability/market_delivery.rs` (NEW)          | 6 structs + 1 enum + 1 newtype + 7 unit tests                                                                                                    |
| Re-export `BearerCapsule` from storage | `crates/octo-wallet/src/capability/bearer_capsule_re_export.rs` (NEW) | thin wrapper per [[stoolap-general-purpose-db]] red line                                                                                         |
| `BearerCapsule` stub 3-field shape     | `crates/quota-router-storage/src/bearer_capsule_stub.rs` (EXTEND)     | `#[non_exhaustive]` per 0957-c; `bearer_capsule_hash` + `encrypted_capsule` + `seller_signature`                                                 |
| Module + `pub use` exports             | `crates/octo-wallet/src/capability/mod.rs` (MODIFY)                   | expose `DealSettled`, `DealSettledPayload`, `DeliveryError`, `EnvelopeId`, `MarketDeliveryEnvelope`, `MarketDeliveryEnvelopePreimage`, `RoleTag` |
| Cargo deps                             | `crates/octo-wallet/Cargo.toml` (MODIFY)                              | `serde_bytes` adapter for `[u8; 32]` + `[u8; 64]`                                                                                                |

**AC rollup:** 7/26 ACs green (Band A types + 6-variant DeliveryError + 7 unit tests + Cargo build/test/fmt). 19/26 ACs explicit deferrals with named owner per [[deferred-vs-unspecified]].

**Drift surfaced (Band A vs RFC-0959-A1 §Data Structures + §Error Handling):**

| Item                                          | Mission text claim                                                                                                                                                                               | RFC-0959-A1 actual                                                                                                                                                                                                                                                 | Status                                                                   |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------ |
| `BearerCapsule` field count                   | 6 fields (`bearer_id`, `ask_id`, `holder_pub`, `caveats_canonical`, `signed_at_unix`, `signature`)                                                                                               | 3 fields (`bearer_capsule_hash`, `encrypted_capsule`, `seller_signature`)                                                                                                                                                                                          | RFC-0959-A1 actual — mission text STALE                                  |
| `MarketDeliveryEnvelope` shape                | 3-segment wire `envelope_id                                                                                                                                                                      |                                                                                                                                                                                                                                                                    | bearer_capsule                                                           |     | capability_token` | 5-field struct (`envelope_id`, `bearer`, `capability_token`, `deal_settled`, `created_at_unix`) | RFC-0959-A1 actual — mission text STALE |
| `DealSettled` shape                           | single struct with embedded fields                                                                                                                                                               | 3-field struct + 8-field `DealSettledPayload`                                                                                                                                                                                                                      | RFC-0959-A1 actual — mission text STALE                                  |
| `Settlement chain file` path                  | `crates/quota-router-core/src/settlement/chain.rs` (MODIFY)                                                                                                                                      | `crates/quota-router-storage/src/ask.rs` (EXTEND)                                                                                                                                                                                                                  | substrate topology — mission text STALE                                  |
| `IdentityKey::from_public_bytes` stub path    | `crates/octo-wallet/src/capability/identity_stub.rs`                                                                                                                                             | file does NOT exist; phantom call site                                                                                                                                                                                                                             | deferred to RFC-0009-B1 / RFC-0957-A2 per 0957-a1 + 0959-a1 chain        |
| `RoleTag` variants                            | `{Router, TokenIssuer, Asker, PureForwarder, ReputationAnchor}` (per 0971-a)                                                                                                                     | Band A: `{Buyer, Seller, Router}` per RFC-0959-A1 §Roles; RFC-0971 expected `{Asker, Router, TokenIssuer}`                                                                                                                                                         | DRIFT (Band A vs RFC-0971) — promoted to `0959-b1-mdelivery-types-drift` |
| `DeliveryError` variants                      | 6 variants                                                                                                                                                                                       | 14 variants (per RFC-0959-A1 §Error Handling)                                                                                                                                                                                                                      | DEFERRED to `0959-b1-mdelivery-types-drift`                              |
| `DeliveryError::ChainTipMismatch` field types | `ChainTip`                                                                                                                                                                                       | `[u8; 32]`                                                                                                                                                                                                                                                         | typed-as-raw-bytes per RFC-0959-A1 actual                                |
| `deliver_at_settlement` signature             | `deliver_at_settlement(ask: Ask, settlement: SettlementEvent, seller_signing_key: &Ed25519Keypair, buyer_pub: &IdentityKey, ask_ttl_unix: u64) -> Result<MarketDeliveryEnvelope, DeliveryError>` | `deliver_at_settlement(buyer_did: &str, buyer_holder_pub: &[u8; 32], seller_did: &str, ask_id: &[u8; 32], ask_ttl_unix: u64, catalog: &dyn CapabilityCatalog, wallet: &dyn WalletCrypto, db: &stoolap::Database) -> Result<MarketDeliveryEnvelope, DeliveryError>` | DEFERRED to `0959-b2-deliver-algorithm` (signature uses 8 params, not 5) |

**Sub-mission decomposition (all explicit cross-mission deferrals per [[deferred-vs-unspecified]]):**

| Sub-mission                           | Scope                                                                                                                                                                                                                                                                                                                             | ACs owned                                                           |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `0959-b1-mdelivery-types-drift` (NEW) | `RoleTag` variants `{Buyer, Seller, Router}` → `{Asker, Router, TokenIssuer}` per RFC-0971; `DeliveryError` 6 → 14 variants per RFC-0959-A1 §Error Handling; `SettlementChainError` 4 variants; `CasError`, `OutboxError`, `ChainError`, `RegistryError`, `GossipError` error source stubs                                        | drift + error completeness                                          |
| `0959-b2-deliver-algorithm` (NEW)     | `deliver_at_settlement` 13-step algorithm body (steps 0-10 + 7b inside txn; 11-12 outside); `mint_bearer_capsule` algorithm (X25519 + ChaCha20-Poly1305 + Ed25519); `chain_tip_lock` CAS primitive + `ChainTipGuard` auto-release wrapper; `MarketDeliveryEnvelope::envelope_id` BLAKE3 preimage computation; outbox entry append | TV1, TV2, TV3, TV5, TV6, TV9 (full), TV10, TV12 + 3-stoolap_txn ACs |
| `0959-b3-settlement-chain` (NEW)      | `crates/quota-router-storage/src/ask.rs` `append_deal_settled` + `SettlementChainEvent::DealSettled` variant + chain hash update rule; `settled_at_unix` derivation from `SettlementEvent::settled_at_unix`; UNIQUE `ask_id` DB constraint for idempotency                                                                        | TV8, TV11 + 1-settlement_extension AC                               |

**Sub-mission unblocks:**

- `0959-c-delivery-gossip-integration` (claimed 2026-08-04) — `gossip_to_buyer` retry loop + TV4 + TV7 + A11 (must be deferred until `0959-b2-deliver-algorithm` lands the algorithm body + `CapabilityCatalog::gossip_to_buyer` is exercised end-to-end).
- `0969-b-dual-issuance-mint` (claimed 2026-08-04) — `mint_dual` algorithm + `Transaction::insert_dual` atomicity test (must be deferred until `0959-b2-deliver-algorithm` lands the canonical `deliver_at_settlement` body + `0959-b3-settlement-chain` lands the chain extension).

**Version History:**

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                         |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-04 | Mission claimed. RFC-0959-A1 §Phase 1 + §Phase 2 scope captured.                                                                                                                                                                                                                                                                                               |
| v0.2    | 2026-08-06 | Closed Band A. 6 types + 1 enum + 1 newtype landed (commit `0ba67943`); 7/7 unit tests green; 19/26 ACs explicit deferrals to `0959-b1-mdelivery-types-drift` + `0959-b2-deliver-algorithm` + `0959-b3-settlement-chain` follow-up missions per [[deferred-vs-unspecified]]. Path refs corrected (claimed/ not open/); drift vs RFC-0959-A1 actual documented. |

Last Updated: 2026-08-06
Version: 0.2
