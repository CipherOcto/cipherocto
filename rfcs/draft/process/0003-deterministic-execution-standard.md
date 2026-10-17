# RFC-0003 (Process/Meta): Deterministic Execution Standard (DES)

## Status

**Version:** 1.1
**Status:** Draft v1.1 (2026-08-22)

> **Note:** This RFC was originally numbered RFC-0003 under the legacy numbering system. It remains at 0003 as it belongs to the Process/Meta category.

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

- RFC-0104/0105/0110/0111 (Numeric): DFP/DQA/BigInt/Decimal (replaces archived RFC-0106 DNT v28)

**Optional:**

- RFC-0109 (Numeric): Deterministic Linear Algebra Engine (replaces legacy numeric alias)
- RFC-0303 (Retrieval): Deterministic Vector Index (replaces legacy numeric alias)
- RFC-0555 (AI Execution): Deterministic Model Execution Engine (replaces legacy numeric alias)

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

All numeric operations **MUST** use the Deterministic Numeric Tower defined in RFC-0104 (DFP) + RFC-0105 (DQA) + RFC-0110 (BigInt) + RFC-0111 (Decimal).

**Allowed numeric types:**

- BigInt (RFC-0110)
- DFP (RFC-0104)
- Decimal (RFC-0111)
- DQA (RFC-0105)

**Disallowed:**

- IEEE 754 native floats
- Platform-dependent math libraries

All rounding **MUST** be:

- round-to-nearest-even

unless explicitly specified.

### 2. Linear Algebra Determinism

All vector/matrix operations **MUST** comply with RFC-0109.

**Constraints:**

- Fixed reduction ordering
- Deterministic accumulation
- Deterministic parallel chunking

### 3. Serialization Determinism

All protocol objects **MUST** use canonical serialization.

**Canonical format:**

- CBOR deterministic mode
- OR DCS (Deterministic Canonical Serialization, RFC-0126)

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

AI model execution **MUST** follow RFC-0555.

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

Defined in RFC-0303.

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

## 2-Cycle Atomic Promotion Cross-Reference

Per BLUEPRINT rule 5 (amendment 2026-08-20; reference at §RFC Process) and BLUEPRINT §Mission Lifecycle 2-Cycle Atomic Promotion gate (amendment 2026-08-22), this RFC is the canonical cross-reference target for all 2-Cycle sibling RFC pairings. The cross-reference is procedure-only — no determinism properties change — but it formalizes the gate semantics for storage-substrate and process-meta RFC pairs.

Sibling pairs invoking this gate (canonical list maintained at BLUEPRINT §Mission Lifecycle 2-Cycle Atomic Promotion gate):

- RFC-0205 + RFC-0206 (storage substrate redesign cascade; Tier 3 promotion sequence in research §20 decision #9)

Future sibling pairs MUST register here as a VH row under the relevant vN.N entry.

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

- [ ] Verify RFC-0104/0105/0110/0111 compliance
- [ ] Verify RFC-0109 compliance
- [ ] Verify RFC-0303 compliance

### Phase 3: Enforcement

- [ ] Add determinism checks to consensus
- [ ] Reject non-compliant nodes
- [ ] Publish compliance certification

## Future Work

- F1: Determinism certification process
- F2: Cross-chain determinism verification
- F3: Formal verification of determinism proofs

## Version History

| Version | Date       | Changes                                                                                                                                                                                                       |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.1     | 2026-08-22 | R20 lens 1 fix: phantom dep cleanup (legacy numeric RFC range → canonical numeric range per R20 phantom dep closure: BigInt/DFP/Decimal/DQA + Linear Algebra + Vector Index + AI Execution modules); vocabulary drift fix (DInt/DFloat/DDecimal/DQuant → BigInt/DFP/Decimal/DQA; canonical protobuf → DCS) |
| 1.1     | 2026-08-22 | R37 follow-up: add 2-Cycle Atomic Promotion Cross-Reference §2-Cycle section per research §20 decision #3 (Path B gate prerequisite for RFC-0008 promotion pathway formalization). Body-only — no determinism property changes. |
| 1.0     | 2026-03-10 | Initial draft                                                                                                                                                                                                 |

## Related RFCs

- RFC-0008 (Process/Meta): Deterministic AI Execution Boundary
- RFC-0104/0105/0110/0111 (Numeric): DFP/DQA/BigInt/Decimal
- RFC-0109 (Numeric): Deterministic Linear Algebra Engine
- RFC-0303 (Retrieval): Deterministic Vector Index
- RFC-0555 (AI Execution): Deterministic Model Execution Engine

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
