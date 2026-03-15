# RFC-0106 Dismantling — Track B: Modular RFCs Design

**Date:** 2026-03-14
**Status:** Design Approved
**Parent:** docs/plans/2026-03-14-0106-dismantling-track-a.md

## Overview

Track B defines the new modular RFCs (0110-0115) that will replace RFC-0106. Each RFC follows the "Complete Spec" pattern established by RFC-0105.

## Phased Rollout

| Phase | RFCs | Timeline | Focus |
|-------|------|----------|-------|
| Phase 1 | 0110 BIGINT, 0111 DECIMAL | Q2 2026 | Base layer arithmetic |
| Phase 2 | 0112 DVEC | Q2 2026 | Vector operations |
| Phase 3 | 0113 DMAT, 0114 Activation | Q3 2026 | Matrix + ML primitives |
| Phase 4 | 0115 DTENSOR | Q4 2026+ | Tensor (future) |

## RFC-0110: Deterministic BIGINT

### Purpose
Arbitrary-precision integer arithmetic for consensus-critical computations requiring values beyond i64/i128.

### Specification Requirements

**Data Structure:**
```rust
struct BigInt {
    // Little-endian limbs, u64 each
    limbs: Vec<u64>,
    // Sign: true = negative, false = positive
    sign: bool,
}
```

**Canonical Form:**
- No leading zero limbs
- Zero represented as empty limbs + sign = false
- Minimum number of limbs for value

**Required Algorithms:**
| Operation | Algorithm | Gas |
|-----------|-----------|-----|
| ADD | Limb-wise with carry | 10 |
| SUB | Limb-wise with borrow | 10 |
| MUL | Schoolbook O(n²) or Karatsuba | 50-200 |
| DIV | Binary long division | 100-500 |
| MOD | Remainder from division | 100-500 |
| POW | Fixed iteration (no variable-time) | 200 |
| SQRT | Binary search iteration | 150 |
| CMP | Limb-wise comparison | 5 |

**Test Vectors Required:**
- i64::MIN, i64::MAX boundary
- i128::MIN, i128::MAX boundary
- 4096-bit extremes
- Karatsuba threshold crossing
- Division by powers of 2
- Negative value edge cases

**Verification Probe:**
- 7-entry Merkle-hash probe
- Hard-coded golden values for add/mul/div/sqrt

---

## RFC-0111: Deterministic DECIMAL

### Purpose
Extended-precision decimal arithmetic (i128-based) for high-precision financial calculations.

### Relationship to DQA (RFC-0105)

| Aspect | DQA | DECIMAL |
|--------|-----|---------|
| Internal | i64 | i128 |
| Scale Range | 0-18 | 0-36 |
| Performance | Faster | 1.2-1.5x slower |
| Use Case | Default financial | High-precision risk |

**Note:** DECIMAL uses same scaled-integer representation as DQA, just extended to i128 and 36 scale.

### Specification Requirements

**Data Structure:**
```rust
struct Decimal {
    // Signed 128-bit mantissa
    mantissa: i128,
    // Decimal scale (0-36)
    scale: u8,
}
```

**Canonical Form:**
- Trailing zeros removed from mantissa
- Scale minimized without losing precision
- Zero: mantissa = 0, scale = 0

**Required Algorithms:**
| Operation | Algorithm | Gas |
|-----------|-----------|-----|
| ADD | i128 + scale alignment | 6 |
| SUB | i128 - scale alignment | 6 |
| MUL | i128 × scale add | 12 |
| DIV | i128 ÷ with target scale | 25 |
| SQRT | Newton-Raphson at target scale | 50 |
| ROUND | RoundHalfEven to target scale | 5 |
| CANONICALIZE | Remove trailing zeros | 2 |

**Conversions:**
- DECIMAL → DQA: Round/trap on scale > 18
- DQA → DECIMAL: Zero-extend mantissa
- DECIMAL → BIGINT: TRAP if scale > 0 (precision loss)

**Test Vectors Required:**
- All DQA test vectors (parity)
- i128::MIN, i128::MAX boundaries
- Scale 19-36 edge cases
- High-precision division (1/7, 1/17, etc.)
- Chain operations with scale overflow

**Verification Probe:**
- Extends DQA probe with i128 entries

---

## RFC-0112: Deterministic Vectors (DVEC)

### Purpose
Deterministic vector operations for similarity search and AI inference.

### Type System

```rust
struct DVec<T: Numeric> {
    data: Vec<T>,
}

enum Numeric {
    Dqa(Dqa),
    Dfp(Dfp),
    Decimal(Decimal),
}
```

### Specification Requirements

**Core Operations:**
| Operation | Algorithm | Gas |
|-----------|-----------|-----|
| DOT_PRODUCT | Sequential sum of products | 10 × N |
| NORM_L2 | Sequential sqrt of sum of squares | 15 × N |
| NORMALIZE | norm + div (each element) | 20 × N |
| ADD | Element-wise | 5 × N |
| SUB | Element-wise | 5 × N |
| MUL | Element-wise | 5 × N |
| SCALE | Multiply all by scalar | 5 × N |

**Constraints:**
| Feature | Limit | Status |
|---------|-------|--------|
| DVEC<DQA> | N ≤ 64 | ALLOWED |
| DVEC<DFP> | DISABLED | ZK-unfriendly |
| DVEC<DECIMAL> | N ≤ 64 | ALLOWED |

**Determinism Rules:**
1. All reductions MUST use sequential loop (no SIMD unless byte-identical)
2. Element order MUST be preserved
3. Overflow/underflow follows scalar type rules

**Test Vectors Required:**
- N=1, 2, 4, 8, 16, 32, 64 boundary tests
- Zero vectors
- Mixed positive/negative
- Overflow at N=64

**Verification Probe:**
- dot_product(N=32), norm(N=32) golden values

---

## RFC-0113: Deterministic Matrices (DMAT)

### Purpose
Deterministic matrix operations for linear algebra.

### Type System

```rust
struct DMat<T: Numeric> {
    rows: usize,
    cols: usize,
    data: Vec<T>,  // Row-major
}
```

### Specification Requirements

**Core Operations:**
| Operation | Algorithm | Gas |
|-----------|-----------|-----|
| ADD | Element-wise | 5 × M × N |
| SUB | Element-wise | 5 × M × N |
| MUL | Naive triple loop | 20 × M × N × K |
| TRANSPOSE | Data reorder | 2 × M × N |
| DOT_PRODUCT | Row × Column | 10 × M × N × K |

**Constraints:**
| Feature | Limit | Status |
|---------|-------|--------|
| DMAT<DQA> | M×N ≤ 8×8 | EXPERIMENTAL |
| DMAT<DFP> | DISABLED | ZK-unfriendly |
| DMAT<DECIMAL> | M×N ≤ 8×8 | EXPERIMENTAL |

**Determinism Rules:**
1. Matrix multiplication uses naive triple loop (no Strassen, no blocking)
2. Row-major order must be explicit
3. No parallelization in reduction steps

**Test Vectors Required:**
- 2×2, 4×4, 8×8 boundaries
- Identity matrices
- Zero matrices
- Overflow at 8×8

**Verification Probe:**
- 4×4 matmul golden value

---

## RFC-0114: Deterministic Activation Functions

### Purpose
Deterministic neural network activation functions for AI inference.

### Functions

| Function | Implementation | Status | Gas |
|----------|---------------|--------|-----|
| ReLU | Exact: max(0, x) | STABLE | 2 |
| Sigmoid | LUT (256 entries) | EXPERIMENTAL | 10 |
| Tanh | LUT (256 entries) | EXPERIMENTAL | 10 |
| LeakyReLU | Exact with alpha | EXPERIMENTAL | 3 |

### LUT Specification

**Sigmoid LUT:**
- Domain: [-8, 8]
- Entries: 256 (0xFF + 1)
- Interpolation: Linear (no cubic)
- Rounding: RNE to output precision

**Tanh LUT:**
- Domain: [-8, 8]
- Entries: 256
- Interpolation: Linear
- Rounding: RNE to output precision

**Determinism Rules:**
1. LUT index MUST use integer arithmetic (DQA-based)
2. LUT values MUST be committed via SHA-256
3. No interpolation beyond linear
4. Out-of-domain inputs TRAP

### SHA-256 Commitment

```rust
struct ActivationCommitment {
    sigmoid_sha256: [u8; 32],
    tanh_sha256: [u8; 32],
    version: u8,
}
```

### Specification Requirements

**ReLU:**
```rust
fn relu(x: Dqa) -> Dqa {
    if x.value < 0 { Dqa { value: 0, scale: x.scale } }
    else { x }
}
```

**Sigmoid LUT Generation:**
```rust
// Domain: [-8, 8]
// Output: [0, 1] at specified scale
// Index: DQA-based (no floating-point)
fn sigmoid_index(x: Dqa, scale: u8) -> u16 {
    // x_scaled = x * 256 / 8
    // index = (x_scaled + 2048) / 16
}
```

**Test Vectors Required:**
- ReLU: -1, 0, 1, MAX, MIN
- Sigmoid: -8, -4, -1, 0, 1, 4, 8, domain edges
- Tanh: Same as sigmoid
- Chained: ReLU(Sigmoid(x))

---

## RFC-0115: Deterministic Tensors (DTENSOR)

### Purpose
Deterministic tensor operations for batch AI inference.

### Status
FUTURE — Phase 4 (Q4 2026+)

### Conceptual Scope

```rust
struct DTensor<T: Numeric> {
    shape: Vec<usize>,  // N-dimensional
    data: Vec<T>,
}
```

### Defer to Future

DTENSOR requires:
- Batch dimension handling
- Broadcasting rules
- Memory layout (NCHW vs NHWC)
- Convolution specifications

These are deferred until Phase 4 when real AI inference requirements are better understood.

---

## Common Requirements Across All RFCs

### 1. Verification Probe Structure

Each RFC MUST define:
- 7 hard-coded entries
- SHA-256 of concatenated entries
- Block header pinning (every 100,000 blocks)

```rust
struct VerificationProbe {
    entries: [u8; 24],  // 7 × ~3 bytes each
    merkle_root: [u8; 32],
    block_height: u64,
}
```

### 2. Gas Model

| Category | Formula |
|----------|---------|
| Scalar ops | Fixed (e.g., ADD = 6 gas) |
| Vector ops | O(N) with coefficient |
| Matrix ops | O(M×N×K) with coefficient |
| LUT ops | Fixed lookup + interpolation |

### 3. Test Vector Requirements

| RFC | Minimum Vectors | Edge Cases |
|-----|-----------------|------------|
| 0110 BIGINT | 100 | 4096-bit, i128::MIN/MAX |
| 0111 DECIMAL | 100 | Scale 36, i128 boundaries |
| 0112 DVEC | 50 | N=64, overflow |
| 0113 DMAT | 30 | 8×8 overflow |
| 0114 Activation | 40 | Domain edges |

### 4. Fuzzing Requirements

Each RFC MUST have differential fuzzing:
- 500+ random inputs
- Compare against reference implementation
- Bit-identical output required

---

## Archive Plan

When all 011x RFCs are finalized:
1. Move RFC-0106 to `rfcs/archived/0106-superseded-by-011x.md`
2. Update RFC README with new numbering
3. Update implementation references in `determin/` crate

---

## Summary

Track B defines 6 new modular RFCs following the Complete Spec pattern:

| RFC | Title | Phase | Key Deliverable |
|-----|-------|-------|-----------------|
| 0110 | BIGINT | 1 | Arbitrary-precision i64 limbs |
| 0111 | DECIMAL | 1 | i128, scale 0-36 |
| 0112 | DVEC | 2 | dot_product, norm, N≤64 |
| 0113 | DMAT | 3 | matmul, M×N≤8×8 |
| 0114 | Activation | 3 | ReLU + LUT sigmoid/tanh |
| 0115 | DTENSOR | 4 | Deferred |
