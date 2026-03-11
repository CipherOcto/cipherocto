# RFC-0003: Deterministic Execution Standard (DES)

## Status

**Version:** 1.0
**Status:** Draft

## Authors

- Author: @CipherOcto

## Maintainers

- Maintainer: @CipherOcto

## Summary

This RFC defines the global determinism requirements for the CipherOcto protocol. All components that influence consensus, proofs, verification, storage, or query execution MUST comply with deterministic execution rules defined in this specification.

This RFC ensures that:

- Identical inputs produce identical outputs across nodes
- Cross-language implementations remain consistent
- Cryptographic verification remains stable
- Distributed AI execution is reproducible

## Dependencies

**Requires:**

- RFC-0106: Deterministic Numeric Tower (numeric types)

**Optional:**

- RFC-0148: Deterministic Linear Algebra Engine
- RFC-0149: Deterministic Vector Index
- RFC-0155: Deterministic Model Execution Engine

## Motivation

Distributed systems fail when execution diverges.

Sources of nondeterminism include:

- Floating-point behavior
- Parallel execution ordering
- Undefined hashing/serialization
- Platform-dependent math
- Random number generation
- Inconsistent rounding rules
- Thread scheduling

Since CipherOcto relies on:

- Verifiable AI
- Deterministic vector search
- Proof-of-inference
- Distributed verification

Determinism is a foundational protocol invariant.

## Scope

This RFC governs determinism across:

- Numeric computation
- Linear algebra
- Vector indexing
- Retrieval pipelines
- AI execution
- Agent runtime
- Proof verification
- Consensus execution

## Deterministic Rules

### 1. Numeric Determinism

All numeric operations **MUST** use the Deterministic Numeric Tower defined in RFC-0106.

**Allowed numeric types:**

- DInt
- DFloat
- DDecimal
- DQuant

**Disallowed:**

- IEEE 754 native floats
- Platform-dependent math libraries

All rounding **MUST** be:

- round-to-nearest-even

unless explicitly specified.

### 2. Linear Algebra Determinism

All vector/matrix operations **MUST** comply with RFC-0148.

**Constraints:**

- Fixed reduction ordering
- Deterministic accumulation
- Deterministic parallel chunking

### 3. Serialization Determinism

All protocol objects **MUST** use canonical serialization.

**Canonical format:**

- CBOR deterministic mode
- OR canonical protobuf

**Rules:**

- Map keys sorted
- No duplicate fields
- No NaN representations
- Normalized numeric encoding

### 4. Hashing

All hashes **MUST** use deterministic algorithms.

**Allowed:**

- SHA-256
- BLAKE3
- Poseidon (for circuits)

**Prohibited:**

- Platform hash
- Language default hash

### 5. Randomness

Randomness **MUST** be derived from deterministic seeds.

**Allowed:**

- VRF(seed, context)
- ChaCha20(seed)

**Seed sources:**

- Block hash
- Transaction hash
- Proof seed

**Prohibited:**

- System RNG
- Clock-based seeds

### 6. Parallel Execution

Parallel operations **MUST** produce identical results independent of:

- Thread count
- Scheduling
- Hardware

**Allowed techniques:**

- Deterministic reduction trees
- Stable sorting
- Chunk hashing

### 7. Floating-Point Restrictions

Native floating-point operations **MUST NOT** influence:

- Consensus
- Verification
- Proof generation
- State transitions

### 8. AI Model Execution

AI model execution **MUST** follow RFC-0155.

**Requirements:**

- Deterministic kernels
- Deterministic attention
- Deterministic layer normalization
- Fixed precision arithmetic

### 9. Vector Search

Approximate search **MUST** produce deterministic results.

**Permitted approach:**

- Deterministic HNSW traversal
- Fixed random seeds
- Fixed candidate ordering

Defined in RFC-0149.

### 10. Deterministic Time

Protocol logic **MUST NOT** depend on wall-clock time.

**Allowed:**

- Block height
- Logical timestamp

## Verification Requirements

All implementations **MUST** pass a determinism test suite.

Test suite includes:

- Numeric test vectors
- Vector search reproducibility
- Model inference determinism
- Serialization roundtrip

## Compliance

Nodes failing determinism tests:

- **MUST** be rejected by consensus

## Security Considerations

| Threat                 | Impact   | Mitigation              |
| ---------------------- | -------- | ----------------------- |
| Determinism violation  | Critical | Mandatory test suite    |
| Platform divergence    | High     | Cross-platform testing  |
| Floating-point leakage | Critical | DQA types only          |
| Hash instability       | Critical | Allowed algorithms only |

## Determinism Requirements

All RFCs that affect consensus, proofs, verification, storage, or query execution **MUST** comply with this standard. Implementations **MUST** document how they ensure deterministic behavior.

## Test Vectors

| Category      | Test               | Expected Behavior          |
| ------------- | ------------------ | -------------------------- |
| Numeric       | DQA addition       | Identical across platforms |
| Numeric       | DQA multiplication | Identical rounding         |
| Vector        | L2Squared          | Identical distance         |
| Serialization | CBOR roundtrip     | Byte-identical             |
| Hash          | SHA-256            | Deterministic output       |

## Compatibility

This standard **MUST** be backward compatible. Any breaking changes to determinism requirements require a new RFC.

## Alternatives Considered

| Approach            | Pros              | Cons                 |
| ------------------- | ----------------- | -------------------- |
| Relaxed determinism | Performance       | Consensus risk       |
| Platform-specific   | Fast              | Non-reproducible     |
| This spec           | Verifiable + safe | Performance overhead |

## Rationale

Determinism is foundational to CipherOcto's value proposition. Without it:

- Proof verification fails
- Consensus breaks
- AI execution becomes unreproducible

This standard ensures all nodes produce identical results for identical inputs.

## Implementation Phases

### Phase 1: Foundation

- [ ] Define test vectors for numeric types
- [ ] Document serialization format
- [ ] Create compliance test suite

### Phase 2: Integration

- [ ] Verify RFC-0106 compliance
- [ ] Verify RFC-0148 compliance
- [ ] Verify RFC-0149 compliance

### Phase 3: Enforcement

- [ ] Add determinism checks to consensus
- [ ] Reject non-compliant nodes
- [ ] Publish compliance certification

## Future Work

- F1: Determinism certification process
- F2: Cross-chain determinism verification
- F3: Formal verification of determinism proofs

## Version History

| Version | Date       | Changes       |
| ------- | ---------- | ------------- |
| 1.0     | 2026-03-10 | Initial draft |

## Related RFCs

- RFC-0106: Deterministic Numeric Tower
- RFC-0148: Deterministic Linear Algebra Engine
- RFC-0149: Deterministic Vector Index
- RFC-0155: Deterministic Model Execution Engine

## Related Use Cases

- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Verifiable Agent Memory Layer](../../docs/use-cases/verifiable-agent-memory-layer.md)

## Appendices

### A. Allowed Algorithm Specifications

#### SHA-256

- Output: 32 bytes
- Input: arbitrary

#### BLAKE3

- Output: 32 bytes (for digests)
- Input: arbitrary

#### Poseidon

- Field: BN254 or BLS12-381
- Input: field elements
- Output: field element

### B. Canonical CBOR Rules

1. Map keys **MUST** be sorted by byte comparison
2. Integers **MUST** use shortest encoding
3. Strings **MUST** be UTF-8
4. Float values **MUST NOT** be used (use integers or decimals)
5. NaN and Infinity **MUST NOT** be encoded
