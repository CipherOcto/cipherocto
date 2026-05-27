# Mission: DPS Recursive Proof Aggregation

## Status

Open

## RFC

RFC-0854: Deterministic Proof Substrate (DPS) — §8

## Summary

Implement recursive proof aggregation following RFC-0650 (Proof Aggregation Protocol) with binary tree aggregation, O(1) verification, and first-seen-wins conflict resolution.

## Acceptance Criteria

- [ ] Binary tree aggregation: local proofs → regional proofs → global overlay proof
- [ ] O(1) verification of aggregated proofs
- [ ] First-seen-wins for double-aggregation conflicts
- [ ] Integration with RFC-0650 actors: Worker, Collector, Aggregator, Verifier
- [ ] `AggregatedProof` with child_proofs, aggregation_root, proof_count
- [ ] Aggregation commitment: BLAKE3-256(child_commitment_0 || child_commitment_1)
- [ ] Recursive depth limit (configurable, default 10)
- [ ] Unit tests: 10+ tests covering aggregation, verification, conflict resolution
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dps/aggregation.rs`

## Key Files

| File | Change |
|------|--------|
| `aggregation.rs` | New file — recursive aggregation logic |
| `mod.rs` | Add aggregation module |

## Complexity

High (3-5 days)

## Prerequisites

- Mission 0854: DPS Deterministic Proof Substrate
- RFC-0650: Proof Aggregation Protocol (reference)

## Implementation Notes

- Binary tree aggregation: each level combines two child proofs into one parent
- O(1) verification: verifier only checks the root proof, not all children
- First-seen-wins: if two aggregations for the same set exist, the first one is canonical
- Integration with RFC-0650's Worker/Collector/Aggregator/Verifier roles

## Reference

- RFC-0854 §8: Recursive Aggregation
- RFC-0650: Proof Aggregation Protocol
