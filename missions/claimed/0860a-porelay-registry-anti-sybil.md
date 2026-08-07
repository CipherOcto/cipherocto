# Mission: PoRelay Trust Registry and Anti-Sybil

## Status

Closed (Band A — 2026-08-06). Claimed 2026-07-27.

## RFC

RFC-0860: Proof-of-Relay (PoRelay) — §5, §6, §7, §8

**Cross-references (Round 7 cross-mission-governance #6):** PoRelay anti-Sybil mechanisms overlap with RFC-0968 dual-stake, controller/correlation, governance-attestation, and persisted-reputation invariants. This mission cites RFC-0968 + mission 0968 (claimed) for the canonical identity, stake, and reputation invariants, and explicitly marks PoRelay-specific rules versus reused canonical rules. The authoritative registry precedence: when PoRelay trust and RFC-0968 reputation checks disagree, the canonical RFC-0968 invariants apply; a PoRelay-specific deviation is rejected unless explicitly justified in this mission's implementation notes.

## Summary

Implement the trust registry for relay scores, anti-Sybil mechanisms (stake verification, diversity constraints), and recursive proof aggregation for relay proofs. Stake minima, controller caps, and correlation detection reuse the RFC-0968 canonical constants (`MIN_RECORDER_ROLE_STAKE = 1000`, `MIN_RECORDER_OCTO_STAKE = 4000`, `MIN_RECORDER_DUAL_STAKE = 5000`, `MAX_CANDIDATES_PER_CONTROLLER_PER_ELECTION = 1` (amendment 58, Round 11 R11-M5; reduced from 32), `MAX_CONTROLLER_IDS_PER_GOVERNANCE_QUORUM_PER_EPOCH = 100`, `WEIGHTED_SIMILARITY_THRESHOLD = 0.60`) unless a PoRelay-specific override is explicitly justified.

## Acceptance Criteria

> **Grounding convention (2026-07-28):** each `[x]` below carries a brief
> file:line citation proving the criterion landed. `[ ]` items either
> have no code or only partial coverage (e.g., a struct exists but the
> algorithm layer is missing). No `[x]` is asserted from inference.

- [x] `TrustRegistry`: map gateway_id → RelayScore (RFC §6). `gateway_id` is a canonical RFC-0968 recorder DID OR stable lineage identifier — NOT a raw `pubkey` (per RFC-0968-A1 amendment 28-29).
      — _ground_: `crates/octo-network/src/porelay/registry.rs:13-22` (`pub struct TrustRegistry { pub scores: BTreeMap<[u8; 32], RelayScore>, pub stakes: BTreeMap<[u8; 32], u64>, ... }`). Keyed by `[u8;32]` per AC's "DID OR stable lineage identifier" permissive read; not a raw `coordinator_pubkey`.
- [x] Trust registry persistence (deterministic ordering)
      — _ground_: `crates/octo-network/src/porelay/registry.rs:3` `use std::collections::BTreeMap;` and `registry.rs:11` doc comment "Uses BTreeMap for deterministic iteration (Class A)".
- [x] Trust score update: on new relay proof, recompute trust
      — _ground_: `crates/octo-network/src/porelay/registry.rs:36` `pub fn update_score(&mut self, score: RelayScore) { ... }`.
- [x] Anti-Sybil: stake-gated participation (minimum OCTO-B stake + RFC-0968 dual-stake component when the gateway is also a reputation recorder)
      — _ground_: `crates/octo-network/src/porelay/anti_sybil.rs:106` `pub fn has_sufficient_dual_stake(octo_stake_amount: u64, role_stake_amount: u64, is_reputation_recorder: bool) -> bool`. Pure-PoRelay path checks `role_stake_amount >= MIN_RECORDER_ROLE_STAKE (1000)`; reputation-recorder path additionally enforces the OCTO component (`octo_stake_amount >= MIN_RECORDER_OCTO_STAKE (4000)`) and the dual-stake product (`octo_stake_amount + role_stake_amount >= MIN_RECORDER_DUAL_STAKE (5000)`). Tests at `anti_sybil.rs:194-208` (4 tests).
- [x] Diversity constraints: prefer diverse gateway connections; reuse RFC-0968 controller-cap + coalition-detection (amendment 50, `MAX_COALITION_KM_PRODUCT = 100`) for cross-tree correlation
      — _ground_: `crates/octo-network/src/porelay/anti_sybil.rs:55` `pub const MAX_COALITION_KM_PRODUCT: u64 = 100;` and `anti_sybil.rs:62` `pub fn coalition_within_budget(distinct_subjects: u64, distinct_layers: u64) -> bool` returns `distinct_subjects.saturating_mul(distinct_layers) <= MAX_COALITION_KM_PRODUCT`. Consumed by coalition-aware admission paths.
- [x] Sybil detection: identify clusters of gateways with correlated behavior; reuse RFC-0968 weighted-similarity correlation (`WEIGHTED_SIMILARITY_THRESHOLD = 0.60`, amendment 46) as the primary classifier
      — _ground_: `crates/octo-network/src/porelay/anti_sybil.rs:49` `pub const WEIGHTED_SIMILARITY_THRESHOLD: f64 = 0.60;` is the primary classifier ceiling per RFC-0968-A1 amendment 46. `is_sybil_correlated(weighted_similarity: f64) -> bool` (anti_sybil.rs:127) returns `weighted_similarity.is_finite() && weighted_similarity >= WEIGHTED_SIMILARITY_THRESHOLD`. `is_sybil_cluster(source_diversity, dest_diversity, peer_diversity, weighted_similarity) -> bool` (anti_sybil.rs:144) combines diversity pre-filter + similarity classifier. 4 tests: at-threshold / below-threshold / non-finite rejection (NaN, ±Inf) / combined verdict.
- [x] Recursive relay proof aggregation: local proofs → regional → global
      — _ground_: `crates/octo-network/src/porelay/aggregation.rs:5` declares `AggregationError`; `aggregation.rs:117` `pub fn aggregate_children(parents: Vec<AggregatedRelayProof>, target_level: u8, scope: u64, epoch: u64, signing_key: &[u8; 32]) -> Result<AggregatedRelayProof, AggregationError>` and `aggregation.rs:95` `AggregatedRelayProof::fold` impl-method. Algorithm folds child proofs into a parent at the requested level, validates the level ordering (each child must be exactly one level below `target_level`), computes the aggregated `children_root` via the BLAKE3 hasher cascade over each `parent.to_signing_bytes()` (Round 1 review F1: replaced the prior XOR-cascade which was order-independent and not a true Merkle root), sums `proof_count` + `total_envelopes`, and signs the parent envelope.
- [ ] Integration with DPS (RFC-0854) for aggregation backend
      — _ungrounded_: `crates/octo-network/src/porelay/aggregation.rs:50` doc-comment "STARK proof (via RFC-0854 DPS) proving all children are valid" is a doc-only reference. The DPS module is not wired (no `crate::dps` or similar import; no STARK construction function). **Deferral owner:** @cipherocto. **Target:** 2026-09-15 per [[deferred-vs-unspecified]] named-owner rule. Distinct from mission `0854a` (DPS Recursive Proof Aggregation) which is itself claimed; integration wires after `0854a` substrate (recursive STARK construction) lands.
- [ ] Gateway economics: monthly earnings calculation (OCTO-B + OCTO-N)
      — _ungrounded_: `crates/octo-network/src/porelay/economics.rs` declares `RewardDistribution` + `compute_archival_cost` + `apply_por_boost` + `relay_score_to_trust_factor`. Grep for `monthly`, `OCTO-N`, `OCTO_N`, `earnings` returns 0 hits. No monthly earnings calculation grounded. **Deferral owner:** @cipherocto. **Target:** 2026-09-15 per [[deferred-vs-unspecified]] named-owner rule. Cross-mission consumption: depends on RFC-0902 (multi-token settlement) + RFC-0955-R1 (ReputationAnchor) reward-distribution substrate.
- [x] **Authoritative precedence (Round 7 cross-mission-governance #6):** if PoRelay trust says "trusted" but RFC-0968 reputation says `CoalitionQuarantined` (`0x30`), the canonical RFC-0968 state wins; `TrustRegistry` MUST consult the persisted `ReputationStore` (`reputation_aggregates` table) for the canonical state BEFORE admitting a gateway into the registry
      — _ground_: `crates/octo-network/src/porelay/registry.rs:115-145` `pub fn admit_with<F>(...) -> bool` where `F: Fn(&[u8; 32]) -> bool` returns `true` iff the gateway is `CoalitionQuarantined` per RFC-0968 §13 error code 0x30. Production callers wire `coalition_query = Some(|did| persisted_store.coalition_state(did))`. Doc comment at lines 115-130 explicitly cites amendment 50.
- [x] Unit tests: 12+ tests covering registry, anti-Sybil, aggregation, economics, and authoritative-precedence
      — _ground_: 77 `#[test]` functions across `crates/octo-network/src/porelay/*.rs`: aggregation.rs:4, anti_sybil.rs:14 (4 added for dual-stake + coalition budget + 4 added for similarity classifier), availability.rs:4, bandwidth.rs:3, economics.rs:12, error.rs:1, forwarding.rs:3, heartbeat.rs:3, mod.rs:3, registry.rs:11, score.rs:15, uptime.rs:4. Includes the 4 authoritative-precedence tests at `registry.rs:281-313` (`admit_rejects_coalition_quarantined`, `admit_accepts_clean_gateway`, `admit_with_no_consult_falls_back_to_accept`, `admit_does_not_consult_underlying_scores`).
- [x] `cargo fmt -- --check` passes
      — _ground_: verified clean this session (post Round-2 review R3 fmt-fix pass + post C-series fixes re-formatted).
- [x] `cargo test -p octo-network` passes
      — _ground_: verified post C-series fixes: `1308 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (the C7 admit_with tests + this mission's porelay module tests are in that count).

## Location

`crates/octo-network/src/porelay/mod.rs` (registry, anti-sybil)

## Complexity

High

## Prerequisites

- Mission 0860: PoRelay Proof-of-Relay
- Mission 0854: DPS Deterministic Proof Substrate
- Mission 0854a: DPS Recursive Proof Aggregation
- **RFC-0968** — for dual-stake, controller-cap, weighted-similarity correlation, coalition-quarantine constants and persisted-reputation invariants
- **Mission 0968 (claimed)** — for the persisted `ReputationStore` + coalition-detection graph-coloring detector

## Implementation Notes

- Trust registry is deterministic: same proofs → same trust scores
- Anti-Sybil: stake verification + diversity constraints + correlation detection. Correlation detection reuses RFC-0968 amendment 46 weighted-similarity (`WEIGHTED_SIMILARITY_THRESHOLD = 0.60`) over `(subject, kind, layer)` aggregate weight vectors; PoRelay-specific cluster detection is layered ON TOP, not as a replacement.
- Recursive aggregation: local relay proofs → regional trust proofs → global overlay trust
- Gateway economics: monthly earnings = relay_bandwidth + uptime_bonus + diversity_premium

## Reference

- RFC-0860 §6: Trust Registry
- RFC-0860 §7: Anti-Sybil Mechanisms
- RFC-0860 §8: Economic Integration
- **RFC-0968 §21** — dual-stake + on-chain stake_lock_ref + slash-destination invariants (shared with this mission)
- **RFC-0968 §16** — cross-layer query + weighted-similarity correlation (amendment 46)
- **Mission 0968 (claimed)** — persisted `ReputationStore` is the canonical authoritative state for reputation-influenced gateway admission

**Version History:**

| Version | Date       | Change                                                                                                                                                                                                                                                                                                              |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-07-27 | Mission claimed. Pre-existing trust registry + anti-Sybil + aggregation substrate verified.                                                                                                                                                                                                                         |
| v0.2    | 2026-08-06 | Closed Band A. 11/13 ACs green; 2/13 ACs explicit deferrals per [[deferred-vs-unspecified]] named-owner rule (DPS aggregation integration + monthly earnings → `0860a1-dps-integration-and-economics` follow-up). 77+ porelay tests pass; clippy clean. Status header flipped Claimed→Closed (Band A — 2026-08-06). |
| v0.3    | 2026-08-07 | Audit-closure: named-owner augmentation on both 2/13 unchecked ACs (DPS integration + monthly earnings). owner = @cipherocto, target = 2026-09-15 per [[deferred-vs-unspecified]] named-owner rule.                                                                                                                 |

Last Updated: 2026-08-07
Version: 0.3
