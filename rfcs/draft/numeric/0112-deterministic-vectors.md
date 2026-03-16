# RFC-0112 (Numeric/Math): Deterministic Vectors (DVEC)

## Status

**Version:** 1.0 (2026-03-14)
**Status:** Draft

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

## Summary

This RFC defines Deterministic Vector (DVEC) operations for consensus-critical vector arithmetic used in similarity search and AI inference.

## Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | DVEC<DFP> is FORBIDDEN (not ZK-friendly) |
| RFC-0105 (DQA) | DVEC<DQA> is the primary type (recommended) |
| RFC-0111 (DECIMAL) | DVEC<DECIMAL> is allowed |
| RFC-0113 (DMAT) | DVEC operations compose with matrix ops |

## Type System

```rust
/// Deterministic Vector
pub struct DVec<T: Numeric> {
    pub data: Vec<T>,
    pub len: usize,
}

/// Supported element types
pub enum Numeric {
    Dqa(Dqa),      // Recommended
    Decimal(Decimal),
    // Dfp is FORBIDDEN
}
```

## Production Limitations

| Feature | Limit | Status |
|---------|-------|--------|
| DVEC<DQA> | N ≤ 64 | ALLOWED |
| DVEC<DFP> | DISABLED | FORBIDDEN |
| DVEC<DECIMAL> | N ≤ 64 | ALLOWED |

## Core Operations

### DOT_PRODUCT — Dot Product

```
dot_product(a: &[Dqa], b: &[Dqa]) -> Dqa

Preconditions:
  - a.len == b.len
  - a.len <= MAX_DVEC_DIM (64)
  - All elements use same scale

Algorithm:
  1. accumulator = i128(0)

  2. For i in 0..a.len (sequential order):
       // Multiply elements (they have same scale)
       product = a[i].value * b[i].value  // i128 multiplication
       accumulator = accumulator + product

  3. Scale: result_scale = a[0].scale

  4. Return Dqa { value: accumulator as i64, scale: result_scale }
```

> ⚠️ **CRITICAL**: Sequential iteration is MANDATORY. Tree reduction `(a1+a2)+(a3+a4)` produces different results than sequential `(((a1+a2)+a3)+a4)` due to overflow/rounding.

### SQUARED_DISTANCE — Squared Euclidean Distance

```
squared_distance(a: &[Dqa], b: &[Dqa]) -> Dqa

> ⚠️ **ZK-OPTIMIZED**: Prefer this over NORM for similarity ranking. Saves ~6,400 ZK gates.

Algorithm:
  1. accumulator = i128(0)

  2. For i in 0..a.len (sequential order):
       diff = a[i].value - b[i].value  // i128
       product = diff * diff
       accumulator = accumulator + product

  3. Scale: result_scale = a[0].scale * 2

  4. Return Dqa { value: accumulator as i64, scale: result_scale }
```

### NORM — L2 Norm

```
norm(a: &[Dqa]) -> Dqa

> ⚠️ **DEPRECATED for consensus**: Use SQUARED_DISTANCE instead. Only use NORM for UI/display purposes.

Algorithm:
  1. dot = dot_product(a, a)
  2. Return sqrt(dot)  // See SQRT below

⚠️ **Zero Vector**: If all elements are zero, return zero (not an error).
```

### NORMALIZE — Vector Normalization

```
normalize(a: &[Dqa]) -> Vec<Dqa>

Algorithm:
  1. n = norm(a)
  2. If n == 0: TRAP (cannot normalize zero vector)
  3. For each element:
       result[i] = a[i] / n  // Element-wise division
  4. Return result
```

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

| Operation | Gas Formula | Example (N=64) |
|-----------|-------------|----------------|
| DOT_PRODUCT | 10 × N | 640 |
| SQUARED_DISTANCE | 12 × N | 768 |
| NORM | DOT + 480 | 1,120 |
| NORMALIZE | NORM + N × 5 | 1,440 |
| VEC_ADD | 5 × N | 320 |
| VEC_SUB | 5 × N | 320 |
| VEC_MUL | 5 × N | 320 |
| VEC_SCALE | 5 × N | 320 |

## Test Vectors

### DOT_PRODUCT

| Input A | Input B | Expected | Notes |
|---------|---------|----------|-------|
| [1, 2, 3] | [4, 5, 6] | 32 | 1×4 + 2×5 + 3×6 |
| [1.0, 2.0] | [3.0, 4.0] | 11.0 | Scale=1 |
| [0, 0, 0] | [1, 2, 3] | 0 | Zero vector |
| [MAX, MAX] | [1, 1] | Overflow | Should TRAP |

### SQUARED_DISTANCE

| Input A | Input B | Expected |
|---------|---------|----------|
| [0, 0] | [3, 4] | 25 |
| [1, 2] | [4, 6] | 29 |
| [1.5, 2.5] | [1.5, 2.5] | 0 | Identical |

### NORM

| Input | Expected | Notes |
|-------|----------|-------|
| [3, 4] | 5 | 3-4-5 triangle |
| [0, 0, 0] | 0 | Zero vector |
| [1, 1, 1] | √3 | ~1.732 |

### Boundary Cases

| Operation | Input | Expected | Notes |
|-----------|-------|----------|-------|
| DOT_PRODUCT | N=64, max values | TRAP | Overflow check |
| DOT_PRODUCT | N=65 | REJECT | Exceeds limit |
| VEC_ADD | Mismatch lengths | TRAP | Dimension error |
| NORMALIZE | Zero vector | TRAP | Cannot normalize |

## Verification Probe

```rust
struct DVecProbe {
    /// Entry 0: dot_product([1,2,3], [4,5,6]) = 32
    entry_0: [u8; 32],
    /// Entry 1: squared_distance([0,0], [3,4]) = 25
    entry_1: [u8; 32],
    /// Entry 2: norm([3,4,0]) = 5
    entry_2: [u8; 32],
    /// Entry 3: element-wise add
    entry_3: [u8; 32],
    /// Entry 4: element-wise mul
    entry_4: [u8; 32],
    /// Entry 5: vec_scale by 2
    entry_5: [u8; 32],
    /// Entry 6: normalize([3,4])
    entry_6: [u8; 32],
}

fn dvec_probe_root(probe: &DVecProbe) -> [u8; 32] {
    sha256(concat!(...))
}
```

## Determinism Rules

1. **No SIMD**: Sequential loops only
2. **Fixed Iteration Order**: i=0, then 1, then 2...
3. **No Tree Reduction**: Accumulators must be sequential
4. **Overflow Traps**: Must trap on overflow (not wrap)
5. **Scale Matching**: Element scales must match

## Implementation Checklist

- [ ] DVec struct with data: Vec<Dqa>
- [ ] DOT_PRODUCT with i128 accumulator
- [ ] SQUARED_DISTANCE (ZK-optimized)
- [ ] NORM (deprecated, with warning)
- [ ] NORMALIZE
- [ ] Element-wise ADD/SUB/MUL
- [ ] SCALE operation
- [ ] Dimension limit enforcement (N ≤ 64)
- [ ] Scale matching validation
- [ ] Overflow detection
- [ ] Gas calculations
- [ ] Test vectors
- [ ] Verification probe

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0110: Deterministic BIGINT
- RFC-0113: Deterministic Matrices
- RFC-0106: Deterministic Numeric Tower (archived)
