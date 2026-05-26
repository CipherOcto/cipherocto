---
title: "RFC-0854: Deterministic Proof Substrate (DPS)"
status: Draft
version: 1.0.0
created: 2026-05-25
updated: 2026-05-25
authors:
  - CipherOcto Core Team
related:
  - RFC-0853 (Networking): Overlay Cryptography
  - RFC-0850 (Networking): Deterministic Overlay Transport
  - RFC-0126 (Numeric): Deterministic Serialization
  - RFC-0104 (Numeric): Deterministic Floating Point
  - RFC-0105 (Numeric): Deterministic Quant Arithmetic
  - RFC-0650 (Proof Systems): Proof Aggregation Protocol
---

# RFC-0854: Deterministic Proof Substrate (DPS)

## Status

Draft

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

The Deterministic Proof Substrate (DPS) defines a proof system abstraction layer for CipherOcto overlays, enabling multiple proving backends (STWO/STARK, PLONK, Halo2, RISC0, zkVM) under a unified deterministic interface.

The key design principle: **Consensus depends ONLY on deterministic proof verification semantics — NEVER on prover implementation details.**

DPS provides:

- Abstract proof interface (DeterministicProofSystem trait)
- Multi-backend support (STARK, PLONK, Halo2, RISC0, zkVM)
- Deterministic verification boundaries
- Recursive proof aggregation
- Mission-scoped verifier selection
- Proof-carrying envelope integration
- Cryptographic agility for proof systems

DPS is the "libp2p for ZK proofs" within CipherOcto — a universal proof substrate.

## Dependencies

**Requires:**

- RFC-0853 (Networking): OCrypt — cryptographic primitives
- RFC-0850 (Networking): DOT — envelope format
- RFC-0126 (Numeric): Deterministic Serialization
- RFC-0104 (Numeric): Deterministic Floating Point
- RFC-0105 (Numeric): Deterministic Quant Arithmetic

**Optional:**

- RFC-0650 (Proof Systems): Proof Aggregation Protocol — recursive aggregation

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1: Backend Agnostic | 5+ proof systems | STARK, PLONK, Halo2, RISC0, zkVM |
| G2: Deterministic Verification | 100% identical results | Cross-implementation consistency |
| G3: Recursive Aggregation | 1000:1 compression | Proofs per aggregated proof |
| G4: Verification Speed | <10ms per proof | Single proof verification |
| G5: Proof Compactness | <10KB per proof | Serialized proof size |
| G6: Cryptographic Agility | Algorithm migration | Backend swap without protocol change |
| G7: Mission Scoping | Per-mission verifier | Different proofs for different missions |

## Motivation

### CAN WE? — Feasibility Research

CipherOcto's architecture already assumes:

- Deterministic execution (Class A/B per RFC-0008)
- Merkle-heavy state
- Replay-safe envelopes
- Hash-oriented pipelines
- Massive distributed computation
- Parallel proving

STARK systems (especially StarkWare STWO) are highly aligned:

| CipherOcto Property | STARK Compatibility |
|-------------------|-------------------|
| Deterministic execution | Excellent |
| Merkle-heavy state | Native |
| Replay-safe proofs | Native |
| Hash-oriented pipelines | Native |
| Massive distributed computation | Excellent |
| Parallel proving | Excellent |
| AI/vector computation | Very promising |

However, binding to one proving system is strategically dangerous. DPS provides the abstraction layer.

### WHY? — Why This Matters

Without DPS:

- CipherOcto is locked to one proving system
- Future proof system advances require protocol changes
- Different missions cannot use different proof systems
- Recursive aggregation requires tight coupling
- Post-quantum migration is impossible without protocol rewrite

## Specification

### 1. Deterministic Proof Interface (DPI)

```rust
trait DeterministicProofSystem {
    type Proof;
    type VerificationKey;
    type PublicInputs;

    /// Generate a proof given trace commitment and public inputs
    fn prove(
        trace_commitment: [u8; 32],
        public_inputs: Self::PublicInputs,
    ) -> Result<Self::Proof, ProofError>;

    /// Verify a proof — MUST be deterministic across all implementations
    fn verify(
        vk: &Self::VerificationKey,
        public_inputs: &Self::PublicInputs,
        proof: &Self::Proof,
    ) -> Result<bool, ProofError>;

    /// Compute proof commitment (hash of proof for Merkle trees)
    fn proof_commitment(proof: &Self::Proof) -> [u8; 32];
}
```

### 2. Proof Execution Models

```rust
enum ProofExecutionModel {
    AIR,        // Algebraic Intermediate Representation (STARKs)
    R1CS,       // Rank-1 Constraint Systems (SNARKs)
    PLONKISH,   // PLONK-style circuits
    zkVM,       // Zero-knowledge virtual machine
    Recursive,  // Recursive composition
}
```

### 3. Proof Suite Identification

```rust
struct ProofSuiteId {
    proof_system: u16,      // STARK=1, PLONK=2, Halo2=3, RISC0=4, zkVM=5
    field_id: u16,          // Field identifier
    hash_id: u16,           // Hash function used in proof
    recursion_scheme: u16,  // Recursion strategy
}
```

### 4. Proof-Carrying Envelopes (RFC-0859 integration)

```rust
struct ProofCarryingEnvelope {
    envelope: DeterministicEnvelope,    // RFC-0850
    proof_system_id: u16,
    proof_commitment: [u8; 32],
    public_input_root: [u8; 32],
    proof_blob: Vec<u8>,
}
```

This enables: verifiable AI inference, mission correctness proofs, validator proofs, distributed execution attestations, privacy-preserving coordination.

### 5. Canonical Proof Boundary

**Consensus MUST NEVER depend on:** prover runtime, hardware acceleration, proving time, memory layout, parallel execution order, witness generation order.

**Consensus MAY depend ONLY on:** `(public_inputs, canonical_verifier, proof_bytes, verification_result)`

### 6. Deterministic Witness Model

Small numeric divergence (`0.30000000001` vs `0.29999999998`) can completely invalidate proofs. CipherOcto's deterministic numeric stack (RFC-0104 DFP, RFC-0105 DQA) becomes a ZK-safe arithmetic substrate for witness generation.

**DQA properties and AIR benefits:**

| DQA Property | AIR Benefit |
|-------------|-------------|
| Integer core | Native field arithmetic |
| Fixed scale | Constraint simplification |
| Canonicalization | Stable witness generation |
| Deterministic rounding | Reproducible traces |
| Bounded ranges | Lower proving cost |

### 7. Mission-Scoped Verifiers

Different missions MAY require different proof systems:

| Mission Type | Proof System |
|-------------|-------------|
| AI inference | STARK |
| Financial privacy | PLONK |
| Embedded edge devices | zkVM |
| Massive aggregation | Recursive STARK |
| Browser verification | SNARK |

CipherOcto supports all under one deterministic substrate.

### 8. Recursive Aggregation

```text
Level 0: Individual proofs (per-computation)
  ↓ aggregate
Level 1: Batch proofs (per-gateway per-window)
  ↓ aggregate
Level 2: Regional proofs (per-region per-epoch)
  ↓ aggregate
Level 3: Global proof (per-epoch)
```

Verification at any level is O(1) regardless of child count.

### 9. Integration with OCrypt

```text
Application / Missions
        ↓
Mission Execution Layer
        ↓
Deterministic Proof Substrate (DPS)
        ↓
Overlay Cryptography (OCrypt)
        ↓
DOT / DGP Networking
```

## Performance Targets

| Metric | Target |
|--------|--------|
| Proof verification (single) | <10ms |
| Proof generation | <1s (hardware-dependent) |
| Recursive aggregation | <100ms per level |
| Aggregated proof verification | <10ms (O(1)) |
| Proof size (STARK) | <50KB |
| Proof size (aggregated) | <10KB |

## Security Considerations

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Invalid proof acceptance | Critical | Deterministic verification |
| Prover side-channel | Medium | Proof boundary isolation |
| Witness manipulation | High | DQA/DFP deterministic numerics |
| Backend compromise | High | Cryptographic agility (swap backend) |

## Implementation Phases

### Phase 1: Core Abstraction (Months 1-3)
- DeterministicProofSystem trait
- ProofSuiteId
- ProofExecutionModel enum
- STARK backend integration (STWO or RISC0)

### Phase 2: Proof-Carrying Envelopes (Months 3-6)
- ProofCarryingEnvelope (RFC-0859)
- Mission-scoped verifier registry
- DQA/DFP witness integration

### Phase 3: Recursive Aggregation (Months 6-9)
- Multi-level aggregation pipeline
- O(1) aggregated verification
- Regional and global aggregation

### Phase 4: Multi-Backend and Agility (Months 9-12)
- PLONK backend
- Halo2 backend
- zkVM backend
- Backend negotiation protocol
- Post-quantum readiness

## Adversarial Review

| Threat | Impact | Mitigation | Verification |
|--------|--------|------------|--------------|
| Proof forgery | Critical | Deterministic verification with canonical verifier | Invalid proof rejection test |
| Invalid verifier key | Critical | Verification key commitment in Merkle tree | Key validation test |
| Replay of old proofs | High | Proof freshness via logical timestamp + mission_id | Replay detection test |
| Consensus isolation violation | Critical | Canonical proof boundary — consensus depends only on verification result | Boundary enforcement test |
| Prover DoS | Medium | Rate limiting + economic friction (OCTO-A stake) | Rate limit test |
| Recursive aggregation manipulation | High | Deterministic aggregation order + Merkle commitment | Aggregation consistency test |
| Backend compromise | High | Cryptographic agility — swap to alternate backend | Backend migration test |
| Witness manipulation | High | DQA/DFP deterministic witness generation | Witness determinism test |

## Test Vectors

### Proof Generation (STARK Backend)

```text
Input:
  trace_commitment = SHA-256("test_execution_trace")
  public_inputs = { mission_id: [0x01; 32], result_hash: SHA-256("result") }
  proof_suite = ProofSuiteId { proof_system: 1 (STARK), field_id: 1, hash_id: 1, recursion_scheme: 0 }

Expected:
  proof_blob = [valid STARK proof bytes]
  proof_commitment = SHA-256(proof_blob)
  verification_result = true when verified with matching vk
```

### Proof Verification (Deterministic)

```text
Input:
  vk = [verification key for STARK suite]
  public_inputs = { mission_id: [0x01; 32], result_hash: SHA-256("result") }
  proof = [proof from generation vector above]

Expected:
  verify(vk, public_inputs, proof) == Ok(true)
  verify(vk, public_inputs, tampered_proof) == Ok(false)
  verify(wrong_vk, public_inputs, proof) == Ok(false)
```

### DPI Trait Execution

```text
Input:
  trait implementation = STARK backend
  trace_commitment = [0x02; 32]
  public_inputs = { computation: "2 + 2 = 4" }

Execution:
  1. prove(trace_commitment, public_inputs) → proof
  2. proof_commitment(proof) → commitment (32 bytes)
  3. verify(vk, public_inputs, proof) → true

All three steps MUST produce identical output across all implementations.
```

## Economic Analysis

### Token Integration

| Activity | Token | Rationale |
|----------|-------|-----------|
| Proof generation compute | OCTO-A | GPU/ASIC-intensive proving workloads |
| Verifier node operation | OCTO-N | Running verification infrastructure |
| Proof orchestration | OCTO-O | Coordinating multi-proof missions |
| Aggregation rewards | OCTO-A | Recursive aggregation compute |

### Proof Economics

Proof generation costs scale with:

- Circuit complexity (constraint count)
- Backend type (STARK vs SNARK vs zkVM)
- Proof size requirements
- Recursion depth

```text
cost = base_cost * constraint_multiplier * backend_premium
```

| Backend | Relative Cost | Proof Size | Verification Speed |
|---------|--------------|------------|-------------------|
| STARK (STWO) | 1.0x | ~50KB | ~5ms |
| PLONK | 2.5x | ~1KB | ~3ms |
| Halo2 | 3.0x | ~1KB | ~3ms |
| RISC0 | 1.5x | ~100KB | ~10ms |
| zkVM | 2.0x | ~50KB | ~8ms |

### Aggregation Economics

Recursive aggregation reduces per-proof verification cost:

```text
effective_cost = proof_cost / aggregation_factor
```

At Level 3 (global), 1000 individual proofs compress to 1 aggregated proof with O(1) verification.

## Alternatives Considered

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| STARK-only (no abstraction) | Simple, proven | Locked to one system, no agility | Rejected — too risky |
| ZK-rollup approach | Batch efficiency | Requires sequencer, centralization risk | Rejected — wrong model |
| Optimistic verification | Low cost | Challenge period, latency | Rejected — not suitable for real-time |
| No proof abstraction | No overhead | Cannot swap backends, no future-proofing | Rejected — strategic risk |

**Decision:** DPS provides a universal proof abstraction that enables backend agility while maintaining deterministic verification semantics.

## Rationale

### Why abstract proof systems?

Without abstraction:

1. CipherOcto is locked to STARK (or any single system)
2. Future advances (new proof systems, post-quantum) require protocol rewrite
3. Different missions have different proof requirements — one size does not fit all
4. Recursive aggregation requires tight coupling to specific backend

DPS separates "what to prove" from "how to prove it" — the same deterministic interface works across all backends.

### Why DQA/DFP for witness generation?

Floating-point nondeterminism (`0.30000000001` vs `0.29999999998`) completely invalidates proofs. CipherOcto's deterministic numeric stack (RFC-0104 DFP, RFC-0105 DQA) ensures:

1. All witnesses are generated from identical numeric representations
2. Integer core maps directly to native field arithmetic
3. Fixed scale eliminates constraint complexity
4. Canonicalization ensures stable trace generation

Without DQA/DFP, proofs generated on different machines would fail verification due to floating-point drift.

### Why mission-scoped verifiers?

Different missions have different trust/security requirements:

- AI inference missions need STARK (transparent, no trusted setup)
- Financial privacy missions need PLONK (compact proofs, fast verification)
- Edge device missions need zkVM (general-purpose, low memory)
- Browser missions need SNARK (small proofs, fast client-side verification)

A single global proof system forces all missions to accept the same tradeoffs. Mission-scoped verifiers allow each mission to choose the optimal system.

## Compatibility

- **RFC-0850 (DOT):** DPS proof commitments are embedded in DOT envelopes via ProofCarryingEnvelope
- **RFC-0853 (OCrypt):** DPS uses OCrypt primitives (BLAKE3-256, Ed25519) for proof signatures
- **RFC-0126 (DCS):** Proof serialization uses Deterministic Canonical Serialization
- **RFC-0104/RFC-0105 (DFP/DQA):** Witness generation uses deterministic numeric arithmetic
- **RFC-0630 (Proof-of-Inference):** DPS generalizes PoI's proof model to arbitrary proof systems
- **RFC-0650 (Proof Aggregation):** DPS integrates with recursive aggregation protocol
- **Forward compatibility:** ProofSuiteId is extensible (0x0005-0xFFFF for future proof systems)

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/octo-proof/src/dps/mod.rs` | DPS module root |
| `crates/octo-proof/src/dps/trait.rs` | DeterministicProofSystem |
| `crates/octo-proof/src/dps/suite.rs` | ProofSuiteId |
| `crates/octo-proof/src/dps/envelope.rs` | ProofCarryingEnvelope |
| `crates/octo-proof/src/dps/recursive.rs` | Recursive aggregation |
| `crates/octo-proof/src/dps/verifier.rs` | Verifier registry |
| `crates/octo-proof/src/dps/witness.rs` | Deterministic witness |
| `crates/octo-proof/src/backends/stark.rs` | STARK backend |
| `crates/octo-proof/src/backends/plonk.rs` | PLONK backend |
| `crates/octo-proof/src/backends/halo2.rs` | Halo2 backend |

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-25 | Initial draft |

## Related RFCs

- RFC-0853 (Networking): OCrypt — cryptographic primitives
- RFC-0859 (Networking): PCE — proof-carrying envelopes
- RFC-0860 (Networking): PoRelay — relay proofs
- RFC-0650 (Proof Systems): Proof Aggregation
- RFC-0104 (Numeric): DFP — deterministic floating point
- RFC-0105 (Numeric): DQA — deterministic quant arithmetic
- RFC-0008 (Process): Deterministic AI Execution Boundary

## Related Use Cases

- [Verifiable AI Agents DeFi](../../docs/use-cases/verifiable-ai-agents-defi.md)
- [Verifiable Reasoning Traces](../../docs/use-cases/verifiable-reasoning-traces.md)
- [Probabilistic Verification Markets](../../docs/use-cases/probabilistic-verification-markets.md)
