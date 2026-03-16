# RFC-0113 (Numeric/Math): Deterministic Matrices (DMAT)

## Status

**Version:** 1.0 (2026-03-14)
**Status:** Draft

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

## Summary

This RFC defines Deterministic Matrix (DMAT) operations for consensus-critical linear algebra used in AI inference.

## Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | DMAT<DFP> is FORBIDDEN |
| RFC-0105 (DQA) | DMAT<DQA> is the primary type |
| RFC-0112 (DVEC) | Matrix-vector multiplication |
| RFC-0114 (Activation) | Applied after matrix ops |

## Type System

```rust
/// Deterministic Matrix
pub struct DMat<T: Numeric> {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<T>,  // Row-major layout
}

/// Supported element types
pub enum Numeric {
    Dqa(Dqa),      // Recommended
    Decimal(Decimal),
    // Dfp is FORBIDDEN
}
```

### Memory Layout (Row-Major)

```
Index(i, j) = i * cols + j

Example: 2x3 matrix
[ a00, a01, a02 ]
[ a10, a11, a12 ]

Data: [a00, a01, a02, a10, a11, a12]
```

## Production Limitations

| Feature | Limit | Status |
|---------|-------|--------|
| DMAT<DQA> | M×N ≤ 8×8 | EXPERIMENTAL |
| DMAT<DFP> | DISABLED | FORBIDDEN |
| DMAT<DECIMAL> | M×N ≤ 8×8 | EXPERIMENTAL |

> **Note**: DMAT is EXPERIMENTAL in Phase 1. It will be enabled after 6-month burn-in of DVEC operations.

## Core Operations

### MAT_ADD — Matrix Addition

```
mat_add(a: &DMat<Dqa>, b: &DMat<Dqa>) -> DMat<Dqa>

Preconditions:
  - a.rows == b.rows
  - a.cols == b.cols
  - a.rows * a.cols <= MAX_DMAT_ELEMENTS (64)

Algorithm:
  For i in 0..a.rows:
    For j in 0..a.cols:
      result[i][j] = a[i][j] + b[i][j]

  Return result
```

### MAT_SUB — Matrix Subtraction

```
mat_sub(a: &DMat<Dqa>, b: &DMat<Dqa>) -> DMat<Dqa>

Algorithm: Same as ADD, but subtract.
```

### MAT_MUL — Matrix Multiplication

```
mat_mul(a: &DMat<Dqa>, b: &DMat<Dqa>) -> DMat<Dqa>

> ⚠️ **REQUIREMENT**: Naive triple loop algorithm ONLY. No Strassen, no blocking.

Preconditions:
  - a.cols == b.rows  (dimension check)
  - a.rows * b.cols <= MAX_DMAT_ELEMENTS (64)

Algorithm (naive triple loop):
  For i in 0..a.rows:           // Row of result
    For j in 0..b.cols:         // Column of result
      accumulator = i128(0)
      For k in 0..a.cols:       // Dot product of row i, col j
        product = a[i][k].value * b[k][j].value
        accumulator = accumulator + product

      result[i][j] = Dqa { value: accumulator as i64, scale: result_scale }
```

> ⚠️ **CRITICAL**: Sequential loops only. No SIMD, no parallelization.

### MAT_VEC_MUL — Matrix-Vector Multiplication

```
mat_vec_mul(a: &DMat<Dqa>, v: &[Dqa]) -> Vec<Dqa>

Preconditions:
  - a.cols == v.len
  - a.rows <= MAX_DVEC_DIM (64)

Algorithm:
  For i in 0..a.rows:
    accumulator = i128(0)
    For j in 0..a.cols:
      accumulator = accumulator + a[i][j].value * v[j].value
    result[i] = Dqa { value: accumulator as i64, scale: result_scale }
```

### MAT_TRANSPOSE — Matrix Transpose

```
mat_transpose(a: &DMat<Dqa>) -> DMat<Dqa>

Algorithm:
  result.rows = a.cols
  result.cols = a.rows
  result[i][j] = a[j][i]
```

### MAT_SCALE — Matrix Scale

```
mat_scale(a: &DMat<Dqa>, scalar: Dqa) -> DMat<Dqa>

Algorithm:
  For each element:
    result[i][j] = a[i][j] * scalar
```

### DOT_PRODUCT (Row × Column)

```
mat_dot_rows(a: &[Dqa], b: &[Dqa]) -> Dqa

Algorithm: Same as DVEC dot_product.
```

## Gas Model

| Operation | Gas Formula | Example |
|-----------|-------------|---------|
| MAT_ADD | 5 × M × N | 5 × 8 × 8 = 320 |
| MAT_SUB | 5 × M × N | 5 × 8 × 8 = 320 |
| MAT_MUL | 8 × M × N × K + 5 × M × N × (K-1) | 8×4×4×4 + 5×4×4×3 = 752 |
| MAT_VEC_MUL | 10 × rows × cols | 10 × 4 × 4 = 160 |
| MAT_TRANSPOSE | 2 × M × N | 2 × 8 × 8 = 128 |
| MAT_SCALE | 5 × M × N | 5 × 8 × 8 = 320 |

## Test Vectors

### MAT_ADD

| A | B | Expected |
|---|---|----------|
| [[1, 2], [3, 4]] | [[5, 6], [7, 8]] | [[6, 8], [10, 12]] |
| [[1.5, 2.5]] | [[0.5, 0.5]] | [[2.0, 3.0]] |

### MAT_MUL

| A | B | Expected |
|---|---|----------|
| [[1, 0], [0, 1]] × [[2, 3], [4, 5]] | [[2, 3], [4, 5]] | Identity |
| [[1, 2], [3, 4]] × [[5, 6], [7, 8]] | [[19, 22], [43, 50]] | Standard |
| [[1, 2, 3]] × [[1], [2], [3]] | [[14]] | Vector result |

### MAT_VEC_MUL

| Matrix | Vector | Expected |
|--------|--------|----------|
| [[1, 2], [3, 4]] | [1, 1] | [3, 7] |
| [[1, 0, 0], [0, 1, 0]] | [1, 2, 3] | [1, 2] |

### Boundary Cases

| Operation | Input | Expected |
|-----------|-------|----------|
| MAT_MUL | 9×9 matrix | REJECT (>64 elements) |
| MAT_MUL | a.cols != b.rows | REVERT |
| MAT_ADD | Dimension mismatch | REVERT |
| MAT_VEC_MUL | a.cols != v.len | REVERT |

## Verification Probe

```rust
struct DMatProbe {
    /// Entry 0: 2x2 identity × 2x2 = identity
    entry_0: [u8; 32],
    /// Entry 1: [[1,2],[3,4]] × [[5,6],[7,8]] = [[19,22],[43,50]]
    entry_1: [u8; 32],
    /// Entry 2: matrix-vector multiply
    entry_2: [u8; 32],
    /// Entry 3: transpose
    entry_3: [u8; 32],
    /// Entry 4: scale by 2
    entry_4: [u8; 32],
    /// Entry 5: 4x4 multiplication
    entry_5: [u8; 32],
    /// Entry 6: 8x8 boundary
    entry_6: [u8; 32],
}

fn dmat_probe_root(probe: &DMatProbe) -> [u8; 32] {
    sha256(concat!(...))
}
```

## Determinism Rules

1. **Naive Algorithm Only**: No Strassen, no blocking optimization
2. **Sequential Loops**: No SIMD, no parallelization
3. **Row-Major Layout**: Must match specification
4. **Dimension Enforcement**: M×N ≤ 64 for execution
5. **Scale Matching**: All elements must have same scale

## Implementation Checklist

- [ ] DMat struct with rows, cols, data
- [ ] Row-major index calculation
- [ ] MAT_ADD with dimension check
- [ ] MAT_SUB with dimension check
- [ ] MAT_MUL with naive triple loop
- [ ] MAT_VEC_MUL
- [ ] MAT_TRANSPOSE
- [ ] MAT_SCALE
- [ ] Dimension limit enforcement
- [ ] Gas calculations
- [ ] Test vectors
- [ ] Verification probe

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0110: Deterministic BIGINT
- RFC-0111: Deterministic DECIMAL
- RFC-0112: Deterministic Vectors
- RFC-0114: Deterministic Activation Functions
- RFC-0106: Deterministic Numeric Tower (archived)
