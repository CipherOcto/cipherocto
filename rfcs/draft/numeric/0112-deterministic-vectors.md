# RFC-0112 (Numeric/Math): Deterministic Vectors (DVEC)

## Status

**Version:** 1.2 (2026-03-17)
**Status:** Draft

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

> **Adversarial Review v1.2 Changes:**
> - ISSUE-1.1: Gas budget - NORMALIZE FORBIDDEN in consensus (exceeds 50k block budget)
> - ISSUE-1.2: SQRT gas corrected to 280 per RFC-0111 (not 50,000)
> - ISSUE-1.3: DOT_PRODUCT now generic `<T: NumericScalar>` not hardcoded to Dqa
> - ISSUE-1.4: Explicit scale TRAP when result_scale > MAX_SCALE
> - ISSUE-1.5: Probe serialization format defined (len + 24-byte elements)
> - ISSUE-1.6: Probe entry count fixed to 32 (power of 2)
> - ISSUE-1.7: BigInt conversion uses RFC-0110 I128_ROUNDTRIP semantics

## Summary

This RFC defines Deterministic Vector (DVEC) operations for consensus-critical vector arithmetic used in similarity search and AI inference.

## Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | DVEC<DFP> is FORBIDDEN (not ZK-friendly) |
| RFC-0105 (DQA) | DVEC<DQA> is the primary type (recommended) |
| RFC-0111 (DECIMAL) | DVEC<DECIMAL> is allowed; required for SQRT ops |
| RFC-0113 (DMAT) | DVEC operations compose with matrix ops |

## Dependencies

- **RFC-0111 (DECIMAL)** is REQUIRED for SQRT operations in NORM/NORMALIZE
- RFC-0105 (DQA) does NOT support SQRT operation (DQA limitation)

## Type System

```rust
/// Maximum scale values per type
pub trait MaxScale {
    const MAX_SCALE: u8;
}

impl MaxScale for Dqa {
    const MAX_SCALE: u8 = 18;
}

impl MaxScale for Decimal {
    const MAX_SCALE: u8 = 36;
}

/// Trait for deterministic numeric scalar types
pub trait NumericScalar: Clone {
    fn scale(&self) -> u8;
    fn mul(self, other: Self) -> Result<Self, Error>;
    fn add(self, other: Self) -> Result<Self, Error>;
    fn sub(self, other: Self) -> Result<Self, Error>;
    fn div(self, other: Self) -> Result<Self, Error>;
    /// sqrt returns Err(Unsupported) for Dqa (no SQRT in RFC-0105)
    fn sqrt(self) -> Result<Self, Error>;
    fn is_zero(&self) -> bool;
}

/// Deterministic Vector
pub struct DVec<T: NumericScalar> {
    pub data: Vec<T>,
    pub len: usize,
}
```

### Mixed-Type Operations

> **FORBIDDEN**: Operations between DVEC<DQA> and DVEC<DECIMAL> are NOT permitted. All elements in a vector must be of the same type.

## Production Limitations

| Feature | Limit | Status |
|---------|-------|--------|
| DVEC<DQA> | N ≤ 64 | ALLOWED |
| DVEC<DFP> | DISABLED | FORBIDDEN |
| DVEC<DECIMAL> | N ≤ 64 | ALLOWED |

## Core Operations

### DOT_PRODUCT — Dot Product

```
fn dot_product<T: NumericScalar + MaxScale>(a: &[T], b: &[T]) -> Result<T, Error>

Preconditions:
  - a.len == b.len
  - a.len <= MAX_DVEC_DIM (64)
  - All elements use same scale

Algorithm:
  1. accumulator = BigInt(0)

  2. For i in 0..a.len (sequential order, i=0 then 1 then 2...):
       // Multiply elements (they have same scale)
       product = BigInt::from(a[i].value()) * BigInt::from(b[i].value())
       accumulator = accumulator + product  // BigInt addition

  3. Scale: result_scale = a[0].scale() + b[0].scale()  // Per RFC-0105 MUL semantics

  4. If result_scale > T::MAX_SCALE: TRAP (INVALID_SCALE)

  5. Conversion: Per RFC-0110 I128_ROUNDTRIP semantics:
     - If !accumulator.fits_in_i64(): TRAP (OVERFLOW)
     - value = accumulator as i64

  6. Return T::new(value, result_scale)
```

> ⚠️ **CRITICAL**: Sequential iteration is MANDATORY. Tree reduction `(a1+a2)+(a3+a4)` produces different results than sequential `(((a1+a2)+a3)+a4)` due to overflow/rounding.
>
> **Overflow TRAP Order**: The accumulator must overflow TRAP before any scale transformation. This ensures deterministic behavior regardless of scale - if the raw sum overflows, it must TRAP even if the scaled result would fit.
>
> **DQA Note**: For Dqa, MAX_SCALE=18. If result_scale > 18, TRAP(INVALID_SCALE).

### SQUARED_DISTANCE — Squared Euclidean Distance

```
fn squared_distance<T: NumericScalar + MaxScale>(a: &[T], b: &[T]) -> Result<T, Error>

Preconditions:
  - a.len == b.len
  - a.len <= MAX_DVEC_DIM (64)
  - All elements use same scale
  - For Dqa: a[0].scale <= 9  // CRITICAL: Enforce to prevent result scale overflow (>18)
  - For Decimal: a[0].scale <= 18  // CRITICAL: Enforce to prevent result scale overflow (>36)

> ⚠️ **ZK-OPTIMIZED**: Prefer this over NORM for similarity ranking. Saves ~6,400 ZK gates.

Algorithm:
  1. input_scale = a[0].scale()

  2. If T is Dqa AND input_scale > 9: TRAP (INPUT_VALIDATION_ERROR)
  3. If T is Decimal AND input_scale > 18: TRAP (INPUT_VALIDATION_ERROR)

  4. accumulator = BigInt(0)

  5. For i in 0..a.len (sequential order):
       diff = BigInt::from(a[i].value()) - BigInt::from(b[i].value())
       product = diff * diff
       accumulator = accumulator + product

  6. Scale: result_scale = input_scale * 2

  7. If result_scale > T::MAX_SCALE: TRAP (INVALID_SCALE)

  8. Conversion: Per RFC-0110 I128_ROUNDTRIP semantics:
     - If !accumulator.fits_in_i64(): TRAP (OVERFLOW)
     - value = accumulator as i64

  9. Return T::new(value, result_scale)
```

### NORM — L2 Norm

```
fn norm<T: NumericScalar + MaxScale>(a: &[T]) -> Result<T, Error>

> ⚠️ **DEPRECATED for consensus**: Use SQUARED_DISTANCE instead. Only use NORM for UI/display purposes.

Preconditions:
  - For Dqa: TRAP (UNSUPPORTED_OPERATION - DQA lacks SQRT per RFC-0105)
  - For Decimal: a[0].scale <= 18 (required for SQRT)

Algorithm:
  1. If T is Dqa: TRAP(UNSUPPORTED_OPERATION)
  2. dot = dot_product(a, a)?
  3. Return dot.sqrt()  // Requires RFC-0111 DECIMAL SQRT

⚠️ **Zero Vector**: If all elements are zero, return zero (not an error).
```

### NORMALIZE — Vector Normalization

```
fn normalize<T: NumericScalar + MaxScale>(a: &[T]) -> Result<Vec<T>, Error>

> ⚠️ **FORBIDDEN IN CONSENSUS**: This operation exceeds the per-block numeric gas budget (50,000).
> Allowed only in Analytics/Off-chain queries.

Preconditions:
  - TRAP(CONSENSUS_RESTRICTION) if executed in deterministic consensus context
  - For Analytics: a[0].scale <= 18

Algorithm:
  1. n = norm(a)?
  2. If n == 0: TRAP (CANNOT_NORMALIZE_ZERO_VECTOR)
  3. For each element:
       result[i] = a[i].div(n)?  // Element-wise division
  4. Return result
```

> **Rationale**: NORMALIZE requires N divisions (N×GAS_DIV ≈ 251,000 for N=64) plus SQRT gas, totaling ~319,000. This exceeds the per-block numeric budget of 50,000 gas defined in RFC-0110/0111. Use SQUARED_DISTANCE for consensus-critical similarity ranking.

### Element-wise Operations

```
// Element-wise ADD
vec_add(a: &[Dqa], b: &[Dqa]) -> Vec<Dqa>
  - TRAP if a.len != b.len
  - Scales must match
  - Result[i] = a[i] + b[i]

// Element-wise SUB
vec_sub(a: &[Dqa], b: &[Dqa]) -> Vec<Dqa>
  - Same as ADD but subtraction

// Element-wise MUL
vec_mul(a: &[Dqa], b: &[Dqa]) -> Vec<Dqa>
  - TRAP if a.len != b.len
  - Result[i] = a[i] * b[i]

// SCALE (multiply all by scalar)
vec_scale(a: &[Dqa], scalar: Dqa) -> Vec<Dqa>
  - Result[i] = a[i] * scalar
```

## Gas Model

| Operation | Gas Formula | Max (N=64, scale=9) |
|-----------|-------------|---------------------|
| DOT_PRODUCT | N × (30 + 3 × scale²) | 17,472 |
| SQUARED_DISTANCE | N × (30 + 3 × scale²) + 10 | 17,482 |
| NORM | DOT_PRODUCT + GAS_SQRT | 17,752 (SQRT=280 per RFC-0111) |
| NORMALIZE | **FORBIDDEN IN CONSENSUS** | TRAP(CONSENSUS_RESTRICTION) |
| VEC_ADD | 5 × N | 320 |
| VEC_SUB | 5 × N | 320 |
| VEC_MUL | 5 × N | 320 |
| VEC_SCALE | 5 × N | 320 |

> **Note:** GAS_SQRT = 280 (max per RFC-0111, formula: `100 + 5 * scale`, max scale 36).
>
> **Consensus Restriction:** NORMALIZE is FORBIDDEN in consensus because it exceeds the 50,000 per-block numeric gas budget. Use SQUARED_DISTANCE for similarity ranking.

## Test Vectors

### DOT_PRODUCT

| Input A | Input B | Expected | Notes |
|---------|---------|----------|-------|
| [1, 2, 3] | [4, 5, 6] | {32, scale=0} | 1×4 + 2×5 + 3×6 |
| [1, 2] (scale=1) | [3, 4] (scale=1) | {11, scale=2} | Scale addition |
| [0, 0, 0] | [1, 2, 3] | {0, scale=0} | Zero vector |
| [MAX, MAX] | [1, 1] | TRAP | Overflow check |

### SQUARED_DISTANCE

| Input A | Input B | Expected | Notes |
|---------|---------|----------|-------|
| [0, 0] | [3, 4] | {25, scale=0} | 3² + 4² |
| [1, 2] | [4, 6] | {29, scale=0} | 3² + 4² |
| [1.5, 2.5] | [1.5, 2.5] | {0, scale=0} | Identical |
| [1.5e10, 2.5e10] | [1.5e10, 2.5e10] | TRAP | scale=10 → result scale=20 > 18 |

### NORM

| Input | Type | Expected | Notes |
|-------|------|----------|-------|
| [3, 4] | Decimal | {5, scale=0} | 3-4-5 triangle |
| [0, 0, 0] | Decimal | {0, scale=0} | Zero vector |
| [1, 1, 1] | Decimal | {1.732..., scale=6} | √3 |
| [3, 4] | Dqa | TRAP | UNSUPPORTED_OPERATION |

### Boundary Cases

| Operation | Input | Expected | Notes |
|-----------|-------|----------|-------|
| DOT_PRODUCT | N=64, max values | TRAP | Overflow check |
| DOT_PRODUCT | N=65 | REJECT | Exceeds limit |
| VEC_ADD | Mismatch lengths | TRAP | Dimension error |
| NORMALIZE | Zero vector | TRAP | Cannot normalize |
| SQUARED_DISTANCE | scale=10 | TRAP | Input scale > 9 |

## Verification Probe

### Probe Entry Serialization Format (Canonical)

Following RFC-0111's rigorous serialization approach:

**DVec Canonical Wire Format:**
```
leaf_input = op_id (8 bytes) || vector_a_len (1 byte) || vector_a_elements... || vector_b_len (1 byte) || vector_b_elements... || result_len (1 byte) || result_elements...
```

Where each scalar element is serialized as 24 bytes (mantissa + scale per RFC-0111):
```
element = mantissa (16 bytes, big-endian i128) || scale (1 byte) || reserved (7 bytes = 0x00)
```

> **Note:** Variable-length vectors require explicit length prefix. N is fixed per probe entry definition.

### Merkle Tree Structure

- **Entry Count:** 32 (fixed power of 2 for deterministic tree structure)
- Each probe entry is a **Merkle tree leaf**: `SHA256(leaf_input)` = 32 bytes
- The Merkle root commits to all 32 entries

```
DVecProbe {
    // DOT_PRODUCT entries (Dqa)
    entry_0:  dot_product([1,2,3], [4,5,6]) → {32, scale=0}
    entry_1:  dot_product([1,2], [3,4]) scale=1 → {11, scale=2}
    entry_2:  dot_product([MAX,MAX], [1,1]) → TRAP(OVERFLOW)
    // ... more DOT_PRODUCT

    // SQUARED_DISTANCE entries (Dqa)
    entry_8:  squared_distance([0,0], [3,4]) → {25, scale=0}
    entry_9:  squared_distance([1,2], [4,6]) → {29, scale=0}
    entry_10: squared_distance scale=10 → TRAP(INVALID_SCALE)
    // ... more SQUARED_DISTANCE

    // NORM entries (Decimal only)
    entry_16: norm([3,4]) Decimal → {5, scale=0}
    entry_17: norm([0,0,0]) Decimal → {0, scale=0}
    entry_18: norm([3,4]) Dqa → TRAP(UNSUPPORTED_OPERATION)
    // ... more NORM

    // Element-wise operations
    entry_24: vec_add [1,2] + [3,4] → [4,6]
    entry_25: vec_sub [4,6] - [1,2] → [3,4]
    entry_26: vec_mul [2,3] * [4,5] → [8,15]
    entry_27: vec_scale [1,2] * 2 → [2,4]

    // TRAP/consensus restriction entries
    entry_28: NORMALIZE in consensus → TRAP(CONSENSUS_RESTRICTION)
    // ... padding entries to reach 32
    entry_31: [reserved]
}
```

### Merkle Root Computation

```
fn dvec_probe_root(probe: &DVecProbe) -> [u8; 32] {
    // Build Merkle tree from 32 leaf hashes
    // Level 0: 32 leaf hashes (SHA256 of each entry's leaf_input)
    // Level 1: 16 parent hashes (SHA256(leaf[i] || leaf[i+1]))
    // Level 2: 8 grandparent hashes
    // Level 3: 4 great-grandparent hashes
    // Level 4: 2 great-great-grandparent hashes
    // Level 5: 1 root hash
    // Return root hash
}
```

### Verification Procedure

1. For each probe entry, serialize inputs using canonical format
2. Execute operation per algorithms in this RFC
3. Serialize result using canonical format
4. Compute leaf hash: SHA256(leaf_input)
5. Build Merkle tree from 32 leaves
6. Verify root matches published value: `[COMPUTED_ROOT]`

> **Note:** The verification probe uses the same Merkle tree structure as RFC-0111 (57 entries there, 32 here) to ensure consistency across the Numeric Tower.

## Determinism Rules

1. **No SIMD**: Sequential loops only
2. **Fixed Iteration Order**: i=0, then 1, then 2...
3. **No Tree Reduction**: Accumulators must be sequential
4. **Overflow Traps**: Must trap on overflow (not wrap)
5. **Scale Matching**: Element scales must match
6. **Type Isolation**: No mixed-type operations (DQA vs Decimal)

## Implementation Checklist

- [ ] DVec struct with data: Vec<T: NumericScalar>
- [ ] DOT_PRODUCT with BigInt accumulator and overflow TRAP
- [ ] SQUARED_DISTANCE with scale constraint (≤9) and overflow TRAP
- [ ] NORM (restricted to Decimal, TRAP for DQA)
- [ ] NORMALIZE (restricted to Decimal, TRAP for DQA)
- [ ] Element-wise ADD/SUB/MUL
- [ ] SCALE operation
- [ ] Dimension limit enforcement (N ≤ 64)
- [ ] Scale matching validation
- [ ] Overflow detection (BigInt accumulator)
- [ ] Gas calculations with corrected formulas
- [ ] Test vectors
- [ ] Verification probe with Merkle tree

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0110: Deterministic BIGINT
- RFC-0111: Deterministic DECIMAL
- RFC-0113: Deterministic Matrices
- RFC-0106: Deterministic Numeric Tower (archived)
