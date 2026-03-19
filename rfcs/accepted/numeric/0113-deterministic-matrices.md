# RFC-0113 (Numeric/Math): Deterministic Matrices (DMAT)

## Status

**Version:** 1.20 (2026-03-19)
**Status:** Accepted
**NUMERIC_SPEC_VERSION:** 1 (per RFC-0110 §Spec Version & Replay Pinning)

> **Note:** NUMERIC_SPEC_VERSION remains at 1 because this RFC does not change fundamental
> protocol semantics of existing numeric types. DMAT is a new container type that operates
> on existing numeric types without modifying their encoding, arithmetic, or TRAP semantics.

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

> **Normative Authority:** This RFC is the normative specification for DMAT operations. The reference script (`scripts/compute_dmat_probe_root.py`) is provided for verification and conformance testing. If any discrepancy exists between this RFC and the reference script, this RFC text takes precedence for consensus behavior.

> **Adversarial Review v1.20 Changes (Round 22 - Gold Standard):**
>
> - MED: Added entry 62 — TRAP at last index [1][1] to force full traversal past first element
> - MED: Tightened MAX_SCALE boundary wording — "must not reduce scale below MAX_SCALE"
> - MED: Recomputed Merkle root with 63 total entries

> **Adversarial Review v1.19 Changes (Round 20):**

> **Adversarial Review v1.18 Changes (Round 19 - Consensus Hardening):**
>
> - CRITICAL: Added Trait Version Enforcement with explicit prohibition on mixed versions
> - CRITICAL: Added Global TRAP Invariant (Phase 0 must precede all validation)
> - CRITICAL: Added Canonicalization Requirements binding T::new to RFC-0105 canonical form
> - MED: Added Algebraic Properties section (informative)
> - Reviewer note: CRIT-2 (iteration order) already fully specified with explicit i/j/k nesting in algorithm pseudocode

> **Adversarial Review v1.17 Changes (Round 18):**
>
> - HIGH-1: Added Scale Compatibility Matrix documenting scale rules per operation
> - Reviewer assessment: CRIT-1 through CRIT-5 are already addressed in prior rounds (trait supersession documented, iteration order explicit, TRAP propagation in Phase 0, canonicalization per RFC-0105, 59-entry probe sufficient)

> **Adversarial Review v1.16 Changes (Round 17):**
>
> - HIGH: Strengthened trait evolution note to explicitly require RFC-0113 trait for DMAT users
> - MED: Added two mixed-scale MAT_VEC_MUL probe entries (57, 58) and recomputed Merkle root (59 total)

> **Adversarial Review v1.15 Changes (Round 16):**
>
> - CRITICAL-2: Standardize TRAP sentinel to `-(1 << 63)` (signed int) in RFC text, matching RFC-0111 and probe script

> **Adversarial Review v1.14 Changes (Round 14 - Final):**
>
> - LOW-FINAL-1: Added note about missing mixed-scale MAT_VEC_MUL probe entry (non-blocking)
> - LOW-FINAL-2: Updated Merkle Root note to reflect "(computed from v1.11 script; unchanged through v1.14)"

> **Adversarial Review v1.13 Changes (Round 13 - Final):**
>
> - HIGH-NEW-FINAL-1: Fixed MAT_VEC_MUL Phase 3 to validate vector internal uniformity only. Removed incorrect cross-scale check against matrix scale. Mixed-scale multiplication now works correctly.

> **Adversarial Review v1.12 Changes (Round 12 - CRIT/HIGH/MED fixes):**
>
> - CRIT-NEW-1: Added M ≥ 1, N ≥ 1 enforcement to Production Limitations and all operation Phase 1 checks
> - HIGH-NEW-1: Added explicit trait evolution note stating RFC-0113 supersedes NumericScalar in RFC-0112
> - HIGH-NEW-2: Clarified MAT_VEC_MUL return type Vec<T> and DVec compatibility guarantees
> - MED-NEW-1: Added global index column to TRAP Codes table with TBD placeholders
> - MED-NEW-2: Added MAT_TRANSPOSE gas formula justification (1 gas per memory read/write)

> **Adversarial Review v1.11 Changes (Round 11):**

> - CRIT-1: Fixed stale gas examples table values
> - MED-1: Added cross-matrix scale check to MAT_ADD/MAT_SUB Phase 2; replaced entry 2 with cross-scale TRAP test
> - LOW-1: Removed empty code fence in Appendix B

> **Adversarial Review v1.10 Changes (Round 10):**

> - CRIT-1: Removed duplicate probe entry table; script is canonical reference
> - MED-1: Fixed gas formulas for MAT_ADD/SUB (10×M×N) and MAT_SCALE (per RFC-0105 MUL cost)
> - LOW-1: Clarified scale derivation comment

> **Adversarial Review v1.9 Changes (Round 8 - CRITICAL/HIGH fixes):**
>
> - CRIT-1: Entries 22/40 correct result from [12,15,18] to [6,15,24]
> - HIGH-1: MAT_SCALE Phase 2 result_scale check moved outside element loop
> - HIGH-2: Added note clarifying INPUT_VALIDATION_ERROR not used in DMAT

> **Adversarial Review v1.8 Changes (Round 7 - MEDIUM/LOW fixes):**
>
> - MED-1: Entry 55 uses uniform scale matrix for clean INVALID_SCALE test
> - MED-2: MAT_TRANSPOSE has explicit Phase 1 dimension validation
> - MED-3: MAT_SCALE separates validation from computation phases
> - LOW-1: Removed duplicate type_id line in wire format
> - LOW-3: Removed INPUT_VALIDATION_ERROR dead code from TRAP tables
> - LOW-4: Fixed op_id encoding example (16 hex digits not 13)

> **Adversarial Review v1.7 Changes (Round 6 - CRITICAL fixes):**
>
> - CRIT-1: TRAP sentinel uses `-(1 << 63)` (Python signed int) not `0x8000000000000000`
> - CRIT-2: Entries 52/53 use correct overflow test values (2^31 and 9223372038)
> - CRIT-3: MAT_MUL/MAT_VEC_MUL phase order swapped (SCALE_MISMATCH before INVALID_SCALE)
> - CRIT-4: Added Phase 0 TRAP Sentinel Pre-check to all operations
> - HIGH-1: Gas formulas use explicit `s_a × s_b` not ambiguous `scale²`
> - HIGH-2: Added `MAX_MANTISSA` associated constant to trait (removes `T == Dqa` branch)
> - HIGH-3: MAT_SCALE has explicit Phase 1 dimension validation
> - MED-4: Scalar encoding includes rows/cols prefix (wire format fix)

> **Adversarial Review v1.6 Changes (Round 5 - MEDIUM/LOW fixes):**
>
> - MED-1: Removed contradictory MAT_MUL comment (intermediate check removed per RFC-0112)
> - MED-3: Updated Merkle Root to v1.4 (verified)
> - LOW-1: MAT_MUL scale derivation example clarifies canonicalization behavior
> - LOW-2: Probe Entry 51 comment clarifies uniform TRAP encoding rationale
> - LOW-3: TRAP priority rationale expanded with two-phase explanation

> **Adversarial Review v1.4 Changes (Round 4 - MEDIUM/LOW fixes):**
>
> - MED-1: MAT_MUL comment clarifies Dqa::mul already validates i64 range
> - MED-2: MAT_VEC_MUL Phase 6 includes sequential loop mandate (RFC-0112 alignment)
> - MED-3: Probe Entry 46 comment clarifies 3×2 × 2×3 = 3×3 dimensions
> - MED-4: Gas Model table includes Type column (DQA/Decimal)
> - LOW-1: NUMERIC_SPEC_VERSION references RFC-0110 §Spec Version
> - LOW-2: TRAP Sentinel reference matches RFC-0111 §Verification Probe naming

> **Adversarial Review v1.3 Changes (Round 3 - HIGH/MEDIUM fixes):**
>
> - HIGH-1: MAT_MUL/MAT_VEC_MUL Decimal overflow check matches RFC-0112
> - MED-1: MAT_TRANSPOSE references Lazy Canonicalization (RFC-0111)
> - MED-2: MAT_MUL intermediate product check removed (redundant per RFC-0112)
> - MED-3: Probe Entry 51 result is `[TRAP, TRAP]` for uniform TRAP encoding
> - MED-4: Python script type hints updated for b_data union type
> - MED-5: RFC Table Entry 46 Input B visual matches 2×3 layout
> - MED-6: Added Vec<T> compatibility note for MAT_VEC_MUL return type

> **Adversarial Review v1.2 Changes (Round 2 - CRIT/HIGH/MEDIUM fixes):**
>
> - CRIT-1: Changed type system from `Numeric` enum to `NumericScalar` trait for composability with DVEC
> - CRIT-2: MAT_MUL overflow detection now checks intermediate product BEFORE accumulation
> - CRIT-3: MAT_VEC_MUL scale validation order matches RFC-0112 (precondition first)
> - CRIT-4: Defined TRAP propagation for Entry 56 via TRAP_INPUT_ERROR
> - HIGH-1: Fixed gas derivation text (ADD = 10 flat, not 10 + 3×scale²)
> - HIGH-2: MAT_SCALE validates scalar.scale() against MAX_SCALE
> - HIGH-3: TRAP priority starts with INPUT_VALIDATION_ERROR per RFC-0112
> - HIGH-4: MAT_TRANSPOSE specifies canonicalization requirement
> - HIGH-5: Python script handles Optional elements correctly
> - HIGH-6: Added explicit M≤8, N≤8 checks to all operation preconditions
> - MED-1: MAT_VEC_MUL return type uses Vec<T: NumericScalar>
> - MED-3: Test vector Entry 14 corrected to [[22,28],[49,64]] (2×2 result)
> - MED-4: Probe Entry 14 aligned with corrected test vector
> - MED-5: MAT_ADD/MAT_SUB validate all scales before iteration
> - MED-6: Added BigInt overhead note to gas model
>
> **Adversarial Review v1.1 Changes (Comprehensive Fixes):**
>
> - CRIT-1: Added explicit scale handling per RFC-0105 semantics
> - CRIT-2: Added overflow detection to MAT_MUL algorithm
> - CRIT-3: Added full verification probe specification (59 entries)
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
/// Trait for numeric scalar types that can be elements of DMAT
pub trait NumericScalar: Clone + PartialEq + Debug {
    type Scalar;
    const MAX_SCALE: u8;
    // HIGH-2: MAX_MANTISSA replaces T == Dqa branch for overflow detection
    const MAX_MANTISSA: i128;

    fn scale(&self) -> u8;
    // CRIT-2/CRIT-5: new accepts i128 to support both DQA (i64-range) and Decimal (i128-range)
    fn new(mantissa: i128, scale: u8) -> Result<Self, Error>
    where
        Self: Sized;
    fn add(&self, other: &Self) -> Result<Self, Error>;
    fn sub(&self, other: &Self) -> Result<Self, Error>;
    fn mul(&self, other: &Self) -> Result<Self, Error>;
    // CRIT-5: raw_mantissa returns i128 to avoid truncating Decimal values
    fn raw_mantissa(&self) -> i128;
}

/// Deterministic Matrix
pub struct DMat<T: NumericScalar> {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<T>,  // Row-major layout
}
```

> **Note:** This RFC uses `NumericScalar` trait for generic element operations, enabling composition with DVEC (RFC-0112). The trait approach replaces the earlier enum-based `Numeric` type for better composability across the Deterministic Numeric Tower.
>
> **Trait Evolution (HIGH-NEW-1):** This RFC **supersedes** the `NumericScalar` trait definition in RFC-0112 v1.12 by adding `const MAX_MANTISSA: i128` and `fn new(mantissa: i128, scale: u8) -> Self`.
> **Normative requirement:** Any type implementing `NumericScalar` that is intended to be used inside `DMat<T>` (via MAT_MUL, MAT_VEC_MUL, MAT_SCALE, etc.) **MUST** implement the RFC-0113 version of the trait with `MAX_MANTISSA` and `new(...)`. Implementations that only target pure DVEC usage MAY continue using the RFC-0112 trait definition until they adopt matrix operations.
>
> **Trait Version Enforcement (CRITICAL):** The `NumericScalar` trait defined in this RFC is the **canonical and exclusive** trait definition for all consensus-critical numeric operations involving DMAT.
> 1. A type implementing `NumericScalar` **MUST NOT** implement multiple versions of the trait across RFC-0112 and RFC-0113 in the same execution environment.
> 2. Any `NumericScalar` implementation used in consensus-critical contexts **MUST** conform to the RFC-0113 trait definition.
> 3. Mixing trait versions across modules, dynamic libraries, or execution boundaries (e.g., WASM, FFI) is **FORBIDDEN**.

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
- Per RFC-0105 ADD: scale(sum) = s_a + s_b (all K products have equal scale, so max is trivially s_a + s_b)
- For DQA: s_a + s_b <= 18 required (MAX_SCALE constraint)
- For Decimal: s_a + s_b <= 36 required

### MAT_VEC_MUL Scale Derivation

For MAT_VEC_MUL where A is M×K with scale s_a, and V is K×1 with scale s_v:

- Result scale = s_a + s_v (per MAT_MUL semantics)
- For DQA: s_a + s_v <= 18 required

### Scale Compatibility Matrix (HIGH-1)

| Operation | Scale Rule | Cross-Operand Scale Matching |
|-----------|-----------|----------------------------|
| MAT_ADD | Elements must match within each operand; operands must match each other | Strict equality required |
| MAT_SUB | Same as MAT_ADD | Strict equality required |
| MAT_MUL | Result scale = s_a + s_b | Composition allowed |
| MAT_VEC_MUL | Result scale = s_a + s_v | Composition allowed |
| MAT_SCALE | Result scale = s_a + scalar.scale() | Composition allowed |
| MAT_TRANSPOSE | Preserves element scales | N/A (unary) |

> **Note:** "Composition allowed" means operands may have different scales. "Strict equality required" means all elements within an operand AND both operands must have identical scales.

> **MAX_SCALE Boundary Invariant (CRITICAL):** If `result_scale == MAX_SCALE` (18 for DQA, 36 for Decimal), canonicalization **MUST NOT** reduce the scale below MAX_SCALE. The result may remain at MAX_SCALE or overflow, but it must not be normalized to a lower scale. For example, `1×10^-18` stored as `(mantissa=1, scale=18)` is valid and must not be canonicalized to `(mantissa=1, scale=0)`.

### Canonicalization Requirements (CRITICAL)

All scalar values stored in `DMat<T>` MUST be in canonical form as defined by RFC-0105.

1. The constructor `T::new(mantissa, scale)` **MUST** return a canonicalized value.
2. All results produced by DMAT operations **MUST** be canonical at the time of insertion into `result.data`.
3. Implementations **MUST NOT** construct non-canonical values and defer normalization.
4. Canonicalization behavior **MUST** be identical to RFC-0105 arithmetic operations.

> **Rationale:** DMAT participates in canonical serialization and Merkle root computation. Non-canonical representations would lead to divergent hashes across implementations.

## Production Limitations

| Feature | Limit | Status |
|---------|-------|--------|
| DMAT<DQA> | M×N ≤ 64, M ≤ 8, N ≤ 8, **M ≥ 1, N ≥ 1** | ALLOWED |
| DMAT<Decimal> | M×N ≤ 64, M ≤ 8, N ≤ 8, **M ≥ 1, N ≥ 1** | ALLOWED |
| DMAT<DFP> | DISABLED | FORBIDDEN |
| DVEC (reference) | N ≤ 64 | ALLOWED |

> **Boundary:** Maximum single dimension is 8. A 9×8 matrix (72 elements) is REJECTED even though 8×9 would be valid. The per-dimension limit M,N ≤ 8 is stricter than the total element limit M×N ≤ 64.
>
> **Dimension Enforcement (CRIT-NEW-1):** Matrices MUST have M ≥ 1 and N ≥ 1. Empty matrices (0×N or M×0) are REJECTED. This prevents out-of-bounds access to `data[0]` in validation phases and ensures deterministic TRAP behavior across implementations.
>
> **Rationale:** The M×N ≤ 64 limit ensures worst-case gas stays within measurable bounds for debuggable execution. The M,N ≤ 8 per-dimension limit prevents pathological 1×64 or 64×1 matrices that could cause issues in certain algorithms. The M,N ≥ 1 requirement prevents empty matrix edge cases that would cause OOB access during validation.

## Core Operations

### MAT_ADD — Matrix Addition

```


mat_add(a: &DMat<T>, b: &DMat<T>) -> DMat<T>
where
    T: NumericScalar<Scalar = T>,

Preconditions:

- a.rows == b.rows
- a.cols == b.cols
- a.rows \* a.cols <= MAX_DMAT_ELEMENTS (64)
- **a.rows <= 8 and a.cols <= 8** (HIGH-6: explicit per-dimension limits)
- All elements in a have same scale as a.data[0]
- All elements in b have same scale as b.data[0]
- a.data[0].scale() == b.data[0].scale() // Scale must match

**Phase 0: TRAP Sentinel Pre-check (CRIT-4)**

```
For each element e in a.data:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
For each element e in b.data:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
```

**Phase 1: Validate dimensions (CRIT-3: prevents OOB access to .data[0])**

```
if a.rows != b.rows or a.cols != b.cols: TRAP(DIMENSION_MISMATCH)
if a.rows * a.cols > MAX_DMAT_ELEMENTS (64): TRAP(DIMENSION_ERROR)
if a.rows > 8 or a.cols > 8: TRAP(DIMENSION_ERROR)
if a.rows < 1 or a.cols < 1: TRAP(DIMENSION_ERROR)  // CRIT-NEW-1: empty matrix
```

**Phase 2: Validate all element scales BEFORE computation (MEDIUM-5)**

```
For i in 0..a.rows:
  For j in 0..a.cols:
    if a.data[i * a.cols + j].scale() != a.data[0].scale(): TRAP(SCALE_MISMATCH)
    if b.data[i * b.cols + j].scale() != b.data[0].scale(): TRAP(SCALE_MISMATCH)
// Cross-matrix scale check:
if a.data[0].scale() != b.data[0].scale(): TRAP(SCALE_MISMATCH)
```

**Phase 3: Compute**

```
For i in 0..a.rows:
  For j in 0..a.cols:
    result.data[i * result.cols + j] = a.data[i * a.cols + j].add(&b.data[i * b.cols + j])?

Return result
```

```

### MAT_SUB — Matrix Subtraction

```


mat_sub(a: &DMat<T>, b: &DMat<T>) -> DMat<T>
where
    T: NumericScalar<Scalar = T>,

Preconditions:

- a.rows == b.rows
- a.cols == b.cols
- a.rows \* a.cols <= MAX_DMAT_ELEMENTS (64)
- **a.rows <= 8 and a.cols <= 8** (HIGH-6: explicit per-dimension limits)
- All elements in a have same scale as a.data[0]
- All elements in b have same scale as b.data[0]
- a.data[0].scale() == b.data[0].scale() // Scale must match

**Phase 0: TRAP Sentinel Pre-check (CRIT-4)**

```
For each element e in a.data:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
For each element e in b.data:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
```

**Phase 1: Validate dimensions (CRIT-3: prevents OOB access to .data[0])**

```
if a.rows != b.rows or a.cols != b.cols: TRAP(DIMENSION_MISMATCH)
if a.rows * a.cols > MAX_DMAT_ELEMENTS (64): TRAP(DIMENSION_ERROR)
if a.rows > 8 or a.cols > 8: TRAP(DIMENSION_ERROR)
if a.rows < 1 or a.cols < 1: TRAP(DIMENSION_ERROR)  // CRIT-NEW-1: empty matrix
```

**Phase 2: Validate all element scales BEFORE computation (MEDIUM-5)**

```
For i in 0..a.rows:
  For j in 0..a.cols:
    if a.data[i * a.cols + j].scale() != a.data[0].scale(): TRAP(SCALE_MISMATCH)
    if b.data[i * b.cols + j].scale() != b.data[0].scale(): TRAP(SCALE_MISMATCH)
// Cross-matrix scale check:
if a.data[0].scale() != b.data[0].scale(): TRAP(SCALE_MISMATCH)
```

**Phase 3: Compute**

```
For i in 0..a.rows:
  For j in 0..a.cols:
    result.data[i * result.cols + j] = a.data[i * a.cols + j].sub(&b.data[i * b.cols + j])?

Return result
```

```

### MAT_MUL — Matrix Multiplication

```


mat_mul(a: &DMat<T>, b: &DMat<T>) -> DMat<T>
where
    T: NumericScalar<Scalar = T>,

> ⚠️ **REQUIREMENT**: Naive triple loop algorithm ONLY. No Strassen, no blocking.

**Phase 0: TRAP Sentinel Pre-check (CRIT-4)**

```
For each element e in a.data:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
For each element e in b.data:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
```

**Phase 1: Validate dimension preconditions (CRIT-3: prevents OOB access)**

```
1. if a.cols != b.rows: TRAP(DIMENSION_MISMATCH)
2. if a.rows * b.cols > MAX_DMAT_ELEMENTS (64): TRAP(DIMENSION_ERROR)
3. if a.rows > 8 or a.cols > 8 or b.rows > 8 or b.cols > 8: TRAP(DIMENSION_ERROR)
4. if a.rows < 1 or a.cols < 1 or b.rows < 1 or b.cols < 1: TRAP(DIMENSION_ERROR)  // CRIT-NEW-1: empty matrix
```

**Phase 2: Validate element scales (CRIT-3: SCALE_MISMATCH before INVALID_SCALE)**

```
For i in 0..a.rows:
  For k in 0..a.cols:
    if a.data[i * a.cols + k].scale() != a.data[0].scale(): TRAP(SCALE_MISMATCH)
For k in 0..b.rows:
  For j in 0..b.cols:
    if b.data[k * b.cols + j].scale() != b.data[0].scale(): TRAP(SCALE_MISMATCH)
```

**Phase 3: Validate result scale (CRIT-3: INVALID_SCALE after SCALE_MISMATCH)**

```
4. result_scale = a.data[0].scale() + b.data[0].scale()
5. if result_scale > T::MAX_SCALE: TRAP(INVALID_SCALE)
```

**Phase 4: Compute with overflow detection (HIGH-1: add Decimal check per RFC-0112)**

```
For i in 0..a.rows:
  For j in 0..b.cols:
    accumulator = BigInt(0)
    For k in 0..a.cols:
      product = a.data[i * a.cols + k].mul(b.data[k * b.cols + j])?
      accumulator = accumulator + BigInt::from(product.raw_mantissa())

    // HIGH-2: Accumulator overflow check uses MAX_MANTISSA constant
    if abs(accumulator) > T::MAX_MANTISSA: TRAP(OVERFLOW)
    result.data[i * result.cols + j] = T::new(accumulator, result_scale)?
```

> ⚠️ **CRITICAL**: Sequential loops only. No SIMD, no parallelization.

### Result Scale

For MAT_MUL(A, B) where A[i][k] has scale s_a and B[k][j] has scale s_b:

- result_scale = s_a + s_b (per RFC-0105 MUL)
- If result_scale > MAX_SCALE (18 for DQA, 36 for Decimal): TRAP(INVALID_SCALE)

**Example (HIGH-6 fix):**
- A[i][k] scale = 4, B[k][j] scale = 5
- product scale = 4 + 5 = 9
- If result mantissa is 1000 at scale 9, canonicalization produces 1 at scale 6
- Note: For scale=0 inputs, result_scale=0, so mantissa values are already canonical (no trailing zeros to remove)

### Overflow Detection

Per RFC-0105 I128_ROUNDTRIP:
- Accumulator uses i128 for intermediate computation
- Final overflow check uses `MAX_MANTISSA` constant: `if abs(accumulator) > T::MAX_MANTISSA: TRAP(OVERFLOW)`
- For DQA: `MAX_MANTISSA = i64::MAX = 2^63 - 1`
- For Decimal: `MAX_MANTISSA = MAX_DECIMAL_MANTISSA`

### MAT_VEC_MUL — Matrix-Vector Multiplication

```


mat_vec_mul(a: &DMat<T>, v: &[T]) -> Vec<T>
where
    T: NumericScalar<Scalar = T>,

> **Note (MED-6/HIGH-NEW-2):** Returns `Vec<T>` compatible with RFC-0112 `DVec<T>` data layout. The result length is `a.rows` which is guaranteed ≤ 8 per dimension constraints, satisfying DVec's N ≤ 64 requirement. The function returns `Vec<T>` rather than `DVec<T>` to avoid circular dependency between RFC-0112 and RFC-0113. Users requiring DVec guarantees can wrap the result: `DVec::try_from(result)?` where the TryFrom implementation verifies length ≤ 64 (which is always satisfied for valid DMAT inputs).

**Phase 0: TRAP Sentinel Pre-check (CRIT-4)**

```
For each element e in a.data:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
For each element e in v:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
```

**Phase 1: Validate dimension preconditions (CRIT-3: prevents OOB access)**

```
1. if a.cols != v.len: TRAP(DIMENSION_MISMATCH)
2. if a.rows * a.cols > MAX_DMAT_ELEMENTS (64): TRAP(DIMENSION_ERROR)
3. if a.rows > 8 or a.cols > 8: TRAP(DIMENSION_ERROR)
4. if a.rows < 1 or a.cols < 1: TRAP(DIMENSION_ERROR)  // CRIT-NEW-1: empty matrix
```

**Phase 2: Validate matrix element scales (CRIT-3: SCALE_MISMATCH before INVALID_SCALE)**

```
For i in 0..a.rows:
  For j in 0..a.cols:
    if a.data[i * a.cols + j].scale() != a.data[0].scale(): TRAP(SCALE_MISMATCH)
```

**Phase 3: Validate vector element scales (HIGH-NEW-FINAL-1: internal uniformity only)**

```
For j in 0..v.len:
  if v[j].scale() != v[0].scale(): TRAP(SCALE_MISMATCH)
```

> **Note (HIGH-NEW-FINAL-1):** Phase 3 validates internal uniformity of vector `v` only. Unlike the previous version, it does NOT require `v.scale() == a.scale()`. Mixed-scale multiplication is allowed: result_scale = a.scale() + v.scale() per RFC-0105 MUL semantics. Matrix scale validation remains in Phase 2.

**Phase 4: Validate result scale (CRIT-3: INVALID_SCALE after SCALE_MISMATCH)**

```
4. result_scale = a.data[0].scale() + v[0].scale()
5. if result_scale > T::MAX_SCALE: TRAP(INVALID_SCALE)
```

**Phase 5: Compute dot products (HIGH-2: uses MAX_MANTISSA constant)**

⚠️ CRITICAL: Sequential loops only. No SIMD, no parallelization.
⚠️ CRITICAL: Fixed iteration order (i=0,1,2... then j=0,1,2...) per RFC-0112 DOT_PRODUCT.
Deterministic TRAP Location: Overflow TRAP conditions are order-dependent.
Sequential left-to-right accumulation is MANDATORY for consensus safety.

```
For i in 0..a.rows:
  accumulator = BigInt(0)
  For j in 0..a.cols:
    product = a.data[i * a.cols + j].mul(v[j])?
    accumulator = accumulator + BigInt::from(product.raw_mantissa())

  if abs(accumulator) > T::MAX_MANTISSA: TRAP(OVERFLOW)
  result[i] = T::new(accumulator, result_scale)?
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


mat_transpose(a: &DMat<T>) -> DMat<T>
where
    T: NumericScalar<Scalar = T>,

Preconditions:

- a.rows \* a.cols <= MAX_DMAT_ELEMENTS (64)
- a.rows <= 8 and a.cols <= 8 (HIGH-6: explicit per-dimension limits)
- All elements in a have same scale as a.data[0]

**Phase 0: TRAP Sentinel Pre-check (CRIT-4)**

```
For each element e in a.data:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
```

**Phase 1: Validate dimensions (MED-2 fix)**

```
if a.rows * a.cols > MAX_DMAT_ELEMENTS (64): TRAP(DIMENSION_ERROR)
if a.rows > 8 or a.cols > 8: TRAP(DIMENSION_ERROR)
if a.rows < 1 or a.cols < 1: TRAP(DIMENSION_ERROR)  // CRIT-NEW-1: empty matrix
```

**Phase 2: Validate all element scales (MED-6 fix: separate validation from computation)**

```
For i in 0..a.rows:
  For j in 0..a.cols:
    if a.data[i * a.cols + j].scale() != a.data[0].scale(): TRAP(SCALE_MISMATCH)
```

**Phase 3: Compute**

```
result.rows = a.cols
result.cols = a.rows
For i in 0..a.rows:
  For j in 0..a.cols:
    // MED-1: Inputs guaranteed canonical per RFC-0111 Lazy Canonicalization.
    // Transpose preserves canonicality (no value change).
    result.data[j * result.cols + i] = a.data[i * a.cols + j].clone()
Return result
```

Note: Transpose does not change element values or scales, only layout.

**Canonicalization (HIGH-4):** Result elements must be in row-major order with canonical representation (no padding, no special formatting).

```


### MAT_SCALE — Matrix Scale

```


mat_scale(a: &DMat<T>, scalar: T) -> DMat<T>
where
    T: NumericScalar<Scalar = T>,

Preconditions:

- a.rows \* a.cols <= MAX_DMAT_ELEMENTS (64)
- **a.rows <= 8 and a.cols <= 8** (HIGH-2: explicit per-dimension limits)
- All elements in a have same scale as a.data[0]
- **scalar.scale() <= T::MAX_SCALE** (HIGH-2: validate scalar scale directly)
- For DQA: a.data[0].scale() + scalar.scale() <= 18
- For Decimal: a.data[0].scale() + scalar.scale() <= 36

**Phase 0: TRAP Sentinel Pre-check (CRIT-4)**

```
For each element e in a.data:
  if e.scale() == 0xFF and e.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
if scalar.scale() == 0xFF and scalar.raw_mantissa() == i64::MIN as i128: TRAP(TRAP_INPUT_ERROR)
```

**Phase 1: Validate dimensions (HIGH-3: explicit dimension pre-validation)**

```
if a.rows * a.cols > MAX_DMAT_ELEMENTS (64): TRAP(DIMENSION_ERROR)
if a.rows > 8 or a.cols > 8: TRAP(DIMENSION_ERROR)
if a.rows < 1 or a.cols < 1: TRAP(DIMENSION_ERROR)  // CRIT-NEW-1: empty matrix
```

**Phase 2: Validate all element scales BEFORE computation (MED-3/HIGH-1 fix: separate validation from computation, result_scale check outside loop)**

```
For i in 0..a.rows:
  For j in 0..a.cols:
    if a.data[i * a.cols + j].scale() != a.data[0].scale(): TRAP(SCALE_MISMATCH)
// Validate result scale once (after all scale mismatch checks, before any computation)
result_scale = a.data[0].scale() + scalar.scale()
if result_scale > T::MAX_SCALE: TRAP(INVALID_SCALE)
```

**Phase 3: Compute (MED-3 fix: no validation in compute phase)**

```
For i in 0..a.rows:
  For j in 0..a.cols:
    result.data[i * result.cols + j] = a.data[i * a.cols + j].mul(&scalar)?
```

```


## Gas Model

Gas derivation follows RFC-0105 where:

- DQA MUL: `20 + 3 × scale_a × scale_b` gas (per RFC-0105 MUL operation)
- DQA ADD: `10` gas flat (per RFC-0105 ADD operation - no scale factor)

### Per-Operation Gas

| Operation     | Type        | Formula                                              | Derivation                                                         |
| ------------- | ----------- | ---------------------------------------------------- | ------------------------------------------------------------------ |
| MAT_ADD       | DQA/Decimal | `10 × M × N`                                         | M×N × RFC-0105 ADD (10 gas flat)                                 |
| MAT_SUB       | DQA/Decimal | `10 × M × N`                                         | M×N × RFC-0105 SUB (10 gas flat)                                 |
| MAT_MUL       | DQA/Decimal | `M × N × K × (30 + 3 × s_a × s_b)`                  | Per MAC: DQA MUL (20 + 3×s_a×s_b) + DQA ADD (10)                 |
| MAT_VEC_MUL   | DQA/Decimal | `rows × cols × (30 + 3 × s_a × s_v)`                | rows dot products, each cols elements                              |
| MAT_TRANSPOSE | DQA/Decimal | `2 × M × N`                                         | M×N reads + M×N writes (1 gas per memory operation)             |
| MAT_SCALE     | DQA/Decimal | `M × N × (20 + 3 × s_a × s_scalar)`                 | M×N × RFC-0105 MUL (20 + 3×s_a×s_scalar)                         |

Where `s_a` is the scale of matrix A and `s_b` is the scale of matrix B (for MAT_MUL), or `s_v` is the scale of vector V (for MAT_VEC_MUL).

**Note:** Gas formula applies to both DQA and Decimal. Decimal operations have slightly higher
actual cost due to i128 arithmetic, but difference is absorbed into base cost for simplicity.
See RFC-0112 §Gas Model for derivation.

### Gas Notes

- **MAT_MUL formula:** `M × N × K × (30 + 3 × s_a × s_b)` combines DQA MUL cost (20 + 3×s_a×s_b) + DQA ADD cost (10 flat) per MAC
- **MAT_TRANSPOSE formula (MED-NEW-2):** `2 × M × N` accounts for M×N element reads plus M×N element writes. Each memory operation (read or write) is counted as 1 gas unit, so 2×M×N represents the total memory bandwidth cost. This is lower than MAT_ADD (10×M×N) because transpose involves no arithmetic operations, only index remapping.
- **Scale check overhead:** The two SCALE_MISMATCH checks per element are O(1) and absorbed into the base cost
- **Per-block budget:** 139,776 gas exceeds 50k consensus budget; MAT_MUL is limited to M×N ≤ 64
- **BigInt overhead (MED-6):** Per RFC-0112, BigInt operations have additional overhead for overflow detection. The `fits_in_i64()` check adds constant O(1) overhead per product and per accumulator.

### Gas Examples (scale=0, DQA)

| Operation     | Dimensions | Gas |
| ------------- | ---------- | --- |
| MAT_ADD       | 8×8        | 640  |
| MAT_MUL       | 4×4 × 4×4  | 1920 |
| MAT_VEC_MUL   | 4×4 × 4    | 480  |
| MAT_TRANSPOSE | 8×8        | 128  |
| MAT_SCALE     | 8×8        | 1280 |

### Per-Block Budget (MED-8 fix)

MAT_MUL at MAX_DMAT_ELEMENTS (8×8=64) with K=8 and scale=9:

- Per dot product: K × (30 + 3 × s_a × s_b) = 8 × (30 + 3 × 9 × 9) = 8 × 273 = 2184
- Total: M × N × 2184 = 8 × 8 × 2184 = 139,776

> **Note:** This example shows worst-case gas (139,776) which exceeds typical 50k consensus budgets. The size limit M×N ≤ 64 limits maximum matrix size, but actual gas consumption depends on K and scale. Implementations may need additional batching or gas metering strategies for very large multiplications.

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
| [[10, 20], [30, 40]] | [[10, 20], [30, 40]] | 0     | [[1400, 2200], [3000, 4600]] (pre-canonical) | Canonical form: see note |

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

- `op_id`: 8-byte operation identifier, big-endian encoding of 16-bit value (LOW-4: 0x0100 = MAT_ADD stored as 0x0000000000000100)
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

> **Note (LOW-4):** These op_ids are in the DMAT namespace (0x01xx). RFC-0111 DVEC uses a separate namespace (0x00xx). Combined probe verifiers should use the type_id to dispatch to the correct namespace.

### TRAP Sentinel Definition

```

TRAP = { mantissa: -(1 << 63), scale: 0xFF }  # i64::MIN as signed integer

```

### Published Merkle Root

> **Merkle Root:** `3e83ddff2c07dc9eafef3e52ed2f3c6ac3363c90099745174721891d22dceaf6` (v1.20 - 63 entries, TRAP last-index probe added)

> **Probe Indexing Rule (CRITICAL):** Probe entries are **zero-indexed**. Previous version contained entries [0..58]. Entries [59..62] were added in v1.19 and v1.20. Total entries: 63. Independent implementations MUST use zero-indexed entries to reproduce the Merkle root.

> **Note (v1.20 entries):**
> - Entry 59: MAT_SCALE canonicalization (1000×10⁻³ → 1×10⁰)
> - Entry 60: MAT_MUL at MAX_SCALE boundary (result_scale=18, valid)
> - Entry 61: TRAP propagation chain (2×2, TRAP at [0][0])
> - Entry 62: TRAP at last index [1][1] — forces full traversal

### Probe Entry Details

> **Canonical Reference:** The script `scripts/compute_dmat_probe_root.py` is the authoritative source for all 63 probe entries (zero-indexed). The Merkle root above is computed from this script.
>
> See §Appendix B for the reference script.

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
root = MerkleRoot(leaf[0], leaf[1], ..., leaf[N-1]) where N = total entries

```

### Verification Procedure

1. For each probe entry, serialize inputs using canonical format
2. Execute operation per algorithms in this RFC
3. Serialize result using canonical format
4. Compute leaf hash: SHA256(leaf_input)
5. Build Merkle tree from 63 leaves
6. Verify root matches published Merkle root

> **Invariant (CRITICAL):** The number of probe entries MUST equal the number of Merkle leaves. Any future addition of probe entries MUST update both the Merkle root and the leaf count in this procedure.

> **Probe Indexing:** All probe entries are zero-indexed [0..61]. Entry 59 refers to the 60th entry.

## Determinism Rules

1. **Naive Algorithm Only**: No Strassen, no blocking optimization
2. **Sequential Loops**: No SIMD, no parallelization
3. **Row-Major Layout**: Must match specification
4. **Dimension Enforcement**: M×N ≤ 64 AND M,N ≤ 8 AND M,N ≥ 1 for execution
5. **Scale Matching**: All elements in a matrix must have the same scale
6. **Type Isolation**: No mixed-type operations (DMAT<DQA> vs DMAT<Decimal>)
7. **Accumulation Semantics (CRITICAL)**: Intermediate accumulation MUST be performed in the exact sequence defined by the loop structure, strictly left-to-right per inner loop index. Implementations MUST NOT restructure accumulation (e.g., buffering, reordering, tree reduction, or delayed writes).

## Algebraic Properties (Informative)

The following properties hold under valid (non-TRAP) execution:

- MAT_ADD is commutative and associative (given identical scale)
- MAT_MUL is associative but NOT commutative
- MAT_TRANSPOSE is involutive: transpose(transpose(A)) = A
- MAT_SCALE distributes over MAT_ADD

> **Note:** These properties are not enforced at runtime but may be used for testing and verification purposes.

## TRAP Codes

| Code               | Condition                                                       | Reference | Global Index |
| ------------------ | --------------------------------------------------------------- | --------- |--------------|
| TRAP_INPUT_ERROR   | Input contains TRAP sentinel                                   | RFC-0113  | TBD          |
| OVERFLOW           | i128 accumulator exceeds i64 range for DQA, or i128 for Decimal | RFC-0105  | TBD          |
| INVALID_SCALE      | Result scale exceeds MAX_SCALE (18 DQA, 36 Decimal)             | RFC-0105  | TBD          |
| SCALE_MISMATCH     | Matrix/vector elements have different scales                    | RFC-0105  | TBD          |
| DIMENSION_ERROR    | Matrix dimensions M×N > 64, M,N > 8, or M,N < 1                | RFC-0113  | TBD          |
| DIMENSION_MISMATCH | Matrix dimensions incompatible for operation                    | RFC-0113  | TBD          |

> **Note (MED-NEW-1):** `DIMENSION_ERROR` and `DIMENSION_MISMATCH` require global error code indices assigned from the unified error registry (per RFC-01XX). The "TBD" indices will be finalized when the global error registry RFC is approved. DMAT-specific codes use the DMAT namespace (0x01xx) for operation IDs, but error codes should align with the global system.

> **Note (MED-7/HIGH-2):** `CANNOT_NORMALIZE_ZERO_VECTOR`, `CONSENSUS_RESTRICTION`, `UNSUPPORTED_OPERATION`, and `INPUT_VALIDATION_ERROR` are defined in other RFCs but are NOT raised by DMAT operations. DMAT's input scale validation is handled entirely by SCALE_MISMATCH (Phase 2) and INVALID_SCALE (Phase 3) per the phase ordering above.

### Global TRAP Invariant (CRITICAL)

All DMAT operations MUST enforce the following invariant:

1. TRAP sentinel detection **MUST** occur before any other validation or computation step.
2. TRAP detection **MUST** iterate elements in strict row-major order using index `(i * cols + j)`. Implementations **MUST NOT** reorder, parallelize, or short-circuit traversal in a way that changes which TRAP is detected first.
3. If any input element matches the TRAP sentinel, the operation MUST immediately return `TRAP(TRAP_INPUT_ERROR)`.
4. No further validation (dimension, scale, overflow) may be performed after TRAP detection.

This rule applies uniformly to ALL operations: MAT_ADD, MAT_SUB, MAT_MUL, MAT_VEC_MUL, MAT_TRANSPOSE, MAT_SCALE.

> **Rationale:** TRAP propagation must be globally consistent and independent of operation-specific logic. This ensures deterministic failure behavior across implementations.

### TRAP Priority Order

When multiple error conditions exist in a single operation:

1. **TRAP_INPUT_ERROR** - Input contains TRAP sentinel (checked FIRST per RFC-0112)
2. **DIMENSION_MISMATCH** - Matrix dimensions incompatible for operation (checked first in MAT_MUL)
3. **DIMENSION_ERROR** - Matrix exceeds size limits (M×N > 64, M,N > 8, or M,N < 1)
4. **SCALE_MISMATCH** - Element scales differ
5. **INVALID_SCALE** - Result scale exceeds MAX_SCALE
6. **OVERFLOW** - Accumulator exceeds representable range

> **Rationale:** TRAP sentinel detection is a pre-validation check (malformed input). If TRAP sentinel is present, no further validation occurs (immediate TRAP).

### TRAP Sentinel (for probe encoding)

```

TRAP = { mantissa: -(1 << 63), scale: 0xFF }  # i64::MIN as signed integer

```

Per RFC-0111 v1.20 §Verification Probe (TRAP Sentinel Definition).
Encoding: 24-byte canonical format per RFC-0111 §Canonical Byte Format.

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
