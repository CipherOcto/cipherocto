# RFC-0112 (Numeric/Math): Deterministic Vectors (DVEC)

## Status

**Version:** 1.4 (2026-03-17)
**Status:** Draft

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

> **Adversarial Review v1.4 Changes:**
> - ISSUE-1.1: SQRT replaced with RFC-0111 integer Newton-Raphson (deterministic)
> - ISSUE-1.2: All 57 probe entries now unique (no placeholder duplicates)
> - ISSUE-1.3: RFC text inconsistencies fixed (57 entries throughout)
> - ISSUE-1.4: Canonicalization added to all operations
> - ISSUE-1.5: DOT_PRODUCT input scale precondition added (≤9 for DQA)
> - ISSUE-1.6: New Merkle root computed: `0e292ee6c12126ca071c3565f3fa49439a8375dbde6cc5a4ee082883e62433e9`

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
| DVec<Dqa> | N ≤ 64 | ALLOWED |
| DVec<Decimal> | N ≤ 64 | ALLOWED |
| DVec<Dfp> | Any | FORBIDDEN (not ZK-friendly) |
| Mixed-Type Ops | Any | FORBIDDEN |
| NORMALIZE | Consensus | FORBIDDEN (exceeds 50k gas budget) |

## Core Operations

### DOT_PRODUCT — Dot Product

```
fn dot_product<T: NumericScalar + MaxScale>(a: &[T], b: &[T]) -> Result<T, Error>

Preconditions:
  - a.len == b.len
  - a.len <= MAX_DVEC_DIM (64)
  - All elements use same scale
  - For Dqa: a[0].scale() <= 9 (to ensure result_scale <= 18)
  - For Decimal: a[0].scale() <= 18 (to ensure result_scale <= 36)

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

> ⚠️ **CRITICAL**: Sequential iteration is MANDATORY.
>
> **Deterministic TRAP Location:** While integer addition is mathematically associative, overflow TRAP conditions are order-dependent:
> - Sequential: `((MAX + 1) + 0)` → TRAP at first addition
> - Tree: `(MAX + (1 + 0))` → TRAP at second addition
> To ensure deterministic TRAP location across implementations, sequential left-to-right accumulation is MANDATORY.
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

### Element-wise Operations (Generic)

```
// Element-wise ADD
fn vec_add<T: NumericScalar>(a: &[T], b: &[T]) -> Result<Vec<T>, Error>
  - TRAP if a.len != b.len
  - Scales must match
  - Result[i] = a[i].add(b[i])?

// Element-wise SUB
fn vec_sub<T: NumericScalar>(a: &[T], b: &[T]) -> Result<Vec<T>, Error>
  - Same as ADD but subtraction

// Element-wise MUL
fn vec_mul<T: NumericScalar>(a: &[T], b: &[T]) -> Result<Vec<T>, Error>
  - TRAP if a.len != b.len
  - Result[i] = a[i].mul(b[i])?

// SCALE (multiply all by scalar)
fn vec_scale<T: NumericScalar>(a: &[T], scalar: T) -> Result<Vec<T>, Error>
  - Result[i] = a[i].mul(scalar)?
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
>
> **BigInt Overhead:** DOT_PRODUCT formula `N × (30 + 3 × scale²)` accounts for scalar MUL/ADD. BigInt accumulator overhead (~12 gas per iteration) is absorbed into the base cost (30). For N=64, total BigInt overhead ≈ 768 gas, which is <5% of total cost.

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

> **Note:** Variable-length vectors require explicit length prefix. N is fixed per probe entry definition. All scalars use RFC-0111 24-byte canonical big-endian format (including DQA for probe consistency).

### Merkle Tree Structure (57 Entries)

- **Entry Count:** 57 (matching RFC-0111)
- Each probe entry is a **Merkle tree leaf**: `SHA256(leaf_input)` = 32 bytes
- The Merkle root commits to all 57 entries

**Entry Distribution:**
- Entries 0-15: DOT_PRODUCT DQA (various N, scales)
- Entries 16-31: DOT_PRODUCT Decimal (various N, scales)
- Entries 32-39: SQUARED_DISTANCE (DQA/Decimal)
- Entries 40-47: NORM (Decimal + DQA TRAPs)
- Entries 48-51: Element-wise ADD/SUB/MUL/SCALE
- Entries 52-56: TRAP cases (overflow, scale, dimension)

### Published Merkle Root

> **Merkle Root:** `0e292ee6c12126ca071c3565f3fa49439a8375dbde6cc5a4ee082883e62433e9`

This root was computed from the reference Python implementation in `scripts/compute_dvec_probe_root.py`.

### Probe Entry Details

| Entry | Operation | Type | Input A | Input B | Expected Result |
|-------|-----------|------|---------|---------|-----------------|
| 0 | DOT_PRODUCT | DQA | [1,2,3] | [4,5,6] | {32, scale=0} |
| 1 | DOT_PRODUCT | DQA | [1,2] scale=1 | [3,4] scale=1 | {11, scale=2} |
| 2 | DOT_PRODUCT | DQA | [0,0,0] | [1,2,3] | {0, scale=0} |
| 3 | DOT_PRODUCT | DQA | [10,20] scale=2 | [30,40] scale=2 | {3, scale=2} |
| 4-15 | DOT_PRODUCT | DQA | Various | Various | Various |
| 16-31 | DOT_PRODUCT | Decimal | Various | Various | Various |
| 32 | SQUARED_DISTANCE | DQA | [0,0] | [3,4] | {25, scale=0} |
| 33 | SQUARED_DISTANCE | DQA | [1,2] | [4,6] | {29, scale=0} |
| 34-39 | SQUARED_DISTANCE | DQA | Various | Various | Various |
| 40 | NORM | Decimal | [3,4] | - | {5, scale=0} |
| 41 | NORM | Decimal | [0,0,0] | - | {0, scale=0} |
| 42 | NORM | DQA | [3,4] | - | TRAP (UNSUPPORTED) |
| 43-47 | NORM | Decimal | Various | - | Various |
| 48 | VEC_ADD | DQA | [1,2] | [3,4] | [4,6] |
| 49 | VEC_SUB | DQA | [4,6] | [1,2] | [3,4] |
| 50 | VEC_MUL | DQA | [2,3] | [4,5] | [8,15] |
| 51 | VEC_SCALE | DQA | [1,2] | scalar=2 | [2,4] |
| 52 | DOT_PRODUCT | DQA | N=65 elements | - | TRAP (DIMENSION) |
| 53 | DOT_PRODUCT | DQA | scale=10+10 | - | TRAP (INVALID_SCALE) |
| 54 | DOT_PRODUCT | DQA | max values | - | TRAP (OVERFLOW) |
| 55 | SQUARED_DISTANCE | DQA | scale=10 input | - | TRAP (INPUT_SCALE) |
| 56 | NORM | DQA | [3,4] | - | TRAP (UNSUPPORTED) |

### Merkle Root Computation

```
fn dvec_probe_root(probe: &DVecProbe) -> [u8; 32] {
    // Build Merkle tree from 57 leaf hashes
    // Level 0: 57 leaf hashes (SHA256 of each entry's leaf_input)
    // Level 1: 29 parent hashes (last entry duplicated for odd count)
    // Level 2: 15 grandparent hashes
    // Level 3: 8 great-grandparent hashes
    // Level 4: 4 great-great-grandparent hashes
    // Level 5: 2 great-great-grandparent hashes
    // Level 6: 1 root hash
    // Return root hash
}
```

### Verification Procedure

1. For each probe entry, serialize inputs using canonical format
2. Execute operation per algorithms in this RFC
3. Serialize result using canonical format
4. Compute leaf hash: SHA256(leaf_input)
5. Build Merkle tree from 57 leaves
6. Verify root matches: `0e292ee6c12126ca071c3565f3fa49439a8375dbde6cc5a4ee082883e62433e9`
5. Build Merkle tree from 57 leaves
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
