# Mission: PCE Proof Types and Recursive Aggregation

## Status

Open

## RFC

RFC-0859: Proof-Carrying Envelopes (PCE) — §4, §6, §7

## Summary

Implement proof type registry, proof attachment protocol (how proofs attach to intents), and recursive proof aggregation for batch verification.

## Acceptance Criteria

- [ ] `ProofType` enum #[repr(u16)] matching RFC §4.1: InferenceProof, DatasetIntegrityProof, MissionExecutionProof, RelayProof, ValidatorAttestation, AggregatedProof, MembershipProof, StateTransitionProof, DataIntegrityProof
- [ ] Proof type registry: map ProofType → verification function
- [ ] Proof attachment protocol: how proofs attach to DOM intents (RFC-0857)
- [ ] Attachment validation: verify proof matches intent type
- [ ] Recursive aggregation: combine multiple proofs into single verifiable proof
- [ ] Aggregated proof verification: O(1) verification of batch
- [ ] Integration with DPS (RFC-0854) for aggregation backend
- [ ] Integration with DOM (RFC-0857) for intent proof attachment
- [ ] Unit tests: 10+ tests covering proof types, attachment, aggregation
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dot/pce/mod.rs` (types, attachment, aggregation)

## Complexity

High

## Prerequisites

- Mission 0859: PCE Proof-Carrying Envelopes
- Mission 0854: DPS Deterministic Proof Substrate
- Mission 0854a: DPS Recursive Proof Aggregation
- Mission 0857: DOM Deterministic Overlay Mempool

## Implementation Notes

- Proof types map to specific verification functions
- Attachment protocol: proof is part of the intent, verified at admission
- Recursive aggregation uses DPS (RFC-0854) aggregation backend
- Aggregated proofs enable batch verification (O(1) for N proofs)

## Reference

- RFC-0859 §4: Proof Types
- RFC-0859 §6: Proof Attachment Protocol
- RFC-0859 §7: Recursive Proof Aggregation
