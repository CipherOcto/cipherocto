# Mission: DPS Deterministic Proof Substrate

## Status

Open

## RFC

RFC-0854: Deterministic Proof Substrate (DPS)

## Summary

Implement the ZK proof abstraction layer with DeterministicProofSystem trait, ProofSuiteId, ProofExecutionClass, proof backends (STARK, PLONK, RISC0), WitnessGenerator with DQA/DFP integration, and canonical proof boundary enforcement.

## Acceptance Criteria

- [ ] `DeterministicProofSystem` trait with prove(witness, trace_commitment, public_inputs), verify, proof_commitment, execution_model
- [ ] `ProofSuiteId` with proof_system, field_id, hash_id, recursion_scheme
- [ ] `ProofExecutionClass` enum: ClassA (verification), ClassB (deterministic witness), ClassC (OS random)
- [ ] `ProofCircuitModel` enum: AIR, R1CS, PLONKISH, zkVM, Recursive
- [ ] `ProofError` enum with InvalidWitness, TraceMismatch, ProofGenerationFailed, VerificationFailed, ConsensusBoundaryViolation
- [ ] Proof system registry (0x0001=STWO, 0x0002=RiscZero, 0x0003=SP1, 0x0004=Winterfell, 0x0005=Halo2, 0x0006=Groth16, 0x0007=PLONK, 0x0008=Cairo)
- [ ] `WitnessGenerator` trait with generate and fp_to_field_element
- [ ] DQA/DFP integration for deterministic witness generation
- [ ] Mission-scoped verifier configuration
- [ ] `VerifierRegistry` with BTreeMap entries, proof_suite, verification_key, registered_at, expires_at
- [ ] `MissionProofRequirement` with mission_id, required_backend, fallback_backends
- [ ] Backend capability advertisement via GDP bitmask (RFC-0851 §5)
- [ ] RFC-0008 execution class mapping table
- [ ] Unit tests: 10+ tests covering trait implementation, proof commitment, execution class
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/dps/`

## Key Files

| File | Change |
|------|--------|
| `mod.rs` | DPS module root |
| `trait.rs` | DeterministicProofSystem trait |
| `suite.rs` | ProofSuiteId and registries |
| `error.rs` | ProofError enum |
| `execution.rs` | ProofExecutionClass, ProofCircuitModel |
| `witness.rs` | WitnessGenerator trait |
| `verifier.rs` | VerifierRegistry, MissionProofRequirement |

## Complexity

High (3-5 days)

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0853: OCrypt Overlay Cryptography

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- `prove()` requires `witness: &Self::Witness` parameter (not omitted)
- ProofExecutionClass maps directly to RFC-0008: ClassA=Protocol Deterministic, ClassB=Deterministic Off-Chain, ClassC=Probabilistic
- WitnessGenerator MUST use RFC-0105 DQA arithmetic for all numeric operations
- Consensus NEVER depends on prover runtime, hardware, proving time

## Reference

- RFC-0854: Deterministic Proof Substrate (§1, §2, §3, §6, §7)
- `docs/07-developers/networking-implementation-guide.md` (Trait Definitions, Error Types)
- RFC-0630: Proof-of-Inference Consensus
- RFC-0650: Proof Aggregation Protocol
