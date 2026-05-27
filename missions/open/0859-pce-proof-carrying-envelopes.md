# Mission: PCE Proof-Carrying Envelopes

## Status

Open

## RFC

RFC-0859: Proof-Carrying Envelopes (PCE)

## Summary

Implement proof-carrying envelopes with proof attachment, verification pipeline, canonical proof boundary enforcement, and integration with DPS (RFC-0854) and DOM (RFC-0857).

## Acceptance Criteria

- [ ] `ProofCarryingEnvelope` with envelope (RFC-0850), proof_system_id, proof_commitment, public_input_root, proof_blob, execution_model, parent_proof_commitment (7 fields per RFC §3.1)
- [ ] `ProofSystemId` enum #[repr(u16)] matching RFC §3.1: STWO=0x0001, RiscZero=0x0002, SP1=0x0003, Winterfell=0x0004, Halo2=0x0005, Groth16=0x0006, PLONK=0x0007, Cairo=0x0008
- [ ] `VerificationResult` enum matching RFC §5.1: Valid, Invalid, UnsupportedSystem, MalformedProof, InputMismatch
- [ ] `AggregatedProof` with inner_proof_commitments, aggregated_blob, aggregation_system, aggregated_public_input_root, proof_count (5 fields per RFC §7.2)
- [ ] `MissionProofPolicy` with mission_id, required_proof_types, allowed_proof_systems, min_security_level, require_aggregation, max_proof_age (6 fields per RFC §8)
- [ ] Proof verification pipeline: deserialize → verify commitment → verify proof → check execution class
- [ ] Canonical proof boundary: consensus NEVER depends on prover runtime, hardware, proving time
- [ ] Verification latency requirements per proof system (STARK <50ms, PLONK <50ms, RISC0 <200ms) per RFC §5.2
- [ ] RFC-0008 execution class mapping: proof generation=Class C, verification=Class A
- [ ] Integration with RFC-0854 (DPS) for proof system abstraction
- [ ] Integration with RFC-0857 (DOM) for intent proof attachment
- [ ] `PceError` enum with all error variants
- [ ] Unit tests: 10+ tests covering verification pipeline, boundary enforcement, intentegration
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dot/pce/`

## Complexity

High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0853: OCrypt Overlay Cryptography
- Mission 0854: DPS Deterministic Proof Substrate

## Optional Dependencies

- Mission 0857: DOM Deterministic Overlay Mempool (for intent proof attachment)

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Canonical proof boundary is the CRITICAL invariant — enforce with error types
- Proof verification is Class A (deterministic across all implementations)
- Proof generation is Class C (non-deterministic, OS randomness)
- Verification latency: measured in production, requirements are targets

## Type Coverage

| RFC-0859 Type | Implemented By |
|---------------|---------------|
| ProofCarryingEnvelope | This mission |
| ProofSystemId | This mission |
| ProofCircuitModel | This mission |
| VerificationResult | This mission |
| AggregatedProof | This mission |
| MissionProofPolicy | This mission |
| ProofType | Mission 0859a |
| PceError | This mission |

## Reference

- RFC-0859: Proof-Carrying Envelopes (§2, §3, §4, §5, §6, §7, §8, §9, §10)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree, Error Types)
