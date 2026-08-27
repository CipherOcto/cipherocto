---
title: "RFC-0854: Deterministic Proof Substrate (DPS)"
status: Draft
version: 1.1.0
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
    type Witness;

    /// Generate a proof given witness data, trace commitment, and public inputs.
    /// witness: computation trace, intermediate values, randomness seed
    /// trace_commitment: Merkle root of the computation trace
    /// public_inputs: inputs visible to verifier
    fn prove(
        witness: &Self::Witness,
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

    /// Return the circuit model for this proof system
    fn circuit_model() -> ProofCircuitModel;
}
```

**ProofError Enum:**

```rust
enum ProofError {
    /// Witness data is invalid or incomplete
    InvalidWitness { reason: &'static str },
    /// Trace commitment does not match witness
    TraceMismatch { expected: [u8; 32], computed: [u8; 32] },
    /// Proof generation failed (backend-specific)
    ProofGenerationFailed { backend: &'static str, detail: &'static str },
    /// Verification failed — proof is invalid
    VerificationFailed,
    /// Verification key is invalid
    InvalidVerificationKey,
    /// Unsupported proof system
    UnsupportedProofSystem { suite_id: ProofSuiteId },
    /// Consensus boundary violation
    ConsensusBoundaryViolation { operation: &'static str },
}
```

### 2. Proof Execution Models

```rust
/// RFC-0008 execution class mapping for proof operations
enum ProofExecutionClass {
    /// Class A: Protocol Deterministic — consensus-critical
    /// Proof verification MUST be deterministic across all implementations
    ClassA,
    /// Class B: Deterministic Off-Chain — deterministic but not consensus-ordered
    /// Proof generation with deterministic witness (DQA/DFP inputs)
    ClassB,
    /// Class C: Probabilistic — non-deterministic
    /// Proof generation with OS randomness, hardware acceleration
    ClassC,
}

/// Backend execution model (circuit type)
enum ProofCircuitModel {
    AIR,        // Algebraic Intermediate Representation (STARKs)
    R1CS,       // Rank-1 Constraint Systems (SNARKs)
    PLONKISH,   // PLONK-style circuits
    zkVM,       // Zero-knowledge virtual machine
    Recursive,  // Recursive composition
}

/// RFC-0008 execution class mapping for DPS operations
/// | DPS Operation                | Class | Rationale |
/// |------------------------------|-------|-----------|
/// | Proof verification           | A     | Consensus-critical — must be identical |
/// | Proof commitment computation | A     | Consensus-critical — Merkle inclusion |
/// | Public input canonicalization| A     | Consensus-critical — serialization |
/// | Proof generation (DQA witness)| B    | Deterministic witness, off-chain |
/// | Proof generation (OS random) | C     | Non-deterministic randomness |
/// | Witness generation (DQA/DFP) | B     | Deterministic numeric computation |
/// | Backend selection            | B     | Mission-configured, not consensus-ordered |
/// | Recursive aggregation        | B     | Deterministic given child proofs |
/// | Proof serialization          | A     | Must be canonical (RFC-0126) |
```

### 3. Proof Suite Identification

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
struct ProofSuiteId {
    proof_system: u16,      // STARK=1, PLONK=2, Halo2=3, RISC0=4, SP1=5
    field_id: u16,          // Field identifier (see registry below)
    hash_id: u16,           // Hash function used in proof (see CryptoSuiteId)
    recursion_scheme: u16,  // Recursion strategy (see registry below)
}
```

**Proof System Registry (aligned with RFC-0859 ProofSystemId):**

| ID | Backend | Notes |
|----|---------|-------|
| 0x0001 | STWO (STARK) | Transparent, no trusted setup, Cairo traces |
| 0x0002 | RiscZero | RISC-V zkVM, STARK-based |
| 0x0003 | SP1 | RISC-V zkVM, PLONK-based |
| 0x0004 | Winterfell | STARK backend, Rust-native |
| 0x0005 | Halo2 | No trusted setup, recursive composition |
| 0x0006 | Groth16 | Succinct SNARK, trusted setup |
| 0x0007 | PLONK | Succinct proofs, universal setup |
| 0x0008 | Cairo | STARK-based, Cairo traces |

**Recursion Scheme Registry:**

| ID | Scheme | Notes |
|----|--------|-------|
| 0x0000 | None | No recursion |
| 0x0001 | Binary tree | RFC-0650 binary aggregation |
| 0x0002 | Accumulation | Halo2 accumulation scheme |
| 0x0003 | Folding | Nova/Nova-style folding |

**Field ID Registry:**

| ID | Field | Notes |
|----|-------|-------|
| 0x0001 | BN254 | Ethereum-compatible |
| 0x0002 | BLS12-381 | Standard pairing curve |
| 0x0003 | Goldilocks | STWO-native, 64-bit |

**Backend Registration:** New backends are registered by assigning a new `proof_system` ID (0x0009-0xFFFF). Implementations MUST support at least STARK (0x0001). Other backends are optional per mission configuration.

### 4. Proof-Carrying Envelopes (RFC-0859 integration)

```rust
struct ProofCarryingEnvelope {
    envelope: DeterministicEnvelope,    // RFC-0850
    proof_system_id: u16,               // ProofSystemId enum
    proof_commitment: [u8; 32],         // BLAKE3-256(proof_blob)
    public_input_root: [u8; 32],        // Merkle root of public inputs
    proof_blob: Vec<u8>,                // Serialized proof (RFC-0126)
    execution_model: u16,               // ProofCircuitModel enum
    parent_proof_commitment: Option<[u8; 32]>,  // For recursive aggregation (Section 8)
}
```

**Construction algorithm:**
1. Serialize proof via RFC-0126 DCS → `proof_blob`
2. Compute `proof_commitment = BLAKE3-256(proof_blob)`
3. Compute `public_input_root = Merkle root of canonicalized public inputs`
4. Set `execution_model` to the `ProofCircuitModel` variant for this backend
5. For recursive aggregation: set `parent_proof_commitment` to the parent's `proof_commitment`

**Verification flow (RFC-0859):**
1. Look up `proof_system_id` in ProofSystemId registry
2. Load verification key for the identified backend
3. Deserialize `proof_blob` via RFC-0126
4. Call `DeterministicProofSystem::verify(vk, public_inputs, proof)`
5. For recursive proofs: verify parent proof commitment chain

This enables: verifiable AI inference, mission correctness proofs, validator proofs, distributed execution attestations, privacy-preserving coordination.

### 5. Canonical Proof Boundary

**Consensus MUST NEVER depend on:** prover runtime, hardware acceleration, proving time, memory layout, parallel execution order, witness generation order.

**Consensus MAY depend ONLY on:** `(public_inputs, canonical_verifier, proof_bytes, verification_result)`

### 6. Deterministic Witness Model

Small numeric divergence (`0.30000000001` vs `0.29999999998`) can completely invalidates proofs. CipherOcto's deterministic numeric stack (RFC-0104 DFP, RFC-0105 DQA) becomes a ZK-safe arithmetic substrate for witness generation.

**DQA properties and AIR benefits:**

| DQA Property | AIR Benefit |
|-------------|-------------|
| Integer core | Native field arithmetic |
| Fixed scale | Constraint simplification |
| Canonicalization | Stable witness generation |
| Deterministic rounding | Reproducible traces |
| Bounded ranges | Lower proving cost |

**WitnessGenerator trait:**

```rust
trait WitnessGenerator {
    type Input;
    type Witness;

    /// Generate a deterministic witness from DQA/DFP inputs.
    /// MUST use RFC-0105 DQA arithmetic for all numeric operations.
    /// MUST use RFC-0104 DFP for any floating-point conversion.
    /// MUST produce identical witness given identical inputs across all implementations.
    fn generate(
        input: &Self::Input,
        trace_commitment: &[u8; 32],
    ) -> Result<Self::Witness, ProofError>;

    /// Convert floating-point values to field elements deterministically
    fn fp_to_field_element(value: DfpValue, field_prime: &[u8; 32]) -> [u8; 32];
}
```

**DQA/DFP → Witness integration:**

```text
1. Input values are represented as DQA fixed-point integers (RFC-0105)
2. Floating-point inputs are converted via DFP canonicalization (RFC-0104)
3. All arithmetic uses DQA's integer core (no floating-point in witness generation)
4. Field element conversion: DQA integer → mod field_prime (deterministic)
5. Witness is serialized via RFC-0126 DCS before passing to prover
```

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

**Verifier Registry:**

```rust
struct VerifierRegistry {
    entries: BTreeMap<[u8; 32], VerifierEntry>,  // BTreeMap for deterministic iteration
}

struct VerifierEntry {
    proof_suite: ProofSuiteId,
    verification_key: Vec<u8>,
    registered_at: u64,
    expires_at: Option<u64>,
}
```

**Mission Proof Requirement:**

```rust
struct MissionProofRequirement {
    mission_id: [u8; 32],
    required_backend: u16,       // ProofSystemId
    fallback_backends: Vec<u16>, // Ordered fallback list
}
```

**Backend Capability Advertisement:** Nodes advertise supported backends via GDP capability bitmask (RFC-0851 §5). Bit positions: STWO=0x0100, RiscZero=0x0200, SP1=0x0400, Winterfell=0x0800, Halo2=0x1000, Groth16=0x2000, PLONK=0x4000, Cairo=0x8000.

**CryptoSuiteId Reference:** The `hash_id` field in `ProofSuiteId` uses values from `CryptoSuiteId` defined in RFC-0853 Section 3 (e.g., 0x0001=SHA-256, 0x0002=BLAKE3-256).

### 8. Verification Key Management

**VK Generation:** Each backend generates VKs during setup. STARK backends derive VKs transparently from circuit description. SNARK/PLONK backends require trusted setup ceremony.

**VK Distribution:** VKs are distributed via GDP (RFC-0851) as part of gateway capability advertisement. VKs are committed in Merkle trees under `capabilities_root`.

**VK Rotation:** VKs rotate when backend parameters change. Rotation uses dual-verification period (old + new VK accepted) with configurable transition window. VK revocation uses DGP gossip (RFC-0852) when available.

**VK Relationship to OCrypt:** VKs are protected by OCrypt (RFC-0853) key hierarchy. VK signing keys derive from the mission root key via `HKDF-BLAKE3(mission_root_key, "dps:vk:sign:v1", mission_id, 32)`.

### 9. Recursive Aggregation

DPS integrates with RFC-0650 (Proof Aggregation Protocol) for recursive proof compression.

**RFC-0650 Actor Model:**

| Actor | Role | Token |
|-------|------|-------|
| Worker | Produces individual proofs | OCTO-A (compute) |
| Collector | Gathers proofs from workers | OCTO-B (bandwidth) |
| Aggregator | Builds recursive aggregation tree | OCTO-A (compute) |
| Verifier | Validates aggregated proofs | OCTO-N (node ops) |

**Aggregation Tree (Binary Recursion per RFC-0650):**

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

**Double-aggregation resolution (RFC-0650 first-seen-wins):**

If two aggregators produce competing aggregated proofs for the same set of children, the first proof seen by the network wins. This is deterministic because all nodes process the same canonical ordering of proofs.

**RFC-0630 (Proof-of-Inference) integration:**

DPS generalizes RFC-0630's proof model. RFC-0630 defines Proof Structure as `(model_id, input_hash, output_hash, stark_proof)` — this maps to DPS's `DeterministicProofSystem` trait with `PublicInputs = (model_id, input_hash, output_hash)` and `Proof = stark_proof`. RFC-0630's verification modes (full, sampling, optimistic) are mission-scoped policies configured per `ProofSuiteId`.

### 10. Integration with OCrypt

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
| Proof replay | High | Epoch + nonce in proof, replay cache per mission |
| Malformed proof DoS | High | Size limits, sandboxed verification, timeout |
| VK compromise | Critical | Rotation via Mission 0853a, dual-verification window |
| Proof malleability | Medium | Proof commitment = BLAKE3-256(canonical_proof_bytes) |

## Implementation Phases

### Phase 1: Core Abstraction (Months 1-3)
- DeterministicProofSystem trait
- ProofSuiteId
- ProofExecutionClass and ProofCircuitModel enums
- STARK backend integration (STWO or RiscZero)

### Phase 2: Witness Generation and Verifier Registry (Months 3-6)
- WitnessGenerator trait implementation
- DQA/DFP witness integration (RFC-0104/0105)
- Mission-scoped verifier registry
- ProofError handling
- Backend registration mechanism

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

## Compatibility

### RFC-0843 Integration

DPS extends RFC-0843's consensus layer with proof-carrying capabilities:

- RFC-0843 provides block production and consensus primitives
- DPS adds verifiable proof attachment to consensus artifacts
- Proof verification integrates with RFC-0843's block validation pipeline

### Backend Interoperability

DPS supports multiple proof system backends through the `DeterministicProofSystem` trait:

- STARK (STWO/RISC0) — transparent, no trusted setup, post-quantum
- PLONK — succinct proofs, universal setup
- Halo2 — no trusted setup, recursive composition
- zkVM — general-purpose verifiable computation

Backend selection is per-mission, allowing different privacy/cost tradeoffs.

### Forward Compatibility

- `ProofSuiteId` is extensible (new backends without protocol changes)
- Proof blobs are opaque to consensus (only verification result matters)
- `ProofCircuitModel` enum supports future circuit types
- Backend migration is possible via dual-verification during transition

## Test Vectors

### Proof Generation (STARK Backend)

```text
Input:
  trace_commitment = BLAKE3-256("test_execution_trace")
  public_inputs = { mission_id: [0x01; 32], result_hash: BLAKE3-256("result") }
  proof_suite = ProofSuiteId { proof_system: 1 (STARK), field_id: 1, hash_id: 1, recursion_scheme: 0 }

Expected:
  proof_blob = [valid STARK proof bytes]
  proof_commitment = BLAKE3-256(proof_blob)
  verification_result = true when verified with matching vk
```

### Proof Verification (Deterministic)

```text
Input:
  vk = [verification key for STARK suite]
  public_inputs = { mission_id: [0x01; 32], result_hash: BLAKE3-256("result") }
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

### Error Path Test Vectors

```text
Input:
  proof_blob = [0x00; 10]  // malformed: too short for any backend

Expected:
  ProofError::ProofGenerationFailed { backend: "any", detail: "proof_blob too short" }
```

```text
Input:
  proof_system_id = 0xFFFF  // unknown proof system

Expected:
  ProofError::UnsupportedProofSystem { suite_id: ProofSuiteId { proof_system: 0xFFFF, ... } }
```

```text
Input:
  public_inputs = {}  // empty

Expected:
  ProofError::InvalidWitness { reason: "public_inputs cannot be empty" }
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
| STWO (STARK) | 1.0x | ~50KB | ~5ms |
| RiscZero | 1.5x | ~100KB | ~10ms |
| SP1 | 2.0x | ~50KB | ~8ms |
| Winterfell | 1.2x | ~60KB | ~6ms |
| Halo2 | 3.0x | ~1KB | ~3ms |
| Groth16 | 4.0x | ~0.5KB | ~2ms |
| PLONK | 2.5x | ~1KB | ~3ms |
| Cairo | 1.0x | ~50KB | ~5ms |

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

## Forward Compatibility

- **RFC-0850 (DOT):** DPS proof commitments are embedded in DOT envelopes via ProofCarryingEnvelope
- **RFC-0853 (OCrypt):** DPS uses OCrypt primitives (BLAKE3-256, Ed25519) for proof signatures
- **RFC-0126 (DCS):** Proof serialization uses Deterministic Canonical Serialization. RFC-0126 status: Accepted. Fallback: if RFC-0126 is not yet implemented, use big-endian byte serialization with length-prefixed fields.
- **RFC-0104/RFC-0105 (DFP/DQA):** Witness generation uses deterministic numeric arithmetic
- **RFC-0630 (Proof-of-Inference):** DPS generalizes PoI's proof model to arbitrary proof systems
- **RFC-0650 (Proof Aggregation):** DPS integrates with recursive aggregation protocol
- **Forward compatibility:** ProofSuiteId is extensible (0x0009-0xFFFF for future proof systems)

## Future Work

- F1: GPU-accelerated proof generation with STWO SIMD
- F2: Formal verification of DeterministicProofSystem trait properties (determinism, completeness, soundness)
- F3: Proof market integration for decentralized proving (supply/demand pricing)
- F4: Cross-chain proof verification bridges (Ethereum, Cosmos, Solana)
- F5: Hardware accelerator support (FPGA, ASIC) for proving
- F6: Proof composition DSL for mission-specific proof pipelines
- F7: ZKML integration via Cairo/STWO for verifiable AI inference
- F8: Post-quantum proof system migration (Lattice-based, STARK-Lattice hybrids)

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/octo-network/src/dps/mod.rs` | DPS module root |
| `crates/octo-network/src/dps/trait.rs` | DeterministicProofSystem |
| `crates/octo-network/src/dps/suite.rs` | ProofSuiteId |
| `crates/octo-network/src/dps/envelope.rs` | ProofCarryingEnvelope |
| `crates/octo-network/src/dps/recursive.rs` | Recursive aggregation |
| `crates/octo-network/src/dps/verifier.rs` | Verifier registry |
| `crates/octo-network/src/dps/witness.rs` | Deterministic witness |
| `crates/octo-network/src/dps/backends/stark.rs` | STARK backend |
| `crates/octo-network/src/dps/backends/plonk.rs` | PLONK backend |
| `crates/octo-network/src/dps/backends/halo2.rs` | Halo2 backend |

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-25 | Initial draft |
| 1.1.0 | 2026-05-27 | Round 1 adversarial review — 24 fixes (3C, 8H, 8M, 5L) |

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
