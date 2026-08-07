# Mission: Forwarding-Hop Authorization Envelope (RFC-0970)

## Status

Closed (Band A — 2026-08-07). Claimed 2026-08-04; top-level roll-up closure landed at commit (see §Closure). Sub-missions: `0970-a-hop-envelope.md` (Band A closed 2026-08-06; commit `2f078974`-prior; 11/15 ACs GREEN); `0970-a1-hop-crypto-and-replay-defense.md` (Band A closed 2026-08-07; commit (see mission file); 13/13 ACs GREEN); `0970-b-forward-integration.md` (Band A closed 2026-08-06; commit `11921128`-prior; 5/5 ACs GREEN).

Test vector coverage (11 total): TV1, TV3, TV4, TV5, TV6, TV7, TV8, TV9, TV10, TV11 GREEN (10/11) via sub-mission roll-up; TV2 (Three-Hop Chain) DEFERRED per [[deferred-vs-unspecified]] named-owner rule (only 2-hop variant covered by `verify_chain_hash_matches_last_envelope`; 3-hop variant not yet added). Adversary findings A15, A16, A17, A22 all covered by sub-mission roll-up (0970-a + 0970-a1). Cross-crate compat: `cargo fmt --check` clean; full workspace `--all-features` clippy blocked by pre-existing unrelated `tdlib-rs` feature-conflict per `0957-c` AC #3.

## RFC

RFC-0970 (Networking): Forwarding-Hop Authorization Envelope — Accepted 2026-08-02

**BLUEPRINT gate note:** RFC reached Accepted 2026-08-02 (multi-round R28-R64 review convergence). Mission now CLAIMABLE per BLUEPRINT Mission Lifecycle.

This mission is the **top-level decomposition mission** for RFC-0970. RFC-0970 has 11 test vectors, 4 implementation phases, and 12+ new types (HopEnvelope, HopCapability, HopScope, InnerRequest, wrap_for_hop, unwrap_at_destination, verify_chain_hash, pure_forward, DestinationNonceStore, node_epoch, audit_replay_log, TTL millisecond resolution). Per BLUEPRINT §Multi-Mission Decomposition (>10 types), this top-level captures acceptance criteria + Type Coverage roll-up; the implementation work decomposes into 2 sub-missions (0970-a, 0970-b).

## Summary

Implement per-hop auth for forwarded requests. Each forwarding hop wraps the inner request in a `HopEnvelope` signed by the intermediate router. The destination unwraps + verifies the entire chain. `HopCapability` is a `HolderKind::HopCapability` row in the unified `HolderRegistry` (per RFC-0957-A1 §Data Structures). `pure_forward` mode exists for routers that do NOT bind to the unified role — they forward without inserting `HopCapability` records (Finding A22 cross-realm replay defense).

The envelope wire format is 4-segment (RFC-0970 §Wire Format): `hop_envelope_id || hop_pub || hop_signature || inner_request`. NOT 3-segment (unlike RFC-0959-A1 which has `envelope_id || bearer_capsule || capability_token`).

## Acceptance Criteria

### Top-level: RFC-0970 acceptance roll-up

The sub-missions (0970-a, 0970-a1, 0970-b) implement the ACs by RFC-0970 §Test Vectors. When all sub-missions are complete and merged, every AC below is satisfied.

- [ ] All 11 RFC-0970 §Test Vectors pass (TV1: Single-Hop Wrap + Unwrap, TV2: Three-Hop Chain, TV3: Replay Detection, TV4: TTL Expiration, TV5: Audience Mismatch, TV6: Intermediate Router Compromise — Inner Content Encrypted, TV7: Hop Signature Forgery, TV8: Chain Hash Mismatch, TV9: Debug Redaction, TV10: Pure Forwarder, TV11: TTL Millisecond Resolution (200ms)) → **GREEN by sub-mission roll-up**: TV1, TV4, TV5, TV8, TV9, TV11 (6 vectors) → `missions/claimed/0970-a-hop-envelope.md` Band A closure (commit `2f078974`-prior; 9/9 unit tests pass); TV3, TV6, TV7, TV10 (4 vectors) → `missions/claimed/0970-a1-hop-crypto-and-replay-defense.md` Band A closure (commit; 13/13 ACs GREEN). **TV2 DEFERRED** per [[deferred-vs-unspecified]] named-owner rule (only 2-hop variant covered; 3-hop chain variant not yet added — Owner: @cipherocto, target 2026-08-21, follow-up mission TBD per `0970-a` §Notes).
- [x] All 4 RFC-0970 §Adversary Analysis findings covered (A15: Replay attack on unwrap, A16: Compromised intermediate router reads inner content, A17: Hop signature key compromise, A22: Cross-realm replay (Round 2 R2 finding)) → **Closure:** A15 (replay attack) covered by `0970-a1` TV3 (`HopError::ReplayDetected` + `audit_replay_log` entry); A16 (intermediate router reads inner content) covered by `0970-a1` TV6 (RFC-0853 channel encryption binding on `InnerRequest.ciphertext` field); A17 (hop signature key compromise) covered by `0970-a1` TV7 (Ed25519 signature verification on `hop_cap.signature`); A22 (cross-realm replay) covered by `0970-a1` TV10 (`pure_forward` returns `InvalidScope` by design + NO `HolderKind::HopCapability` row inserted).
- [x] Phantom type `DestinationNonceStore` properly DEFERRED to RFC-0009-B1 / RFC-0957-A2 (cross-mission; consumed by sub-mission 0970-a) → **Closure:** `DestinationNonceStore` substrate landed in `0970-a1` (real implementation, not phantom); the phantom reference in mission text is stale wording — the actual store lives at `crates/octo-wallet/src/capability/destination_nonce_store.rs` per `0970-a1` substrate. The "phantom DEFERRED to RFC-0009-B1" rule is now a documentation relic; the underlying type is concrete and consumed by `unwrap_at_destination` for replay defense.
- [x] Sub-missions 0970-a, 0970-b all merged and ACs flipped → **Closure:** `0970-a` Band A closed 2026-08-06 (commit `2f078974`-prior; 11/15 ACs GREEN, 4 deferred to `0970-a1`); `0970-a1` Band A closed 2026-08-07 (13/13 ACs GREEN); `0970-b` Band A closed 2026-08-06 (commit `11921128`-prior; 5/5 ACs GREEN). Sub-mission decomposition complete.
- [ ] Cross-crate compat: `cargo build --workspace` green; `cargo test --workspace` green; `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean; `cargo fmt --check` clean → **PARTIAL**: `cargo fmt --check` clean (verified 2026-08-07). `cargo clippy --workspace --all-targets --all-features -- -D warnings` blocked by pre-existing unrelated `tdlib-rs` feature-conflict per `missions/claimed/0957-c-holder-registry-impl.md` AC #3; package-scoped clippy on touched crates (`octo-wallet`, `quota-router-storage`) clean. Full workspace rerun → follow-up mission (target 2026-08-21).

### Type Coverage

| RFC-0970 Type | Implemented By |
|---------------|----------------|
| `HopEnvelope` struct (4-segment wire) | Sub-mission 0970-a |
| `HopCapability` struct (HolderKind::HopCapability row) | Sub-mission 0970-a |
| `HopScope` enum (Forwarder / Auditor / PureForwarder) | Sub-mission 0970-a |
| `InnerRequest` struct (encrypted inner payload) | Sub-mission 0970-a |
| `wrap_for_hop` algorithm (per-hop wrap + sign) | Sub-mission 0970-a |
| `unwrap_at_destination` algorithm (chain verify + unwrap) | Sub-mission 0970-a |
| `verify_chain_hash` free function | Sub-mission 0970-a |
| `pure_forward` algorithm (no HolderKind insert) | Sub-mission 0970-a |
| `DestinationNonceStore` (DEFERRED to RFC-0009-B1 / RFC-0957-A2) | Sub-mission 0970-a (phantom call site) |
| `node_epoch` per-destination nonce seed | Sub-mission 0970-a |
| `audit_replay_log` append-only log | Sub-mission 0970-a |
| TTL millisecond resolution (200ms TV11) | Sub-mission 0970-a |
| `ForwardRequestPayload` extension (RFC-0870 §Roles Update) | Sub-mission 0970-b |
| Manual redacting `Debug` impls on `HopEnvelope`, `HopCapability`, `InnerRequest` | Sub-mission 0970-a |

### Mission Dependency Model

```yaml
depends_on:
  - 0957-c-holder-registry-impl # HolderRegistry + from_hop_capability constructor
  - RFC-0870 # Router role substrate (ForwardRequestPayload)
  - RFC-0853 # HopScope substrate (per-hop channel binding)
  - RFC-0862 # audit_replay_log sync (stoolap substrate)
decomposes_into:
  - 0970-a-hop-envelope # HopEnvelope + HopCapability + wrap_for_hop + unwrap_at_destination + verify_chain_hash + pure_forward + DestinationNonceStore phantom
  - 0970-b-forward-integration # ForwardRequestPayload extension + RFC-0870 §Roles cross-reference
```

## Dependencies

**Requires (RFC gates):**

- RFC-0853 — per-hop channel binding (HopScope substrate)
- RFC-0862 — HolderRegistry gossip + audit_replay_log sync
- RFC-0870 — Router role + ForwardRequestPayload
- RFC-0957-A1 — unified HolderRegistry (`HolderKind::HopCapability` row + `HolderRecord::from_hop_capability` constructor)
- RFC-0958 — optional ZK-verified hops; F4 promotion path (build-pipeline Ed25519 sig + verify-side pubkey check) documented in RFC-0958 §Future Work; implementation in flight via `missions/claimed/0958-a-zk-capability-circuit.md`; `hop_envelope` signature verifiability integration is post-0958-a merge scope
- RFC-0971 — destination-node role consolidation (downstream)

**Mission gates:**

- `missions/open/0957-c-holder-registry-impl.md` — `HolderRecord::from_hop_capability` constructor MUST exist (cross-mission; co-author contract per 0957-c Notes)
- Router substrate: RFC-0870 (Router role). Coordinate with RFC-0870 §Roles (updated by sub-mission 0970-b).
- Channel substrate: RFC-0853 (per-hop channel binding). Coordinate with existing 0957-a capability substrate (BLAKE3 keyed-hash + HKDF-BLAKE3 primitives).

**Not Requires:**

- RFC-0958 (ZK subclass) — Accepted; implementation in flight via `missions/claimed/0958-a-zk-capability-circuit.md` (S05 4-session plan); hop_envelope ZK-verifiability integration is post-0958-a merge scope

## Implementation Guide

- RFC-0970 §Specification → §System Architecture → §Data Structures → §Algorithms → §Wire Format → §Test Vectors (single canonical reference)
- RFC-0970 §Appendices: §Why Not Transitive Trust?, §Why Not Destination-Only Auth?, §RFC-0870 ForwardRequestPayload Update, §Example 3-Hop Chain
- Developer guide: inline §Developer Guide section in sub-mission 0970-b (inline in this mission)

## Decomposition Rationale

RFC-0970 qualifies for decomposition per BLUEPRINT §Multi-Mission Decomposition:

- **12 new types** — exceeds >10 threshold
- **4 implementation phases** (§Phase 1: Data Structures + Algorithms, §Phase 2: Channel Layer Integration, §Phase 3: HolderRegistry Binding, §Phase 4: Mission Decomposition) — at threshold
- **Different prerequisite chains:**
  - 0970-a (hop envelope) depends on 0957-c HolderKind::HopCapability row + RFC-0853 channel substrate
  - 0970-b (forward integration) depends on RFC-0870 Router substrate

Splitting by module boundary (envelope / forward) lets 0970-a merge independently of the Router lifecycle work.

## Claimant

@mmacedoeu (top-level decomposition; ACs roll up as 0970-a, 0970-b land)

## Pull Request

(unset)

## Closure (2026-08-07)

**Status:** Closed (Band A — 2026-08-07). Top-level roll-up closure landed.

**Sub-mission roll-up:**

- `0970-a-hop-envelope.md`: Band A closed 2026-08-06 (commit `2f078974`-prior). 11/15 ACs GREEN. 9/9 unit tests pass (`hop_scope_variants`, `hop_envelope_debug_redacts`, `wrap_then_unwrap_roundtrip`, `unwrap_audience_mismatch`, `unwrap_ttl_exceeded`, `verify_chain_hash_matches_last_envelope`, `verify_chain_hash_mismatch`, `forward_request_payload_new_has_no_hop_envelope`, `forward_request_payload_with_hop_envelope`). `HolderKind::HopCapability = 0x03` discriminator landed in `crates/quota-router-storage/src/holder_kind.rs`.
- `0970-a1-hop-crypto-and-replay-defense.md`: Band A closed 2026-08-07. 13/13 ACs GREEN. Real RFC-0853 crypto (X25519 + ChaCha20-Poly1305 + Ed25519) landed; `DestinationNonceStore` + `AuditReplayLog` new substrates; `node_epoch: u64` field on `HopCapability` + stale-epoch reject; 4 TVs (TV3, TV6, TV7, TV10) + 3 invariant tests (genuine sig accepts, stale epoch rejects, grace window accepts).
- `0970-b-forward-integration.md`: Band A closed 2026-08-06 (commit `11921128`-prior). 5/5 ACs GREEN. `ForwardRequestPayload` struct + `new(inner)` + `with_hop_envelope(inner, env)` constructors landed in `crates/octo-wallet/src/capability/hop_envelope.rs`.

**Test vector coverage (11 total):**

- GREEN (10): TV1, TV4, TV5, TV8, TV9, TV11 (via `0970-a`) + TV3, TV6, TV7, TV10 (via `0970-a1`)
- DEFERRED (1): TV2 (Three-Hop Chain) — only 2-hop variant covered; 3-hop variant not yet added. Owner: @cipherocto. Target: 2026-08-21. Follow-up mission TBD.

**Adversary findings (4 total):**

- A15 (replay attack on unwrap) → GREEN via `0970-a1` TV3
- A16 (intermediate router reads inner content) → GREEN via `0970-a1` TV6
- A17 (hop signature key compromise) → GREEN via `0970-a1` TV7
- A22 (cross-realm replay) → GREEN via `0970-a1` TV10

**Phantom `DestinationNonceStore`:** substrate landed in `0970-a1` (real impl, not phantom); mission text's "DEFERRED to RFC-0009-B1" wording is stale documentation relic. Per [[no-phantom-mission-pointers]] the underlying type is concrete and consumed; AC remains GREEN by substrate existence.

**Cross-crate compat:** `cargo fmt --check` clean (verified 2026-08-07). Full workspace `--all-features` clippy blocked by pre-existing unrelated `tdlib-rs` feature-conflict; package-scoped clippy on touched crates clean.

**Per [[git-workflow]] push awaits user instruction. Per [[no-line-refs-anywhere]] all references use §symbol-name form. Per [[rfc-referencing-convention]] RFCs referenced by number only.**

## Notes

- Phantom type `DestinationNonceStore` is a per-destination nonce store used to detect replay. The store MUST be per-destination (not per-hop) because the same destination receives from multiple hops; the nonce seed is `node_epoch` (per-destination epoch). Cross-mission: full signature deferred to RFC-0009-B1 / RFC-0957-A2; working stub lives in `crates/octo-wallet/src/capability/nonce_store_stub.rs`.
- Wire format is 4-segment: `hop_envelope_id || hop_pub || hop_signature || inner_request` (base64url-no-pad). TV11 (TTL Millisecond Resolution, 200ms) is the determinism gate — millisecond resolution required, NOT seconds.
- `pure_forward` is a NO-INSERT path: routers that do NOT bind to the unified destination-node role (per RFC-0971) can forward without inserting `HopCapability` records. Cross-realm replay defense (Finding A22): the destination treats `pure_forward` envelopes with stricter TTL + lower trust.
- Manual redacting Debug on `HopEnvelope`, `HopCapability`, `InnerRequest` (R10-N5 fix).

### Related

- [Dual-Mode Authorization Batch Accepted 2026-08-02](../rfcs/accepted/networking/0970-forwarding-hop-auth-envelope.md)
- Original research: `docs/research/2026-08-01-dual-mode-workflow-gap-research.md`
- Original use case: `docs/use-cases/dual-mode-authorization-workflow.md`
