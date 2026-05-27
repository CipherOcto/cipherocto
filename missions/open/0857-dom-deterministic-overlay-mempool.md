# Mission: DOM Deterministic Overlay Mempool

## Status

Open

## RFC

RFC-0857: Deterministic Overlay Mempool (DOM)

## Summary

Implement the deterministic overlay mempool with overlay intents (8 types), execution classes (7 levels), canonical admission rules, deterministic ordering, deterministic eviction, mission-scoped pools, and fee model integration.

## Acceptance Criteria

- [ ] `OverlayIntent` with intent_id, intent_type, mission_id, sender_id, sequence, logical_timestamp, expiration, payload_root, economic_weight, execution_class, signature (RFC §1)
- [ ] `IntentType` enum: Transaction, MissionCommand, AIExecution, ConsensusVote, ProofSubmission, ResourceLease, GovernanceProposal, RelayCommitment
- [ ] `ExecutionClass` enum: CriticalConsensus, Consensus, MissionCritical, Economic, Standard, Bulk, Archive
- [ ] Canonical ordering: (execution_class ASC, economic_weight DESC, logical_timestamp ASC, sequence ASC, intent_id ASC) per RFC §4
- [ ] Deterministic admission: signature, replay window, sequence, mission authorization, resource constraints
- [ ] Deterministic eviction: lowest class → lowest weight → oldest timestamp
- [ ] Mission-scoped mempool isolation
- [ ] Fee model: base_fee=1 OCTO, intent_type_multiplier per class (RFC Economic Analysis), priority_premium max 2.0
- [ ] Fee distribution: 70/10/10/5/5 (whitepaper §10.6)
- [ ] IntentType to ExecutionClass mapping table (RFC §6.1)
- [ ] Capacity limits: max_pending_intents=100,000, max_per_mission=10,000
- [ ] `MempoolStateRoot` with BLAKE3-256 Merkle commitment
- [ ] `DomError` enum with all error variants
- [ ] Unit tests: 15+ tests covering ordering, admission, eviction, fee computation
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dom/`

## Complexity

High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0852: DGP Deterministic Gossip

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Canonical ordering is by (execution_class, economic_weight DESC, logical_timestamp, sequence, intent_id)
- Economic weight ordering is DESC within same class (higher weight = higher priority)
- Fee model: intent_fee = base_fee × intent_type_multiplier × (1 + priority_premium)
- Forbidden in admission: local latency, wall-clock, CPU load, thread order

## Reference

- RFC-0857: Deterministic Overlay Mempool (§4, §5, §6, §7, §8)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree)
- `docs/01-foundation/whitepaper/v1.0-whitepaper.md` (§10.6 fee structure)
