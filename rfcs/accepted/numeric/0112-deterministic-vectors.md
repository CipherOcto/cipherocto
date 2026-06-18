# RFC-0112 (Numeric/Math): Deterministic Vectors (DVEC)

## Status

**Version:** 1.14 (2026-03-19)
**Status:** Accepted
**NUMERIC_SPEC_VERSION:** 1 (per RFC-0110, incremented only when protocol semantics change)

> **Rationale:** NUMERIC_SPEC_VERSION remains at 1 because this RFC does not change the fundamental protocol semantics of any existing numeric types (DFP, DQA, Decimal). DVEC is a new container type that operates on existing numeric types without modifying their encoding, arithmetic, or TRAP semantics. Changes to probe entries or reference implementations do not constitute protocol semantic changes per RFC-0110.

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

> **Cross-RFC Amendment v1.14:**
> - CROSS-1: Updated RFC-0113 relationship — was "Future - not yet drafted", now "Accepted v1.21"
> - CROSS-2: Added note on DOT_PRODUCT vs MAT_VEC_MUL input scale guard discrepancy

> **Cross-RFC Amendment v1.13:**
> - Added NumericScalar trait version note in §Type System — clarifies RFC-0113 supersedes the trait definition in this RFC
> - Added implementation reference to RFC-0105 Dqa as canonical NumericScalar implementation

> **Adversarial Review v1.12 Changes (Round 5):**
> - CRIT-1 (R5): Updated §Published Merkle Root from stale v1.11 value to new root `74a4c3b44b88bae483ae24b26d04980868a0cc26772b06fe2029c328c1118998`
> - CRIT-2 (R5): Corrected 15 probe expected values (13 non-canonical, 2 wrong math)
>   - Non-canonical: entries 6, 8, 10, 11, 12, 18, 20, 22, 23, 24, 26, 36
>   - Wrong math: entries 15 (220 not 200), 31 (50 not 60), 33 (25 not 29)
> - Updated RFC §Probe Entry Details table to match corrected expected values
> - MED-1: Fixed §Test Vectors §SQUARED_DISTANCE prose table `{29}` → `{25}`
> - MED-2: Added gas derivation footnote for `3 × scale²` term
> - MED-3: Fixed NORMALIZE gas total `~319,000` → `~269,000`
> - LOW-2: Added note explaining VEC_ADD/SUB/MUL entries 48-51 verified by inspection
> - LOW-4: Added note stating canonical script takes precedence over embedded copy

> **Adversarial Review v1.11 Changes (Round 3):**
> - CRIT-NEW-R1: Fixed NORM probe entries 43, 45, 46 with correct RFC-0111 SQRT values
> - CRIT-NEW-R2: Fixed b-vector scale check - now validates all b elements against a[0].scale()
> - CRIT-NEW-R3: Fixed RFC algorithm - Decimal path now uses i128 type annotation
> - CRIT-NEW-R4: Added input_scale > 18 guard to dot_product_decimal
> - MED-NEW-1: Fixed NORM derivation to cite "§SQRT algorithm" instead of "Section X.Y"
> - MED-NEW-R2: Updated entry distribution text to specify "Decimal" for element-wise ops
> - LOW-NEW-R2: Renamed TRAP_SCALE to TRAP_INPUT_SCALE_GUARD for clarity
> - New Merkle root computed: `2f33256f429009e5cf3529ae05f68efd4039105d83d9b6d659a049fbaab76c33`

> **Adversarial Review v1.10 Changes (Round 2):**

> **Adversarial Review v1.9 Changes (Round 9):**
> - CRIT-1: DQA encoding now correctly states sign-extension (was incorrectly documented as zero-extension)
> - CRIT-2: Removed early-exit break from integer_sqrt (fixed-iteration mandate per RFC-0111)
> - CRIT-3: Added raw_mantissa() method to NumericScalar trait for probe serialization
> - CRIT-4: Added scale validation loop to verify all elements have same scale
> - CRIT-5: Changed NORM input scale limit from ≤18 to ≤9 for Decimal (per RFC-0111 SQRT requirements)
> - HIGH-1: Removed unreachable negative-overflow branch in SQUARED_DISTANCE
> - HIGH-6: Added input scale guard to DOT_PRODUCT (≤9 for DQA)
> - MED-1: Added 2 Decimal SQUARED_DISTANCE probe entries
> - MED-2: Added 4 Decimal element-wise probe entries (VEC_ADD/SUB/MUL/SCALE)
> - LOW-4: Fixed gas table to include Type column (DQA/Decimal)
> - LOW-6: Added NUMERIC_SPEC_VERSION declaration
> - LOW-7: Completed implementation checklist
> - New Merkle root computed

> **Adversarial Review v1.8 Changes:**
> - ISSUE-3.1: Unified Merkle root in verification procedure (was old v1.6 root)
> - ISSUE-3.2: Fixed TRAP sentinel reference (Section X → actual section)
> - ISSUE-3.3: Added DQA probe encoding note (24-byte probe vs 16-byte native)
> - ISSUE-1.1: SQRT replaced with RFC-0111 integer Newton-Raphson (deterministic)
> - ISSUE-1.2: All 57 probe entries now unique (no placeholder duplicates)
> - ISSUE-1.3: RFC text inconsistencies fixed (57 entries throughout)
> - ISSUE-1.4: Canonicalization added to all operations
> - ISSUE-1.5: DOT_PRODUCT input scale precondition added (≤9 for DQA)
> - ISSUE-1.6: Entry 56 changed from duplicate NORM TRAP to NORMALIZE consensus TRAP
> - ISSUE-1.7: All "Various" entries replaced with explicit probe values
> - ISSUE-1.8: Python reference implementation added to appendix
> - ISSUE-1.9: DQA 24-byte encoding clarified
> - ISSUE-2.0: TRAP sentinel definition added
> - ISSUE-2.1: Entry 3 fixed: {3, scale=2} → {11, scale=2} (dot product math correction)
> - ISSUE-2.2: New Merkle root computed: `deedbcd8bf9800ffa4b102693f7eb43fcad2c0366af0ff5b6fcd35dd9d55df20`
> - ISSUE-2.3: Entry 56 fixed: NORM DQA → NORMALIZE Decimal (consensus TRAP)
> - ISSUE-2.4: NORMALIZE operation added to Python reference implementation
> - ISSUE-2.5: New Merkle root: `f2255b50e4b887cd97377a39ebf55b761b949d668d640c8424fa6dbb94402238`
> - ISSUE-2.6: Entry 3 comment clarified (raw → canonical explanation)
> - ISSUE-2.7: TRAP sentinel reference added (RFC-0111 v1.20)
> - ISSUE-2.8: DQA zero-extension rationale documented

## Summary

This RFC defines Deterministic Vector (DVEC) operations for consensus-critical vector arithmetic used in similarity search and AI inference.

## Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | DVEC<DFP> is FORBIDDEN (not ZK-friendly) |
| RFC-0105 (DQA) | DVEC<DQA> is the primary type (recommended) |
| RFC-0111 (DECIMAL) | DVEC<DECIMAL> is allowed; required for SQRT ops |
| RFC-0113 (DMAT) | DVEC operations compose with matrix ops (Accepted v1.21) |

> **CROSS-2 Note (Input Scale Guard Discrepancy):** DVEC's `DOT_PRODUCT` enforces an input scale precondition (`a[0].scale() <= 9` for DQA) that rejects high-scale inputs early. DMAT's `MAT_VEC_MUL` does not enforce this precondition — instead, it relies on Phase 4's `result_scale > MAX_SCALE` check. The same logical inputs may produce different TRAP codes depending on which operation is used:
> - `DOT_PRODUCT` with `a.scale()=10, v.scale()=8` → `TRAP(INPUT_VALIDATION_ERROR)` (Phase 1)
> - `MAT_VEC_MUL` with `a.scale()=10, v.scale()=8` → `TRAP(INVALID_SCALE)` (Phase 4)
> This is a known inconsistency — `MAT_VEC_MUL` was designed for matrix-vector composition where per-element scale variance is expected. Implementations should document which path they use for mixed-scale workloads.

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
    fn raw_mantissa(&self) -> i128;
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
}
```

> **Trait Version Note (RFC-0113):** The `NumericScalar` trait defined in this RFC (v1.12) was the **original** definition. RFC-0113 (Deterministic Matrices) defines the **canonical** trait version with additional members (`const MAX_MANTISSA` and `fn new(mantissa: i128, scale: u8) -> Self`). For consensus-critical DMAT operations, implementations **MUST** use the RFC-0113 trait version. See RFC-0113 §Trait Version Enforcement.

> **Implementation Reference:** The concrete `Dqa` type (RFC-0105) provides the canonical implementation of the RFC-0113 `NumericScalar` trait.

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
  1. // Check input scale precondition (must be first check)
     - For Dqa: If a[0].scale() > 9: TRAP (INPUT_VALIDATION_ERROR)
     - For Decimal: If a[0].scale() > 18: TRAP (INPUT_VALIDATION_ERROR)

  2. // Validate all elements in both vectors have the same scale as a[0]
     For i in 0..a.len:
       If a[i].scale() != a[0].scale(): TRAP (SCALE_MISMATCH)
       If b[i].scale() != a[0].scale(): TRAP (SCALE_MISMATCH)

  3. accumulator = BigInt(0)

  4. For i in 0..a.len (sequential order, i=0 then 1 then 2...):
       // Multiply elements (they have same scale)
       product = BigInt::from(a[i].raw_mantissa()) * BigInt::from(b[i].raw_mantissa())
       accumulator = accumulator + product  // BigInt addition

  5. Scale: result_scale = a[0].scale() + b[0].scale()  // Per RFC-0105 MUL semantics

  6. If result_scale > T::MAX_SCALE: TRAP (INVALID_SCALE)

  7. Conversion: Per RFC-0110 I128_ROUNDTRIP semantics:
     - For Dqa:
       - If !accumulator.fits_in_i64(): TRAP (OVERFLOW)
       - value: i64 = accumulator as i64
     - For Decimal:
       - If abs(accumulator) > MAX_DECIMAL_MANTISSA: TRAP (OVERFLOW)
       - value: i128 = accumulator as i128

  8. (value, result_scale) = canonicalize(value, result_scale)

  9. Return T::new(value, result_scale)
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
  1. // Check input scale precondition (must be first check)
     - For Dqa: If a[0].scale() > 9: TRAP (INPUT_VALIDATION_ERROR)
     - For Decimal: If a[0].scale() > 18: TRAP (INPUT_VALIDATION_ERROR)

  2. // Validate all elements in both vectors have the same scale as a[0]
     For i in 0..a.len:
       If a[i].scale() != a[0].scale(): TRAP (SCALE_MISMATCH)
       If b[i].scale() != a[0].scale(): TRAP (SCALE_MISMATCH)

  3. input_scale = a[0].scale()

  4. accumulator = BigInt(0)

  5. For i in 0..a.len (sequential order):
       diff = BigInt::from(a[i].raw_mantissa()) - BigInt::from(b[i].raw_mantissa())
       product = diff * diff
       accumulator = accumulator + product

  6. Scale: result_scale = input_scale * 2

  7. If result_scale > T::MAX_SCALE: TRAP (INVALID_SCALE)

  8. Conversion: Per RFC-0110 I128_ROUNDTRIP semantics:
     - For Dqa:
       - If !accumulator.fits_in_i64(): TRAP (OVERFLOW)
       - value: i64 = accumulator as i64
     - For Decimal:
       - If abs(accumulator) > MAX_DECIMAL_MANTISSA: TRAP (OVERFLOW)
       - value: i128 = accumulator as i128

  9. (value, result_scale) = canonicalize(value, result_scale)

 10. Return T::new(value, result_scale)
```

### NORM — L2 Norm

```
fn norm<T: NumericScalar + MaxScale>(a: &[T]) -> Result<T, Error>

> ⚠️ **DEPRECATED for consensus**: Use SQUARED_DISTANCE instead. Only use NORM for UI/display purposes.

Preconditions:
  - For Dqa: TRAP (UNSUPPORTED_OPERATION - DQA lacks SQRT per RFC-0105)
  - For Decimal: a[0].scale <= 9 (required for SQRT per RFC-0111 v1.20)
    - **Derivation:** input_scale <= 9 is a design constraint:
      1. dot(a,a) has scale = 2 × input_scale
      2. RFC-0111 v1.20 §SQRT algorithm produces result at scale P = min(36, dot_scale + 6)
      3. For input_scale = 9: dot_scale = 18, P = 24 (fits in DECIMAL)
      4. For input_scale > 9: result scale grows beyond 24, increasing precision requirements
      5. The limit aligns NORM output precision with practical embedding use cases

Algorithm:
  1. If T is Dqa: TRAP(UNSUPPORTED_OPERATION)
  2. dot = dot_product(a, a)?
  3. result = sqrt_rfc0111(dot)  // Per RFC-0111 v1.20: P = min(36, scale+6), scale_factor = 2*P - scale
  4. Return result.canonicalize()

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

> **Rationale**: NORMALIZE requires NORM gas (17,752) plus N divisions:
> - At max Decimal scale (36): N × GAS_DIV = 64 × 3,938 = 251,000
> - Total: 17,752 + 251,000 ≈ 269,000
> This exceeds the per-block numeric budget of 50,000 gas defined in RFC-0110/0111. Use SQUARED_DISTANCE for consensus-critical similarity ranking.

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

> **Probe Serialization Note:** For VEC_SCALE, input_b contains a single-element vector representing the scalar. The probe encoding format follows the standard DVEC encoding: len (1 byte) + scalar element (24 bytes). Entries 48–51 (VEC_ADD/SUB/MUL/SCALE) commit to constant expected values verified by direct arithmetic inspection (e.g., 1+3=4, 4−1=3, 2×4=8, 1×2=2 with scale=0).
```

## Gas Model

| Operation | Type | Gas Formula | Max (N=64, scale=9) |
|-----------|------|-------------|---------------------|
| DOT_PRODUCT | DQA | N × (30 + 3 × scale²) | 17,472 |
| DOT_PRODUCT | Decimal | N × (30 + 3 × scale²) | 17,472 |
| SQUARED_DISTANCE | DQA | N × (30 + 3 × scale²) + 10 | 17,482 |
| SQUARED_DISTANCE | Decimal | N × (30 + 3 × scale²) + 10 | 17,482 |
| NORM | Decimal | DOT_PRODUCT + GAS_SQRT | 17,752 (SQRT=280 per RFC-0111) |
| NORMALIZE | Decimal | **FORBIDDEN IN CONSENSUS** | TRAP(CONSENSUS_RESTRICTION) |
| VEC_ADD | DQA/Decimal | 5 × N | 320 |
| VEC_SUB | DQA/Decimal | 5 × N | 320 |
| VEC_MUL | DQA/Decimal | 5 × N | 320 |
| VEC_SCALE | DQA/Decimal | 5 × N | 320 |

> **Note:** GAS_SQRT = 280 (max per RFC-0111, formula: `100 + 5 * scale`, max scale 36).
>
> **Consensus Restriction:** NORMALIZE is FORBIDDEN in consensus because it exceeds the 50,000 per-block numeric gas budget. Use SQUARED_DISTANCE for similarity ranking.
>
> **BigInt Overhead:** DOT_PRODUCT formula `N × (30 + 3 × scale²)` accounts for scalar MUL/ADD. BigInt accumulator overhead (~12 gas per iteration) is absorbed into the base cost (30). For N=64, total BigInt overhead ≈ 768 gas, which is <5% of total cost.
>
> **Derivation of `3 × scale²` term:** Per RFC-0105 §Gas Model, DQA MUL costs `20 + 3 × scale_a × scale_b` gas. For DOT_PRODUCT where `scale_a = scale_b = input_scale`, per-element MUL cost is `20 + 3 × scale²`. Adding BigInt accumulator cost (~10 gas per ADD): per-element total = `30 + 3 × scale²`.

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
| [1, 2] | [4, 6] | {25, scale=0} | (4-1)²+(6-2)²=9+16=25 |
| [1.5, 2.5] | [1.5, 2.5] | {0, scale=0} | Identical |
| [1.5e10, 2.5e10] | [1.5e10, 2.5e10] | TRAP | scale=10 → result scale=20 > 18 |

### NORM

| Input | Type | Expected | Notes |
|-------|------|----------|-------|
| [3, 4] | Decimal | {5, scale=0} | 3-4-5 triangle |
| [0, 0, 0] | Decimal | {0, scale=0} | Zero vector |
| [1, 1, 1] | Decimal | {173205, scale=5} | √3 ≈ 1.73205, canonical form |
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
leaf_input = op_id (8 bytes) || type_id (1 byte) || vector_a_len (1 byte) || vector_a_elements... || vector_b_len (1 byte) || vector_b_elements... || result_len (1 byte) || result_elements...
```

> **CRIT-1 Fix:** The `type_id` byte distinguishes between numeric types:
> - `1` = DQA (Deterministic Quantized Arithmetic)
> - `2` = Decimal (per RFC-0111)
>
> This ensures DQA and Decimal entries with identical values produce distinct leaf hashes.

> **Note:** Probe entries 48–51 (VEC_ADD, VEC_SUB, VEC_MUL, VEC_SCALE) commit to constant expected values trivially verifiable by inspection.

Where each scalar element is serialized as 24 bytes (mantissa + scale):

**For DQA (per RFC-0105):**
```
element = version (1 byte = 0x01) || reserved (3 bytes = 0x00) || scale (1 byte) || reserved (3 bytes = 0x00) || mantissa (16 bytes, big-endian i128)
```

**For DECIMAL (per RFC-0111):**
```
element = version (1 byte = 0x01) || reserved (3 bytes = 0x00) || scale (1 byte) || reserved (3 bytes = 0x00) || mantissa (16 bytes, big-endian i128)
```

> **Note:** Variable-length vectors require explicit length prefix. N is fixed per probe entry definition. All scalars use 24-byte canonical big-endian format for probe consistency.

> **DQA Note:** DQA values are promoted to 24-byte RFC-0111 format for **probe serialization only** (mantissa sign-extended to i128). This ensures uniform leaf format across numeric types for Merkle tree computation. **Note:** Native DQA encoding per RFC-0105 is 16 bytes total (i64 mantissa + scale + reserved). The 24-byte format is probe-specific and not the on-wire or storage format.
>
> **Sign-Extension Rationale:** When encoding DQA's 64-bit mantissa into the 128-bit slot, the upper 64 bits are sign-extended (duplicate the sign bit). This matches two's complement representation semantics and ensures the probe encoding correctly represents negative DQA values in the 128-bit slot for deterministic Merkle tree construction.

#### TRAP Sentinel Definition

For TRAP entries, the result is encoded as a sentinel value:
```
TRAP = { mantissa: 0x8000000000000000 (i64 min), scale: 0xFF }
```

This sentinel is encoded using the same 24-byte format as normal values, with mantissa set to the minimum i64 value (signifying error) and scale set to 0xFF (255) as the error indicator.

> **Reference:** See RFC-0111 v1.20 Section 13.3 (Verification Probe) for the canonical definition.

### Merkle Tree Structure (57 Entries)

- **Entry Count:** 57 (matching RFC-0111)
- Each probe entry is a **Merkle tree leaf**: `SHA256(leaf_input)` = 32 bytes
- The Merkle root commits to all 57 entries

**Entry Distribution:**
- Entries 0-15: DOT_PRODUCT DQA (various N, scales)
- Entries 16-31: DOT_PRODUCT Decimal (various N, scales)
- Entries 32-39: SQUARED_DISTANCE (DQA/Decimal)
- Entries 40-47: NORM (Decimal + DQA TRAPs)
- Entries 48-51: Element-wise Decimal ADD/SUB/MUL/SCALE (DQA element-wise ops not separately probed)
- Entries 52-56: TRAP cases (overflow, scale, dimension)

### Published Merkle Root

> **Merkle Root:** `74a4c3b44b88bae483ae24b26d04980868a0cc26772b06fe2029c328c1118998`

This root was computed from the reference Python implementation in `scripts/compute_dvec_probe_root.py`.

### Probe Entry Details

| Entry | Operation | Type | Input A | Input B | Expected Result |
|-------|-----------|------|---------|---------|-----------------|
| 0 | DOT_PRODUCT | DQA | [1,2,3] | [4,5,6] | {32, scale=0} |
| 1 | DOT_PRODUCT | DQA | [1,2] scale=1 | [3,4] scale=1 | {11, scale=2} |
| 2 | DOT_PRODUCT | DQA | [0,0,0] | [1,2,3] | {0, scale=0} |
| 3 | DOT_PRODUCT | DQA | [10,20] scale=2 | [30,40] scale=2 | {11, scale=2} | Raw: 1100→canonical: 11 |
| 4 | DOT_PRODUCT | DQA | [1] | [1] | {1, scale=0} |
| 5 | DOT_PRODUCT | DQA | [3,5] scale=1 | [2,4] scale=1 | {26, scale=2} |
| 6 | DOT_PRODUCT | DQA | [100] scale=2 | [100] scale=2 | {1, scale=0} | Canonical: 10000→1 |
| 7 | DOT_PRODUCT | DQA | [1,2,3] scale=3 | [4,5,6] scale=3 | {32, scale=6} |
| 8 | DOT_PRODUCT | DQA | [10,20] scale=4 | [30,40] scale=4 | {11, scale=6} | Canonical: 1100→11 |
| 9 | DOT_PRODUCT | DQA | [1,1,1,1] scale=5 | [1,1,1,1] scale=5 | {4, scale=10} |
| 10 | DOT_PRODUCT | DQA | [100,200] scale=6 | [300,400] scale=6 | {11, scale=8} | Canonical: 110000→11 |
| 11 | DOT_PRODUCT | DQA | [1,1,1,1,1] scale=7 | [2,2,2,2,2] scale=7 | {1, scale=13} | Canonical: 10→1 |
| 12 | DOT_PRODUCT | DQA | [50,50] scale=8 | [50,50] scale=8 | {5, scale=13} | Canonical: 5000→5 |
| 13 | DOT_PRODUCT | DQA | [1,1,1,1,1,1] scale=9 | [1,1,1,1,1,1] scale=9 | {6, scale=18} |
| 14 | DOT_PRODUCT | DQA | [10,20,30] | [1,2,3] | {140, scale=0} |
| 15 | DOT_PRODUCT | DQA | [5,15,25] scale=1 | [2,4,6] scale=1 | {22, scale=1} | 5×2+15×4+25×6=220 |
| 16 | DOT_PRODUCT | Decimal | [1] | [1] | {1, scale=0} |
| 17 | DOT_PRODUCT | Decimal | [1,2] scale=1 | [3,4] scale=1 | {11, scale=2} |
| 18 | DOT_PRODUCT | Decimal | [100] scale=2 | [100] scale=2 | {1, scale=0} | Canonical: 10000→1 |
| 19 | DOT_PRODUCT | Decimal | [1,2,3] scale=3 | [4,5,6] scale=3 | {32, scale=6} |
| 20 | DOT_PRODUCT | Decimal | [10,20] scale=4 | [30,40] scale=4 | {11, scale=6} | Canonical: 1100→11 |
| 21 | DOT_PRODUCT | Decimal | [1,1,1,1] scale=5 | [1,1,1,1] scale=5 | {4, scale=10} |
| 22 | DOT_PRODUCT | Decimal | [100,200] scale=6 | [300,400] scale=6 | {11, scale=8} | Canonical: 110000→11 |
| 23 | DOT_PRODUCT | Decimal | [1,1,1,1,1] scale=7 | [2,2,2,2,2] scale=7 | {1, scale=13} | Canonical: 10→1 |
| 24 | DOT_PRODUCT | Decimal | [50,50] scale=8 | [50,50] scale=8 | {5, scale=13} | Canonical: 5000→5 |
| 25 | DOT_PRODUCT | Decimal | [1,1,1,1,1,1] scale=9 | [1,1,1,1,1,1] scale=9 | {6, scale=18} |
| 26 | DOT_PRODUCT | Decimal | [10,20] scale=10 | [30,40] scale=10 | {11, scale=18} | Canonical: 1100→11 |
| 27 | DOT_PRODUCT | Decimal | [1,1,1,1,1,1,1,1] scale=12 | [1,1,1,1,1,1,1,1] scale=12 | {8, scale=24} |
| 28 | DOT_PRODUCT | Decimal | [2,3] scale=14 | [4,5] scale=14 | {23, scale=28} |
| 29 | DOT_PRODUCT | Decimal | [5,5,5] scale=16 | [5,5,5] scale=16 | {75, scale=32} |
| 30 | DOT_PRODUCT | Decimal | [1,1] scale=18 | [1,1] scale=18 | {2, scale=36} |
| 31 | DOT_PRODUCT | Decimal | [10,20] | [1,2] | {50, scale=0} | 10×1+20×2=50 |
| 32 | SQUARED_DISTANCE | DQA | [0,0] | [3,4] | {25, scale=0} |
| 33 | SQUARED_DISTANCE | DQA | [1,2] | [4,6] | {25, scale=0} | (4-1)²+(6-2)²=9+16=25 |
| 34 | SQUARED_DISTANCE | DQA | [0,0] scale=1 | [3,4] scale=1 | {25, scale=2} |
| 35 | SQUARED_DISTANCE | DQA | [1,2] scale=2 | [1,2] scale=2 | {0, scale=0} |
| 36 | SQUARED_DISTANCE | DQA | [10,20] scale=3 | [0,0] scale=3 | {5, scale=4} | Canonical: 500→5 |
| 37 | SQUARED_DISTANCE | DQA | [1] scale=4 | [0] scale=4 | {1, scale=8} |
| 38 | SQUARED_DISTANCE | Decimal | [3,4] scale=5 | [0,0] scale=5 | {25, scale=10} |
| 39 | SQUARED_DISTANCE | Decimal | [1,2,3] scale=6 | [0,0,0] scale=6 | {14, scale=12} |
| 40 | NORM | Decimal | [3,4] | - | {5, scale=0} |
| 41 | NORM | Decimal | [0,0,0] | - | {0, scale=0} |
| 42 | NORM | DQA | [3,4] | - | TRAP (UNSUPPORTED) |
| 43 | NORM | Decimal | [1,2] scale=2 | - | {223606797, scale=10} |
| 44 | NORM | Decimal | [6,8] | - | {10, scale=0} |
| 45 | NORM | Decimal | [1] scale=4 | - | {1, scale=4} |
| 46 | NORM | Decimal | [2,2] scale=6 | - | {2828427124746, scale=18} |
| 47 | NORM | DQA | [1,1,1] | - | TRAP (UNSUPPORTED) |
| 48 | VEC_ADD | Decimal | [1,2] | [3,4] | [4,6] |
| 49 | VEC_SUB | Decimal | [4,6] | [1,2] | [3,4] |
| 50 | VEC_MUL | Decimal | [2,3] | [4,5] | [8,15] |
| 51 | VEC_SCALE | Decimal | [1,2] | scalar=2 | [2,4] |
| 52 | DOT_PRODUCT | DQA | N=65 elements | - | TRAP (DIMENSION) |
| 53 | DOT_PRODUCT | DQA | scale=10+10 | - | TRAP (INPUT_VALIDATION_ERROR) |
| 54 | DOT_PRODUCT | DQA | max values | - | TRAP (OVERFLOW) |
| 55 | SQUARED_DISTANCE | DQA | scale=10 input | - | TRAP (INPUT_SCALE) |
| 56 | NORMALIZE | Decimal | [3,4] | - | TRAP (CONSENSUS_RESTRICTION) |

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
6. Verify root matches: `74a4c3b44b88bae483ae24b26d04980868a0cc26772b06fe2029c328c1118998`

> **Note:** The verification probe uses the same Merkle tree structure as RFC-0111 (57 entries) to ensure consistency across the Numeric Tower.

## Determinism Rules

1. **No SIMD**: Sequential loops only
2. **Fixed Iteration Order**: i=0, then 1, then 2...
3. **No Tree Reduction**: Accumulators must be sequential
4. **Overflow Traps**: Must trap on overflow (not wrap)
5. **Scale Matching**: Element scales must match
6. **Type Isolation**: No mixed-type operations (DQA vs Decimal)

## Implementation Checklist

- [x] DVec struct with data: Vec<T: NumericScalar>
- [x] DOT_PRODUCT with BigInt accumulator and overflow TRAP
- [x] SQUARED_DISTANCE with scale constraint (≤9) and overflow TRAP
- [x] NORM (restricted to Decimal, TRAP for DQA)
- [x] NORMALIZE (restricted to Decimal, TRAP for DQA)
- [x] Element-wise ADD/SUB/MUL
- [x] SCALE operation
- [x] Dimension limit enforcement (N ≤ 64)
- [x] Scale matching validation (all elements same scale)
- [x] Overflow detection (BigInt accumulator)
- [x] Gas calculations with corrected formulas
- [x] Test vectors (57 probe entries)
- [x] Verification probe with Merkle tree
- [x] raw_mantissa() method on NumericScalar trait

## Appendix A: Reference Python Implementation

> **Note:** The canonical reference is `scripts/compute_dvec_probe_root.py`. In case of discrepancy, the script file takes precedence over this embedded copy.

The canonical reference script is the only authoritative implementation:

**File:** `scripts/compute_dvec_probe_root.py`

Run with: `python3 scripts/compute_dvec_probe_root.py`

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0110: Deterministic BIGINT
- RFC-0111: Deterministic DECIMAL
- RFC-0113: Deterministic Matrices (Accepted v1.21)
- RFC-0106: Deterministic Numeric Tower (archived)
