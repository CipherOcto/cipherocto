# Mission: PCE Proof-Carrying Envelopes

## Status

Open

## RFC

RFC-0859: Proof-Carrying Envelopes (PCE)

## Summary

Implement proof-carrying envelopes with proof attachment, verification pipeline, canonical proof boundary enforcement, and integration with DPS (RFC-0854) and DOM (RFC-0857).

## Acceptance Criteria

- [ ] `ProofCarryingEnvelope` with envelope (RFC-0850), proof_system_id, proof_commitment, public_input_root, proof_blob
- [ ] `ProofSystemId` struct with backend_id, scheme_id, security_level
- [ ] `VerificationResult` struct with valid, verification_time_us, error_detail
- [ ] `AggregatedProof` struct with constituent_proofs, aggregation_root, proof_count
- [ ] `MissionProofPolicy` struct with required_proof_types, min_security_level, max_verification_time_ms
- [ ] Proof verification pipeline: deserialize → verify commitment → verify proof → check execution class
- [ ] Canonical proof boundary: consensus NEVER depends on prover runtime, hardware, proving time
- [ ] Consensus MAY depend ONLY on: (public_inputs, canonical_verifier, proof_bytes, verification_result)
- [ ] Verification latency requirements per proof system (STARK <50ms, PLONK <20ms, RISC0 <100ms)
- [ ] RFC-0008 execution class mapping: proof generation=Class C, verification=Class A
- [ ] Integration with RFC-0854 (DPS) for proof system abstraction
- [ ] Integration with RFC-0857 (DOM) for intent proof attachment
- [ ] `PceError` enum with all error variants
- [ ] Unit tests: 10+ tests covering verification pipeline, boundary enforcement, intentegration
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/pce/`

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

## Reference

- RFC-0859: Proof-Carrying Envelopes (§2, §4, §5, §6, §9)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree, Error Types)
