# Mission: Market Delivery Envelope (RFC-0959-A1 Amendment)

## Status

Closed (Decomposition Spec Band A — 2026-08-06). Claimed 2026-08-04 by @mmacedoeu; RFC-0959-A1 acceptance roll-up captured; all 5 top-level ACs are explicit cross-mission deferrals with named owner per [[deferred-vs-unspecified]]: AC-1 (12 §Test Vectors) + AC-2 (5 §Adversary Analysis findings) + AC-5 (cross-crate compat) → 0959-b (10 TVs + A9/A10) + 0959-c (TV4 + TV7 + A11) sub-mission landings; AC-3 (phantom type `IdentityKey::from_public_bytes`) → RFC-0009-B1 / RFC-0957-A2 full signature promotion (working stub lives at `crates/octo-wallet/src/capability/identity_stub.rs` per top-level mission 0957-a1); AC-4 (sub-mission merges) → 0959-b + 0959-c merge chain. Mission role: spec roll-up + type coverage decomposition + dependency documentation — no new code authored in this mission per BLUEPRINT §Multi-Mission Decomposition.

## RFC

RFC-0959-A1 (Economics): Market Delivery Envelope (Amendment) — Accepted 2026-08-02

**BLUEPRINT gate note:** RFC reached Accepted 2026-08-02 (multi-round R28-R64 review convergence). Mission now CLAIMABLE per BLUEPRINT Mission Lifecycle.

This mission is the **top-level decomposition mission** for RFC-0959-A1. RFC-0959-A1 has 12 test vectors, 4 implementation phases, and 9+ new types (BearerCapsule, DealSettled, DealSettledPayload, MarketDeliveryEnvelope, MarketDeliveryEnvelopePreimage, EnvelopeId, chain_tip_lock, ask_ttl_unix parameter, settled_at_unix). Per BLUEPRINT §Multi-Mission Decomposition (>10 types, >4 phases), this top-level captures acceptance criteria + Type Coverage roll-up; the implementation work decomposes into 2 sub-missions (0959-b, 0959-c).

## Summary

Implement the dual-mode delivery flow at deal settlement time. Extend the RFC-0959 settlement chain (Accepted 2026-07-20) with a fourth chained artifact `DealSettled` that joins `Ask`, `SettlementEvent`, `SettlementReceipt`. The artifact carries BOTH the bearer token (BearerCapsule) AND the capability token (CapabilityToken via RFC-0957-A1 HolderRegistry) atomically. On settlement, the seller atomically inserts both into the local registry; the envelope is gossiped to the buyer (RFC-0862 gossip) via `CapabilityCatalog::gossip_to_buyer(buyer_did, env)`. The atomicity is a Stoolap transaction with `chain_tip_lock` CAS to break the TOCTOU race (Finding A10). `BearerCapsule` is a typed struct (RFC-0959-A1, NOT RFC-0903 virtual keys — see §Out of Scope + §Related RFCs).

## Acceptance Criteria

### Top-level: RFC-0959-A1 acceptance roll-up

The sub-missions (0959-b, 0959-c) implement the ACs by RFC-0959-A1 §Test Vectors. When both sub-missions are complete and merged, every AC below is satisfied.

- [ ] All 12 RFC-0959-A1 §Test Vectors pass (TV1: Minimal Delivery, TV2: Atomicity Rollback — Bearer Insert Fails, TV3: Atomicity Rollback — Capability Insert Fails, TV4: Gossip Retry, TV5: Backward Compat — Legacy Verifier, TV6: Replay Defense, TV7: Cross-Node Delivery, TV8: Chain Hash Continuity, TV9: Debug Redaction, TV10: Chain-Tip TOCTOU Race, TV11: Idempotency via UNIQUE, TV12: Buyer Identity Binding) → **DEFERRED to sub-mission landings per [[deferred-vs-unspecified]]**: 10 vectors (TV1, TV2, TV3, TV5, TV6, TV8, TV9, TV10, TV11, TV12) → `missions/claimed/0959-b-market-delivery-impl.md` (BearerCapsule + DealSettled + envelope types + `deliver_at_settlement` + `chain_tip_lock` + `stoolap_txn`); 2 vectors (TV4, TV7) → `missions/claimed/0959-c-delivery-gossip-integration.md` (bounded retry loop + cross-node delivery verification).
- [ ] All 5 RFC-0959-A1 §Adversary Analysis findings covered (A9: Delivery-vs-settlement race, A10: Atomicity rollback exploitation, A11: Gossip partition → envelope not received, plus 2 from RFC-0959 base spec preserved) → **DEFERRED to sub-mission landings per [[deferred-vs-unspecified]]**: A9 + A10 → `missions/claimed/0959-b-market-delivery-impl.md` (`chain_tip_lock` CAS breaks TOCTOU race + atomicity rollback exploitation); A11 → `missions/claimed/0959-c-delivery-gossip-integration.md` (bounded retry loop defeats gossip partition); 2 RFC-0959 base spec findings preserved unchanged.
- [ ] Phantom type `IdentityKey::from_public_bytes` properly DEFERRED to RFC-0009-B1 / RFC-0957-A2 (working stub per top-level mission 0957-a1) → **DEFERRED to RFC-0009-B1 / RFC-0957-A2 full signature promotion per [[deferred-vs-unspecified]]**. Working stub currently lives at `crates/octo-wallet/src/capability/identity_stub.rs` per top-level mission `missions/claimed/0957-a1-holder-registry.md`; this mission inherits 0957-a1's deferral (0957-a1 AC-3 also defers with same named owner chain). Stub call site at `deliver_at_settlement` per RFC-0959-A1 §Algorithms:phantom_call_site.
- [ ] Sub-missions 0959-b, 0959-c all merged and ACs flipped → **DEFERRED to sub-mission landings per [[deferred-vs-unspecified]]**: 0959-b currently Claimed 2026-08-04 (0/26 ACs); 0959-c currently Claimed 2026-08-04 (0/12 ACs). Both depend on `missions/claimed/0957-c-holder-registry-impl.md` (CLOSED Band A 2026-08-06 per commit `7609aaad`) + `missions/claimed/0957-e-mint-txn-parameter.md` (Claimed 2026-08-04, 0/15 ACs).
- [ ] Cross-crate compat: `cargo build --workspace` green; `cargo test --workspace` green; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --check` clean → **DEFERRED to sub-mission landings per [[deferred-vs-unspecified]]**: surfaces when 0959-b + 0959-c implementations land; not in scope for this spec-only mission.

### Type Coverage

| RFC-0959-A1 Type                                                                                                                                            | Implemented By     |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ |
| `BearerCapsule` struct (typed, NOT RFC-0903 virtual key)                                                                                                    | Sub-mission 0959-b |
| `DealSettled` struct (4th settlement chain artifact)                                                                                                        | Sub-mission 0959-b |
| `DealSettledPayload` struct (chain payload)                                                                                                                 | Sub-mission 0959-b |
| `MarketDeliveryEnvelope` struct (3-segment wire: `envelope_id \|\| bearer_capsule \|\| capability_token`)                                                   | Sub-mission 0959-b |
| `MarketDeliveryEnvelopePreimage` struct (signed bytes)                                                                                                      | Sub-mission 0959-b |
| `EnvelopeId` newtype (32-byte BLAKE3, Hash impl)                                                                                                            | Sub-mission 0959-b |
| `DeliveryError` enum (6 variants: ChainTipMismatch, BearerInsertFailed, CapabilityInsertFailed, GossipFailed { attempts }, ReplayDetected, ChainHashBroken) | Sub-mission 0959-b |
| `chain_tip_lock` CAS primitive                                                                                                                              | Sub-mission 0959-b |
| `deliver_at_settlement` algorithm                                                                                                                           | Sub-mission 0959-b |
| `stoolap_txn` atomic transaction wrapper                                                                                                                    | Sub-mission 0959-b |
| `CapabilityCatalog::gossip_to_buyer` extension (cross-mission: lives in 0957-e)                                                                             | Sub-mission 0957-e |
| `append_deal_settled` + chain hash update                                                                                                                   | Sub-mission 0959-b |
| `settled_at_unix` field derived from prior `SettlementEvent`                                                                                                | Sub-mission 0959-b |
| Manual redacting `Debug` impls on all 6 security-bearing structs                                                                                            | Sub-mission 0959-b |

### Mission Dependency Model

```yaml
depends_on:
  - 0959-a-ask-pricing-stoolap # base RFC-0959 settlement chain (closed Band A 2026-08-04 per commit 598273b0)
  - 0957-c-holder-registry-impl # HolderRegistry + Transaction substrate (closed Band A 2026-08-06 per commit 7609aaad)
  - 0957-e-mint-txn-parameter # CapabilityCatalog extensions (gossip_to_buyer) (claimed 2026-08-04; in progress)
decomposes_into:
  - 0959-b-market-delivery-impl # BearerCapsule + DealSettled + MarketDeliveryEnvelope + EnvelopeId + deliver_at_settlement + stoolap_txn + append_deal_settled + DeliveryError (claimed 2026-08-04; in progress)
  - 0959-c-delivery-gossip-integration # gossip_to_buyer + retry policy + cross-node delivery verification (claimed 2026-08-04; in progress)
```

## Dependencies

**Requires (RFC gates):**

- RFC-0009 — seller signature substrate (`holder_sign` per §Capability Keys) + buyer encryption pubkey
- RFC-0126 — canonical_ser for `DealSettled` + envelope
- RFC-0853 — BLAKE3 primitive source (for `EnvelopeId`)
- RFC-0862 — atomic transaction + gossip
- RFC-0903 — virtual keys (sibling, NOT BearerCapsule — BearerCapsule defined here per §Out of Scope)
- RFC-0957 — CapabilityToken format
- RFC-0957-A1 — `TransactionExt::insert_dual` + `CapabilityCatalog::gossip_to_buyer` (R10-N5 fix: this RFC consumes those substrate methods)
- RFC-0959 — base settlement chain (this amendment extends it)

**Mission gates:**

- `missions/claimed/0959-a-ask-pricing-stoolap.md` — Band A closed 2026-08-04 (audit-grade AC flip landed per commit `598273b0`); base `Ask` + `SettlementEvent` + `SettlementReceipt` exist
- `missions/claimed/0957-c-holder-registry-impl.md` — Band A closed 2026-08-06 (commit `7609aaad`); `HolderRegistry` + `Transaction` substrate exist
- `missions/claimed/0957-e-mint-txn-parameter.md` — Claimed 2026-08-04 (0/15 ACs); `CapabilityCatalog::gossip_to_buyer` MUST land before envelope gossip

**Not Requires:**

- RFC-0958 (ZK subclass) — Accepted; ZK capability circuit implementation in flight via `missions/claimed/0958-a-zk-capability-circuit.md` (S05 4-session plan); `HolderKind::ZKBearing` row accommodated by RFC-0957-A1 §Data Structures; dual-pipeline authority extension to ZK is post-0958-a merge scope
- RFC-0955 (marketplace ordering) — orthogonal

## Implementation Guide

- RFC-0959-A1 §Specification → §System Architecture → §Data Structures → §Algorithms → §Settlement Chain Extension → §Test Vectors (single canonical reference)
- RFC-0959-A1 §Appendices: §Sample Walk-Through, §RFC-0959 §Roles Update, §Forward-Compat Behavior for Legacy Verifiers
- Developer guide: inline §Developer Guide section in sub-mission 0959-b (inline in this mission)

## Decomposition Rationale

RFC-0959-A1 qualifies for decomposition per BLUEPRINT §Multi-Mission Decomposition:

- **9 new types** (BearerCapsule, DealSettled, DealSettledPayload, MarketDeliveryEnvelope, MarketDeliveryEnvelopePreimage, EnvelopeId, DeliveryError, chain_tip_lock, settled_at_unix) — borderline >10 with DeliveryError variants
- **4 implementation phases** (§Phase 1: Data Structures + Algorithms, §Phase 2: Stoolap Transaction Wrapper, §Phase 3: Gossip Integration, §Phase 4: Mission Decomposition) — at threshold
- **Different prerequisite chains:**
  - 0959-b (delivery impl) depends on RFC-0959 base + 0957-c Transaction
  - 0959-c (gossip integration) depends on 0959-b envelope + 0957-e CapabilityCatalog::gossip_to_buyer

Splitting by module boundary (transaction wrapper / gossip) lets 0959-b merge independently of the gossip retry policy.

## Claimant

@mmacedoeu (Top-level decomposition; ACs roll up as 0959-b, 0959-c land)

## Pull Request

(unset; sub-mission PRs land via 0959-b / 0959-c per decomposition model)

## Closure

**Closure Date:** 2026-08-06 (Decomposition Spec Band A)

**Closure Status:** Spec roll-up captured; ACs explicitly deferred to sub-mission landings with named owner per [[deferred-vs-unspecified]].

**Decomposition model:** RFC-0959-A1 decomposition per BLUEPRINT §Multi-Mission Decomposition captured 14 type coverage rows (BearerCapsule, DealSettled, DealSettledPayload, MarketDeliveryEnvelope, MarketDeliveryEnvelopePreimage, EnvelopeId, DeliveryError, chain_tip_lock, deliver_at_settlement, stoolap_txn, CapabilityCatalog::gossip_to_buyer extension, append_deal_settled, settled_at_unix derivation, manual redacting Debug impls) split across 2 sub-missions: 0959-b (13 types / 10 vectors / A9 + A10 adversary findings) + 0959-c (gossip_to_buyer extension + retry loop / TV4 + TV7 / A11 finding).

**Phantom type carry-forward:** AC-3 (phantom `IdentityKey::from_public_bytes`) inherits the 0957-a1 deferral chain (top-level mission `missions/claimed/0957-a1-holder-registry.md` AC-3 defers to RFC-0009-B1 / RFC-0957-A2 full signature promotion). Working stub at `crates/octo-wallet/src/capability/identity_stub.rs` referenced from RFC-0959-A1 §Algorithms:phantom_call_site + RFC-0957-A1 §Phantom Types + RFC-0969 §Algorithms:phantom_call_site. Full promotion tracked independently in 0957-a1 closure chain.

**Sub-mission dependency chain (snapshot 2026-08-06):**

| Sub-mission                          | Status             | ACs  | Owner      | Blocker                                                                        |
| ------------------------------------ | ------------------ | ---- | ---------- | ------------------------------------------------------------------------------ |
| `0959-b-market-delivery-impl`        | Claimed 2026-08-04 | 0/26 | @mmacedoeu | `missions/claimed/0957-e-mint-txn-parameter.md` (CapabilityCatalog extensions) |
| `0959-c-delivery-gossip-integration` | Claimed 2026-08-04 | 0/12 | @mmacedoeu | 0959-b (envelope + DeliveryError) + 0957-e (gossip_to_buyer)                   |

**Mission role confirmation:** This top-level mission does NOT author code per BLUEPRINT §Multi-Mission Decomposition (>10 types, >4 phases). All code lands in sub-missions 0959-b + 0959-c. Re-opening this mission is unnecessary; sub-mission landings automatically flip this mission's ACs via the roll-up model.

**Version history:**

| Version | Date       | Change                                                                                                                                                                                    |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-04 | Mission claimed. RFC-0959-A1 §Spec roll-up captured; 14-type decomposition to 0959-b + 0959-c documented.                                                                                 |
| v0.2    | 2026-08-06 | Closed Decomposition Spec Band A. All 5 ACs explicit deferrals with named owner per [[deferred-vs-unspecified]]. Mission gates + yaml `depends_on:` paths corrected (claimed/ not open/). |

Last Updated: 2026-08-06
Version: 0.2

## Notes

- `BearerCapsule` is defined HERE per RFC-0959-A1 §Out of Scope, NOT in RFC-0903 (virtual keys are a different primitive). Round 1 review found the dual-definition risk; R8-N4 fix sealed it to RFC-0959-A1.
- The `chain_tip_lock` CAS primitive is the load-bearing atomicity mechanism (Finding A10). Implementation must compare-and-swap on `(chain_tip_hash, expected_prev_hash)`; any mismatch returns `DeliveryError::ChainTipMismatch`.
- Phantom type `IdentityKey::from_public_bytes` call site is at `deliver_at_settlement` (the buyer pubkey extraction point). Stub lives in `crates/octo-wallet/src/capability/identity_stub.rs` per top-level mission 0957-a1.
- `EnvelopeId` is a 32-byte BLAKE3 over `MarketDeliveryEnvelopePreimage`. Newtype wraps `[u8; 32]` with `Hash` impl (so it can be a HashMap key).
- Debug redaction: `MarketDeliveryEnvelopePreimage`, `EnvelopeId`, `DealSettled`, `DealSettledPayload`, `BearerCapsule`, `MarketDeliveryEnvelope` all have manual redacting `Debug` impls.
- Future Work F5 (bounded retry loop wrapping `catalog.gossip_to_buyer`; exhaustion via `DeliveryError::GossipFailed { attempts }`): the variant is RESERVED in RFC-0959-A1 §Error Handling, but the retry loop is sub-mission 0959-c scope.

### Related

- [Dual-Mode Authorization Batch Accepted 2026-08-02](../rfcs/accepted/economics/0959-a1-market-delivery.md)
- Original research: `docs/research/2026-08-01-dual-mode-workflow-gap-research.md`
- Original use case: `docs/use-cases/dual-mode-authorization-workflow.md`
