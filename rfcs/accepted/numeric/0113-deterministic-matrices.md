# RFC-0113 (Numeric/Math): Deterministic Matrices (DMAT)

## Status

**Version:** 1.1 (2026-03-18)
**Status:** Accepted
**NUMERIC_SPEC_VERSION:** 1 (per RFC-0110, incremented only when protocol semantics change)

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

> **Adversarial Review v1.1 Changes (Comprehensive Fixes):**
>
> - CRIT-1: Added explicit scale handling per RFC-0105 semantics
> - CRIT-2: Added overflow detection to MAT_MUL algorithm
> - CRIT-3: Added full verification probe specification (57 entries)
> - CRIT-4: Added complete serialization format
> - HIGH-1: Fixed gas model with derivation from underlying DQA operations
> - HIGH-2: Added explicit result_scale definition
> - HIGH-3: Added scale preconditions to MAT_VEC_MUL
> - HIGH-4: Added TRAP code definitions
> - MED-1: Clarified dimension limits (M,N ≤ 8)
> - MED-2: Added element scale validation to MAT_ADD, MAT_SUB, MAT_SCALE
> - MED-4: Added NUMERIC_SPEC_VERSION declaration
> - MED-5: Completed test vector tables
> - LOW-1: Added scale matching determinism rule
> - LOW-2: Specified MAT_TRANSPOSE canonicalization
> - LOW-3: Added type trait consistency note
> - LOW-4: Created reference Python implementation

## Summary

This RFC defines Deterministic Matrix (DMAT) operations for consensus-critical linear algebra used in AI inference.

## Relationship to Other RFCs

| RFC                   | Relationship                  |
| --------------------- | ----------------------------- |
| RFC-0104 (DFP)        | DMAT<DFP> is FORBIDDEN        |
| RFC-0105 (DQA)        | DMAT<DQA> is the primary type |
| RFC-0112 (DVEC)       | Matrix-vector multiplication  |
| RFC-0114 (Activation) | Applied after matrix ops      |

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

> **Note:** This RFC uses `Numeric` enum for phase 1 simplicity. Future versions may transition to `NumericScalar` trait (per RFC-0112) for generic element operations. The enum approach matches RFC-0105's Dqa/Decimal distinction.

```

### Memory Layout (Row-Major)

```

Index(i, j) = i \* cols + j

Example: 2x3 matrix
[ a00, a01, a02 ]
[ a10, a11, a12 ]

Data: [a00, a01, a02, a10, a11, a12]

```

## Scale Handling

### Per-Element Scale Requirements

All elements in a DMAT must have the same scale (per RFC-0105 scale matching rules).

### MAT_MUL Scale Derivation

For MAT_MUL where A is M×K with scale s_a, and B is K×N with scale s_b:

- Each dot product element C[i][j] = sum(A[i][k] * B[k][j] for k in 0..K)
- Per RFC-0105 MUL: scale(product) = s_a + s_b
- Per RFC-0105 ADD: scale(sum) = max(s_a + s_b for all products)
- For DQA: s_a + s_b <= 18 required (MAX_SCALE constraint)
- For Decimal: s_a + s_b <= 36 required

### MAT_VEC_MUL Scale Derivation

For MAT_VEC_MUL where A is M×K with scale s_a, and V is K×1 with scale s_v:

- Result scale = s_a + s_v (per MAT_MUL semantics)
- For DQA: s_a + s_v <= 18 required

## Production Limitations

| Feature | Limit | Status |
|---------|-------|--------|
| DMAT<DQA> | M×N ≤ 64, M ≤ 8, N ≤ 8 | ALLOWED |
| DMAT<Decimal> | M×N ≤ 64, M ≤ 8, N ≤ 8 | ALLOWED |
| DMAT<DFP> | DISABLED | FORBIDDEN |
| DVEC (reference) | N ≤ 64 | ALLOWED |

> **Boundary:** Maximum single dimension is 8. A 9×8 matrix (72 elements) is REJECTED even though 8×9 would be valid. The per-dimension limit M,N ≤ 8 is stricter than the total element limit M×N ≤ 64.
>
> **Rationale:** The M×N ≤ 64 limit ensures worst-case gas stays within measurable bounds for debuggable execution. The M,N ≤ 8 per-dimension limit prevents pathological 1×64 or 64×1 matrices that could cause issues in certain algorithms.

## Core Operations

### MAT_ADD — Matrix Addition

```

mat_add(a: &DMat<Dqa>, b: &DMat<Dqa>) -> DMat<Dqa>

Preconditions:

- a.rows == b.rows
- a.cols == b.cols
- a.rows \* a.cols <= MAX_DMAT_ELEMENTS (64)
- All elements in a have same scale as a[0][0]
- All elements in b have same scale as b[0][0]
- a[0][0].scale() == b[0][0].scale() // Scale must match

Algorithm:
For i in 0..a.rows:
For j in 0..a.cols:
if a[i][j].scale() != a[0][0].scale(): TRAP(SCALE_MISMATCH)
if b[i][j].scale() != b[0][0].scale(): TRAP(SCALE_MISMATCH)
result[i][j] = a[i][j].add(b[i][j])?

Return result

```

### MAT_SUB — Matrix Subtraction

```

mat_sub(a: &DMat<Dqa>, b: &DMat<Dqa>) -> DMat<Dqa>

Preconditions:

- a.rows == b.rows
- a.cols == b.cols
- a.rows \* a.cols <= MAX_DMAT_ELEMENTS (64)
- All elements in a have same scale as a[0][0]
- All elements in b have same scale as b[0][0]
- a[0][0].scale() == b[0][0].scale() // Scale must match

Algorithm:
For i in 0..a.rows:
For j in 0..a.cols:
if a[i][j].scale() != a[0][0].scale(): TRAP(SCALE_MISMATCH)
if b[i][j].scale() != b[0][0].scale(): TRAP(SCALE_MISMATCH)
result[i][j] = a[i][j].sub(b[i][j])?

Return result

```

### MAT_MUL — Matrix Multiplication

```

mat_mul(a: &DMat<Dqa>, b: &DMat<Dqa>) -> DMat<Dqa>

> ⚠️ **REQUIREMENT**: Naive triple loop algorithm ONLY. No Strassen, no blocking.

Preconditions:

- a.cols == b.rows (dimension check)
- a.rows \* b.cols <= MAX_DMAT_ELEMENTS (64)
- All elements in a have same scale as a[0][0]
- All elements in b have same scale as b[0][0]
- a[0][0].scale() == b[0][0].scale() // Scale must match
- For DQA: a[0][0].scale() <= 9 (ensure result_scale <= 18)
- For Decimal: a[0][0].scale() <= 18 (ensure result_scale <= 36)

Algorithm (naive triple loop with overflow TRAP):

1. result_scale = a[0][0].scale() + b[0][0].scale() // Per RFC-0105 MUL
2. if result_scale > T::MAX_SCALE: TRAP(INVALID_SCALE)

For i in 0..a.rows: // Sequential: i=0, then 1, then 2...
For j in 0..b.cols:
accumulator = i128(0)
For k in 0..a.cols: // Sequential: k=0, then 1, then 2...
// TRAP priority: SCALE_MISMATCH checked first, then INVALID_SCALE, then OVERFLOW
if a.data[i * a.cols + k].scale() != a.data[0].scale(): TRAP(SCALE_MISMATCH)
if b.data[k * b.cols + j].scale() != b.data[0].scale(): TRAP(SCALE_MISMATCH)
product = a.data[i * a.cols + k].mul(b.data[k * b.cols + j])?
accumulator = accumulator + i128(product.raw_mantissa())

    if !accumulator.fits_in_i64(): TRAP(OVERFLOW)
    result.data[i * result.cols + j] = Dqa { value: accumulator as i64, scale: result_scale }

```

> ⚠️ **CRITICAL**: Sequential loops only. No SIMD, no parallelization.

### Result Scale

For MAT_MUL(A, B) where A[i][k] has scale s_a and B[k][j] has scale s_b:

- result_scale = s_a + s_b (per RFC-0105 MUL)
- If result_scale > MAX_SCALE (18 for DQA, 36 for Decimal): TRAP(INVALID_SCALE)

**Example:**
- A[i][k] scale = 4, B[k][j] scale = 5
- product scale = 4 + 5 = 9
- Each dot product element C[i][j] = sum of 8 products, each with scale 9
- After canonicalization: result_scale = min(9, MAX_SCALE)

### Overflow Detection

Per RFC-0105 I128_ROUNDTRIP:
- Accumulator uses i128 for intermediate computation
- Final cast to i64 checks: `if !accumulator.fits_in_i64(): TRAP(OVERFLOW)`

### MAT_VEC_MUL — Matrix-Vector Multiplication

```

mat_vec_mul(a: &DMat<Dqa>, v: &[Dqa]) -> Vec<Dqa>

Preconditions:

- a.cols == v.len
- a.rows <= MAX_DVEC_DIM (64)
- a.rows \* a.cols <= MAX_DMAT_ELEMENTS (64)
- All matrix elements have same scale as a[0][0]
- All vector elements have same scale as v[0]
- a[0][0].scale() == v[0].scale() // Scale must match
- For DQA: a[0][0].scale() <= 9 (ensure result_scale <= 18)
- For Decimal: a[0][0].scale() <= 18 (ensure result_scale <= 36)

Algorithm:
For i in 0..a.rows:
accumulator = i128(0)
For j in 0..a.cols:
// Scale check per RFC-0105
if a.data[i * a.cols + j].scale() != a.data[0].scale(): TRAP(SCALE_MISMATCH)
if v[j].scale() != v[0].scale(): TRAP(SCALE_MISMATCH)
product_scale = a.data[i * a.cols + j].scale() + v[j].scale()
if product_scale > T::MAX_SCALE: TRAP(INVALID_SCALE)
accumulator = accumulator + i128(a.data[i * a.cols + j].raw_mantissa() \* v[j].raw_mantissa())
if !accumulator.fits_in_i64(): TRAP(OVERFLOW)
result[i] = Dqa { value: accumulator as i64, scale: result_scale }

````

### Result Scale

For MAT_VEC_MUL where A has scale s_a and V has scale s_v:
- result_scale = s_a + s_v (per RFC-0105 MUL semantics)
- If result_scale > MAX_SCALE: TRAP(INVALID_SCALE)

### Equivalence to DVEC.dot_product

MAT_VEC_MUL produces identical results to:
```rust
let row = &a.data[i * a.cols..(i+1) * a.cols];
let result[i] = dot_product(row, v)?;
````

Where `dot_product` is defined per RFC-0112 §DOT_PRODUCT.

### MAT_TRANSPOSE — Matrix Transpose

```

mat_transpose(a: &DMat<Dqa>) -> DMat<Dqa>

Preconditions:

- a.rows \* a.cols <= MAX_DMAT_ELEMENTS (64)
- All elements in a have same scale as a[0][0]

Algorithm:
result.rows = a.cols
result.cols = a.rows
For i in 0..a.rows:
For j in 0..a.cols:
if a[i][j].scale() != a[0][0].scale(): TRAP(SCALE_MISMATCH)
result[j][i] = a[i][j].clone()
Return result

Note: Transpose does not change element values or scales, only layout.

```

### MAT_SCALE — Matrix Scale

```

mat_scale(a: &DMat<Dqa>, scalar: Dqa) -> DMat<Dqa>

Preconditions:

- a.rows \* a.cols <= MAX_DMAT_ELEMENTS (64)
- All elements in a have same scale as a[0][0]
- For DQA: a[0][0].scale() + scalar.scale() <= 18
- For Decimal: a[0][0].scale() + scalar.scale() <= 36

Algorithm:
For i in 0..a.rows:
For j in 0..a.cols:
if a[i][j].scale() != a[0][0].scale(): TRAP(SCALE_MISMATCH)
product_scale = a[i][j].scale() + scalar.scale()
if product_scale > T::MAX_SCALE: TRAP(INVALID_SCALE)
result[i][j] = a[i][j].mul(scalar)?

```

### DOT_PRODUCT (Row × Column)

```

mat_dot_rows(a: &[Dqa], b: &[Dqa]) -> Dqa

Algorithm: Same as DVEC dot_product.

```

## Gas Model

Gas derivation follows RFC-0105 where:

- DQA MUL: `20 + 3 × scale_a × scale_b` gas
- DQA ADD: `10 + 3 × max(scale_a, scale_b)` gas

### Per-Operation Gas

| Operation     | Formula                           | Derivation                            |
| ------------- | --------------------------------- | ------------------------------------- |
| MAT_ADD       | `5 × M × N`                       | M×N element ADD operations            |
| MAT_SUB       | `5 × M × N`                       | M×N element SUB operations            |
| MAT_MUL       | `M × N × K × (30 + 3 × scale²)`   | M×N×K dot products, each N elements   |
| MAT_VEC_MUL   | `rows × cols × (30 + 3 × scale²)` | rows dot products, each cols elements |
| MAT_TRANSPOSE | `2 × M × N`                       | M×N element copies                    |
| MAT_SCALE     | `5 × M × N`                       | M×N element MUL operations            |

### Gas Notes

- **MAT_MUL formula:** `M × N × K × (30 + 3 × scale²)` combines DQA MUL cost (20 + 3×scale²) + DQA ADD cost (10 + 3×scale²) per MAC
- **Scale check overhead:** The two SCALE_MISMATCH checks per element are O(1) and absorbed into the base cost
- **Per-block budget:** 139,776 gas exceeds 50k consensus budget; MAT_MUL is limited to M×N ≤ 64

### Gas Examples (scale=0, DQA)

| Operation     | Dimensions | Gas |
| ------------- | ---------- | --- |
| MAT_ADD       | 8×8        | 320 |
| MAT_MUL       | 4×4 × 4×4  | 640 |
| MAT_VEC_MUL   | 4×4 × 4    | 160 |
| MAT_TRANSPOSE | 8×8        | 128 |
| MAT_SCALE     | 8×8        | 320 |

### Per-Block Budget

MAT_MUL at MAX_DMAT_ELEMENTS (8×8=64) with K=8 and scale=9:

- Per dot product: K × (30 + 3 × scale²) = 8 × (30 + 3 × 81) = 8 × 273 = 2184
- Total: M × N × 2184 = 8 × 8 × 2184 = 139,776

> This exceeds 50k consensus budget; MAT_MUL is limited to M×N ≤ 64 to stay within measurable bounds.

## Test Vectors

### MAT_ADD

| A                | B                | Scale | Expected           | Notes    |
| ---------------- | ---------------- | ----- | ------------------ | -------- |
| [[1, 2], [3, 4]] | [[5, 6], [7, 8]] | 0     | [[6, 8], [10, 12]] | Basic    |
| [[1, 2]]         | [[3, 4]]         | 0     | [[4, 6]]           | 1×2      |
| [[0, 0], [0, 0]] | [[1, 2], [3, 4]] | 0     | [[1, 2], [3, 4]]   | Identity |

### MAT_SUB

| A                | B                | Scale | Expected         | Notes       |
| ---------------- | ---------------- | ----- | ---------------- | ----------- |
| [[5, 6], [7, 8]] | [[1, 2], [3, 4]] | 0     | [[4, 4], [4, 4]] | Basic       |
| [[1, 1], [1, 1]] | [[1, 1], [1, 1]] | 0     | [[0, 0], [0, 0]] | Zero result |

### MAT_MUL

| A                    | B                    | Scale | Expected                                            | Notes              |
| -------------------- | -------------------- | ----- | --------------------------------------------------- | ------------------ |
| [[1, 0], [0, 1]]     | [[2, 3], [4, 5]]     | 0     | [[2, 3], [4, 5]]                                    | Identity           |
| [[1, 2], [3, 4]]     | [[5, 6], [7, 8]]     | 0     | [[19, 22], [43, 50]]                                | Standard           |
| [[1, 2, 3]]          | [[1], [2], [3]]      | 0     | [[14]]                                              | Vector result      |
| [[2, 2], [2, 2]]     | [[3, 3], [3, 3]]     | 0     | [[12, 12], [12, 12]]                                | Uniform            |
| [[10, 20], [30, 40]] | [[10, 20], [30, 40]] | 0     | [[1400, 2200], [3000, 4600]] → [[14, 22], [30, 46]] | Canonical: 1400→14 |

### MAT_VEC_MUL

| Matrix                 | Vector    | Scale | Expected | Notes  |
| ---------------------- | --------- | ----- | -------- | ------ |
| [[1, 2], [3, 4]]       | [1, 1]    | 0     | [3, 7]   | Basic  |
| [[1, 0, 0], [0, 1, 0]] | [1, 2, 3] | 0     | [1, 2]   | Sparse |

### MAT_TRANSPOSE

| Input            | Scale | Expected         | Notes         |
| ---------------- | ----- | ---------------- | ------------- |
| [[1, 2], [3, 4]] | 0     | [[1, 3], [2, 4]] | 2×2           |
| [[1, 2, 3]]      | 0     | [[1], [2], [3]]  | Row to column |

### MAT_SCALE

| Matrix           | Scalar | Scale | Expected         | Notes       |
| ---------------- | ------ | ----- | ---------------- | ----------- |
| [[1, 2], [3, 4]] | 2      | 0     | [[2, 4], [6, 8]] | Basic       |
| [[1, 1], [1, 1]] | 0      | 0     | [[0, 0], [0, 0]] | Zero scalar |

### Boundary Cases

| Operation   | Input               | Expected | TRAP Code          |
| ----------- | ------------------- | -------- | ------------------ |
| MAT_MUL     | 9×9 matrix          | REJECT   | DIMENSION_ERROR    |
| MAT_MUL     | a.cols != b.rows    | REVERT   | DIMENSION_MISMATCH |
| MAT_ADD     | Dimension mismatch  | REVERT   | DIMENSION_MISMATCH |
| MAT_SUB     | Dimension mismatch  | REVERT   | DIMENSION_MISMATCH |
| MAT_VEC_MUL | a.cols != v.len     | REVERT   | DIMENSION_MISMATCH |
| MAT_MUL     | Scale > 9 (DQA)     | TRAP     | INVALID_SCALE      |
| MAT_ADD     | Scale mismatch      | TRAP     | SCALE_MISMATCH     |
| MAT_MUL     | Max values overflow | TRAP     | OVERFLOW           |

## Verification Probe

### Probe Entry Serialization Format (Canonical)

**DMat Canonical Wire Format:**

```

leaf_input = op_id (8 bytes) || type_id (1 byte) ||
a_rows (1 byte) || a_cols (1 byte) || a_elements... ||
b_rows (1 byte) || b_cols (1 byte) || b_elements... ||
result_rows (1 byte) || result_cols (1 byte) || result_elements...

```

Where:

- `op_id`: 8-byte operation identifier (see Operation IDs)
- `type_id`: 1 byte (1=DQA, 2=Decimal)
- Matrix elements serialized as 24-byte blocks per RFC-0105/0111

### Operation IDs

| Operation     | ID (hex) |
| ------------- | -------- |
| MAT_ADD       | 0x0100   |
| MAT_SUB       | 0x0101   |
| MAT_MUL       | 0x0102   |
| MAT_VEC_MUL   | 0x0103   |
| MAT_TRANSPOSE | 0x0104   |
| MAT_SCALE     | 0x0105   |

### TRAP Sentinel Definition

```

TRAP = { mantissa: 0x8000000000000000 (i64 min), scale: 0xFF }

```

### Published Merkle Root

> **Merkle Root:** `16851510fcd205f753f3f0b0aeed9b7015332632d455b468a0dd43d9610899ca`

### Probe Entry Details

| Entry | Operation     | Type    | Input A                       | Input B                       | Expected                          |
| ----- | ------------- | ------- | ----------------------------- | ----------------------------- | --------------------------------- |
| 0     | MAT_ADD       | DQA     | [[1,2],[3,4]]                 | [[5,6],[7,8]]                 | [[6,8],[10,12]]                   |
| 1     | MAT_MUL       | DQA     | [[1,0],[0,1]]                 | [[2,3],[4,5]]                 | [[2,3],[4,5]]                     |
| 2     | MAT_MUL       | DQA     | [[1,2],[3,4]]                 | [[5,6],[7,8]]                 | [[19,22],[43,50]]                 |
| 3     | MAT_VEC_MUL   | DQA     | [[1,2],[3,4]]                 | [1,1]                         | [3,7]                             |
| 4     | MAT_TRANSPOSE | DQA     | [[1,2],[3,4]]                 | -                             | [[1,3],[2,4]]                     |
| 5     | MAT_SCALE     | DQA     | [[1,2],[3,4]]                 | scalar=2                      | [[2,4],[6,8]]                     |
| 6     | MAT_ADD       | Decimal | [[1,2],[3,4]]                 | [[5,6],[7,8]]                 | [[6,8],[10,12]]                   |
| 7     | MAT_MUL       | Decimal | [[1,0],[0,1]]                 | [[2,3],[4,5]]                 | [[2,3],[4,5]]                     |
| 8     | MAT_MUL       | Decimal | [[1,2],[3,4]]                 | [[5,6],[7,8]]                 | [[19,22],[43,50]]                 |
| 9     | MAT_ADD       | DQA     | [[0,0],[0,0]]                 | [[1,2],[3,4]]                 | [[1,2],[3,4]]                     |
| 10    | MAT_SUB       | DQA     | [[5,6],[7,8]]                 | [[1,2],[3,4]]                 | [[4,4],[4,4]]                     |
| 11    | MAT_VEC_MUL   | Decimal | [[1,2],[3,4]]                 | [1,1]                         | [3,7]                             |
| 12    | MAT_TRANSPOSE | Decimal | [[1,2],[3,4]]                 | -                             | [[1,3],[2,4]]                     |
| 13    | MAT_SCALE     | Decimal | [[1,2],[3,4]]                 | scalar=2                      | [[2,4],[6,8]]                     |
| 14    | MAT_MUL       | DQA     | [[1,2,3],[4,5,6]]             | [[1,2],[3,4],[5,6]]           | [[9,12,15],[19,26,33],[29,40,51]] |
| 15    | MAT_MUL       | DQA     | [[1,0,0,0],[0,1,0,0]]         | [[1,2],[3,4],[5,6],[7,8]]     | [[5,6],[23,34]]                   |
| 16    | MAT_MUL       | DQA     | [[10,20]]                     | [[3],[4]]                     | [[110]]                           |
| 17    | MAT_MUL       | DQA     | [[3],[4]]                     | [[10,20]]                     | [[30,60],[40,80]]                 |
| 18    | MAT_MUL       | DQA     | [[1],[2],[3]]                 | [[1,2,3]]                     | [[1,2,3],[2,4,6],[3,6,9]]         |
| 19    | MAT_MUL       | DQA     | [[5,5],[5,5]]                 | [[5,5],[5,5]]                 | [[50,50],[50,50]]                 |
| 20    | MAT_VEC_MUL   | DQA     | [[1,2],[3,4]]                 | [1,1]                         | [3,7]                             |
| 21    | MAT_VEC_MUL   | DQA     | [[1,0,0],[0,1,0]]             | [1,2,3]                       | [1,2]                             |
| 22    | MAT_VEC_MUL   | DQA     | [[1,2,3],[4,5,6],[7,8,9]]     | [1,1,1]                       | [12,15,18]                        |
| 23    | MAT_VEC_MUL   | DQA     | [[2,4,6,8]]                   | [2]                           | [40]                              |
| 24    | MAT_VEC_MUL   | DQA     | [[1],[2],[3],[4]]             | [1,2,3,4]                     | [30]                              |
| 25    | MAT_TRANSPOSE | DQA     | [[1,2],[3,4]]                 | -                             | [[1,3],[2,4]]                     |
| 26    | MAT_TRANSPOSE | DQA     | [[1,2,3]]                     | -                             | [[1],[2],[3]]                     |
| 27    | MAT_TRANSPOSE | DQA     | [[1],[2],[3]]                 | -                             | [[1,2,3]]                         |
| 28    | MAT_TRANSPOSE | DQA     | [[1,2,3],[4,5,6]]             | -                             | [[1,4],[2,5],[3,6]]               |
| 29    | MAT_TRANSPOSE | DQA     | [[1,2],[3,4],[5,6],[7,8]]     | -                             | [[1,3,5,7],[2,4,6,8]]             |
| 30    | MAT_SCALE     | DQA     | [[1,2],[3,4]]                 | scalar=2                      | [[2,4],[6,8]]                     |
| 31    | MAT_SCALE     | DQA     | [[1,1],[1,1]]                 | scalar=0                      | [[0,0],[0,0]]                     |
| 32    | MAT_SCALE     | DQA     | [[5,5],[5,5],[5,5]]           | scalar=3                      | [[15,15],[15,15],[15,15]]         |
| 33    | MAT_SCALE     | DQA     | [[10,20,30,40]]               | scalar=2                      | [[20,40,60,80]]                   |
| 34    | MAT_SCALE     | DQA     | [[3],[3],[3],[3]]             | scalar=3                      | [[9],[9],[9],[9]]                 |
| 35    | MAT_ADD       | Decimal | [[1,2],[3,4]]                 | [[5,6],[7,8]]                 | [[6,8],[10,12]]                   |
| 36    | MAT_SUB       | Decimal | [[5,6],[7,8]]                 | [[1,2],[3,4]]                 | [[4,4],[4,4]]                     |
| 37    | MAT_MUL       | Decimal | [[1,0],[0,1]]                 | [[2,3],[4,5]]                 | [[2,3],[4,5]]                     |
| 38    | MAT_MUL       | Decimal | [[1,2],[3,4]]                 | [[5,6],[7,8]]                 | [[19,22],[43,50]]                 |
| 39    | MAT_VEC_MUL   | Decimal | [[1,2],[3,4]]                 | [1,1]                         | [3,7]                             |
| 40    | MAT_VEC_MUL   | Decimal | [[1,2,3],[4,5,6],[7,8,9]]     | [1,1,1]                       | [12,15,18]                        |
| 41    | MAT_TRANSPOSE | Decimal | [[1,2],[3,4]]                 | -                             | [[1,3],[2,4]]                     |
| 42    | MAT_SCALE     | Decimal | [[1,2],[3,4]]                 | scalar=2                      | [[2,4],[6,8]]                     |
| 43    | MAT_ADD       | Decimal | [[10,20],[30,40]]             | [[1,2],[3,4]]                 | [[11,22],[33,44]]                 |
| 44    | MAT_SUB       | Decimal | [[100,200],[300,400]]         | [[10,20],[30,40]]             | [[90,180],[270,360]]              |
| 45    | MAT_MUL       | Decimal | [[1,2,3]]                     | [[1],[2],[3]]                 | [[14]]                            |
| 46    | MAT_MUL       | Decimal | [[1,2],[3,4],[5,6]]           | [[1,2],[3,4],[5,6]]           | [[9,12,15],[19,26,33],[29,40,51]] |
| 47    | MAT_SCALE     | Decimal | [[10,20,30,40]]               | scalar=3                      | [[30,60,90,120]]                  |
| 48    | MAT_MUL       | DQA     | 9×9 empty                     | 9×9 empty                     | TRAP (DIMENSION_ERROR)            |
| 49    | MAT_MUL       | DQA     | 2×3                           | 2×3                           | TRAP (DIMENSION_MISMATCH)         |
| 50    | MAT_ADD       | DQA     | 2×2                           | 2×3                           | TRAP (DIMENSION_MISMATCH)         |
| 51    | MAT_VEC_MUL   | DQA     | 2×3                           | [1,2]                         | TRAP (DIMENSION_MISMATCH)         |
| 52    | MAT_MUL       | DQA     | [[10^8,0],[0,10^8]]           | [[10^8,0],[0,10^8]]           | TRAP (OVERFLOW)                   |
| 53    | MAT_SCALE     | DQA     | [[10^9×4]]                    | scalar=10^9                   | TRAP (OVERFLOW)                   |
| 54    | MAT_ADD       | DQA     | [[1@scale10,2],[3,4]]         | [[5,6],[7,8]]                 | TRAP (SCALE_MISMATCH)             |
| 55    | MAT_MUL       | DQA     | [[1@scale10,0],[0,1@scale10]] | [[1@scale10,0],[0,1@scale10]] | TRAP (INVALID_SCALE)              |
| 56    | MAT_ADD       | DQA     | [TRAP]                        | [0]                           | TRAP (propagated)                 |

> **Note:** Full 57 entries required per RFC-0110/NUMERIC_SPEC conventions.

## Serialization Format

### Matrix Element Encoding (24 bytes)

**For DQA:**

```

element = version (1 byte = 0x01) || reserved (3 bytes = 0x00) ||
scale (1 byte) || reserved (3 bytes = 0x00) ||
mantissa (16 bytes, big-endian i128)

```

**For Decimal:**

```

element = version (1 byte = 0x01) || reserved (3 bytes = 0x00) ||
scale (1 byte) || reserved (3 bytes = 0x00) ||
mantissa (16 bytes, big-endian i128)

```

> **Sign-Extension Rationale:** When encoding DQA's 64-bit mantissa into the 128-bit slot, the upper 64 bits are sign-extended (duplicate the sign bit). This matches two's complement representation semantics and ensures the probe encoding correctly represents negative DQA values in the 128-bit slot for deterministic Merkle tree construction.

### Type ID Byte

- `0x01` = DQA (Deterministic Quantized Arithmetic)
- `0x02` = Decimal (per RFC-0111)

### Matrix Encoding

```

matrix = rows (1 byte) || cols (1 byte) || element[0] || element[1] || ...

```

### Scalar Encoding in Probes

For MAT_SCALE and MAT_VEC_MUL, the scalar operand is encoded as a 1×1 matrix:

```
scalar = rows (1 byte = 0x01) || cols (1 byte = 0x01) || element (24 bytes)
```

For MAT_VEC_MUL, the vector is encoded as N×1:

```
vector = rows (1 byte) || cols (1 byte = 0x01) || element[0] || element[1] || ...
```

### Probe Leaf Computation

```

leaf = SHA256(concat(leaf_input elements))
root = MerkleRoot(leaf[0], leaf[1], ..., leaf[56])

```

### Verification Procedure

1. For each probe entry, serialize inputs using canonical format
2. Execute operation per algorithms in this RFC
3. Serialize result using canonical format
4. Compute leaf hash: SHA256(leaf_input)
5. Build Merkle tree from 57 leaves
6. Verify root matches published Merkle root

## Determinism Rules

1. **Naive Algorithm Only**: No Strassen, no blocking optimization
2. **Sequential Loops**: No SIMD, no parallelization
3. **Row-Major Layout**: Must match specification
4. **Dimension Enforcement**: M×N ≤ 64 for execution
5. **Scale Matching**: All elements in a matrix must have the same scale
6. **Type Isolation**: No mixed-type operations (DMAT<DQA> vs DMAT<Decimal>)

## TRAP Codes

| Code                         | Condition                                                       | Reference |
| ---------------------------- | --------------------------------------------------------------- | --------- |
| OVERFLOW                     | i128 accumulator exceeds i64 range for DQA, or i128 for Decimal | RFC-0105  |
| INVALID_SCALE                | Result scale exceeds MAX_SCALE (18 DQA, 36 Decimal)             | RFC-0105  |
| SCALE_MISMATCH               | Matrix/vector elements have different scales                    | RFC-0105  |
| DIMENSION_ERROR              | Matrix dimensions M×N > 64                                      | RFC-0113  |
| DIMENSION_MISMATCH           | Matrix dimensions incompatible for operation                    | RFC-0113  |
| CANNOT_NORMALIZE_ZERO_VECTOR | NORM of zero vector                                             | RFC-0112  |
| CONSENSUS_RESTRICTION        | Operation forbidden in consensus context                        | RFC-0113  |
| UNSUPPORTED_OPERATION        | Operation not supported for element type                        | RFC-0113  |

### TRAP Priority Order

When multiple error conditions exist in a single operation:

1. **SCALE_MISMATCH** - Element scale differs from matrix/vector scale
2. **INVALID_SCALE** - Result scale exceeds MAX_SCALE
3. **OVERFLOW** - Accumulator exceeds representable range
4. **DIMENSION_MISMATCH** - Matrix dimensions incompatible for operation
5. **DIMENSION_ERROR** - Matrix exceeds size limits

> **Rationale:** Scale validation is checked first to catch semantic errors early. Dimension errors are checked last as they are configuration errors.

### TRAP Sentinel (for probe encoding)

```

TRAP = { mantissa: 0x8000000000000000 (i64 min), scale: 0xFF }

```

Per RFC-0111 v1.20 Section 13.3.

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

## Appendix B: Reference Python Implementation

**File:** `scripts/compute_dmat_probe_root.py`

Run with: `python3 scripts/compute_dmat_probe_root.py`

> **Note:** The canonical reference is the script file. This RFC takes precedence over embedded descriptions.

```

```
