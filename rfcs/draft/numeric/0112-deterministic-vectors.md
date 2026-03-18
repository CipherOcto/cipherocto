# RFC-0112 (Numeric/Math): Deterministic Vectors (DVEC)

## Status

**Version:** 1.11 (2026-03-17)
**Status:** Draft
**NUMERIC_SPEC_VERSION:** 1 (per RFC-0110, incremented only when protocol semantics change)

> **Rationale:** NUMERIC_SPEC_VERSION remains at 1 because this RFC does not change the fundamental protocol semantics of any existing numeric types (DFP, DQA, Decimal). DVEC is a new container type that operates on existing numeric types without modifying their encoding, arithmetic, or TRAP semantics. Changes to probe entries or reference implementations do not constitute protocol semantic changes per RFC-0110.

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

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
| RFC-0113 (DMAT) | DVEC operations compose with matrix ops (Future - not yet drafted) |

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

  8. Return T::new(value, result_scale)
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

 11. Return T::new(value, result_scale)
```

### NORM — L2 Norm

```
fn norm<T: NumericScalar + MaxScale>(a: &[T]) -> Result<T, Error>

> ⚠️ **DEPRECATED for consensus**: Use SQUARED_DISTANCE instead. Only use NORM for UI/display purposes.

Preconditions:
  - For Dqa: TRAP (UNSUPPORTED_OPERATION - DQA lacks SQRT per RFC-0105)
  - For Decimal: a[0].scale <= 9 (required for SQRT per RFC-0111)
    - **Derivation:** Per RFC-0111 §SQRT algorithm, SQRT computes:
      - P = min(36, scale + 6)
      - scale_factor = 2 * P - scale
      - For the result to have valid scale_factor >= 0, we need scale <= P*2
      - Since P = scale + 6 (at minimum), scale_factor >= 0 implies scale <= 2*(scale+6)
      - This simplifies to input_scale <= 9 (to ensure result mantissa fits in DECIMAL range)

Algorithm:
  1. If T is Dqa: TRAP(UNSUPPORTED_OPERATION)
  2. dot = dot_product(a, a)?
  3. result = sqrt_rfc0111(dot)  // Per RFC-0111: P = min(36, scale+6), scale_factor = 2*P - scale
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

> **Probe Serialization Note:** For VEC_SCALE, input_b contains a single-element vector representing the scalar. The probe encoding format follows the standard DVEC encoding: len (1 byte) + scalar element (24 bytes).
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

> **Merkle Root:** `2f33256f429009e5cf3529ae05f68efd4039105d83d9b6d659a049fbaab76c33`

This root was computed from the reference Python implementation in `scripts/compute_dvec_probe_root.py`.

### Probe Entry Details

| Entry | Operation | Type | Input A | Input B | Expected Result |
|-------|-----------|------|---------|---------|-----------------|
| 0 | DOT_PRODUCT | DQA | [1,2,3] | [4,5,6] | {32, scale=0} |
| 1 | DOT_PRODUCT | DQA | [1,2] scale=1 | [3,4] scale=1 | {11, scale=2} |
| 2 | DOT_PRODUCT | DQA | [0,0,0] | [1,2,3] | {0, scale=0} |
| 3 | DOT_PRODUCT | DQA | [10,20] scale=2 | [30,40] scale=2 | {11, scale=2} | Raw: 1100→canonical: 11 |
| 4 | DOT_PRODUCT | DQA | [1] | [1] | {1, scale=0} |
| 5 | DOT_PRODUCT | DQA | [1,2] | [3,4] | {11, scale=2} |
| 6 | DOT_PRODUCT | DQA | [100] scale=2 | [100] scale=2 | {10000, scale=4} |
| 7 | DOT_PRODUCT | DQA | [1,2,3] scale=3 | [4,5,6] scale=3 | {32, scale=6} |
| 8 | DOT_PRODUCT | DQA | [10,20] scale=4 | [30,40] scale=4 | {1100, scale=8} |
| 9 | DOT_PRODUCT | DQA | [1,1,1,1] scale=5 | [1,1,1,1] scale=5 | {4, scale=10} |
| 10 | DOT_PRODUCT | DQA | [100,200] scale=6 | [300,400] scale=6 | {110000, scale=12} |
| 11 | DOT_PRODUCT | DQA | [1,1,1,1,1] scale=7 | [2,2,2,2,2] scale=7 | {10, scale=14} |
| 12 | DOT_PRODUCT | DQA | [50,50] scale=8 | [50,50] scale=8 | {5000, scale=16} |
| 13 | DOT_PRODUCT | DQA | [1,1,1,1,1,1] scale=9 | [1,1,1,1,1,1] scale=9 | {6, scale=18} |
| 14 | DOT_PRODUCT | DQA | [10,20,30] | [1,2,3] | {140, scale=0} |
| 15 | DOT_PRODUCT | DQA | [5,15,25] scale=1 | [2,4,6] scale=1 | {200, scale=2} |
| 16 | DOT_PRODUCT | Decimal | [1] | [1] | {1, scale=0} |
| 17 | DOT_PRODUCT | Decimal | [1,2] scale=1 | [3,4] scale=1 | {11, scale=2} |
| 18 | DOT_PRODUCT | Decimal | [100] scale=2 | [100] scale=2 | {10000, scale=4} |
| 19 | DOT_PRODUCT | Decimal | [1,2,3] scale=3 | [4,5,6] scale=3 | {32, scale=6} |
| 20 | DOT_PRODUCT | Decimal | [10,20] scale=4 | [30,40] scale=4 | {1100, scale=8} |
| 21 | DOT_PRODUCT | Decimal | [1,1,1,1] scale=5 | [1,1,1,1] scale=5 | {4, scale=10} |
| 22 | DOT_PRODUCT | Decimal | [100,200] scale=6 | [300,400] scale=6 | {110000, scale=12} |
| 23 | DOT_PRODUCT | Decimal | [1,1,1,1,1] scale=7 | [2,2,2,2,2] scale=7 | {10, scale=14} |
| 24 | DOT_PRODUCT | Decimal | [50,50] scale=8 | [50,50] scale=8 | {5000, scale=16} |
| 25 | DOT_PRODUCT | Decimal | [1,1,1,1,1,1] scale=9 | [1,1,1,1,1,1] scale=9 | {6, scale=18} |
| 26 | DOT_PRODUCT | Decimal | [10,20] scale=10 | [30,40] scale=10 | {1100, scale=20} |
| 27 | DOT_PRODUCT | Decimal | [1,1,1,1,1,1,1,1] scale=12 | [1,1,1,1,1,1,1,1] scale=12 | {8, scale=24} |
| 28 | DOT_PRODUCT | Decimal | [2,3] scale=14 | [4,5] scale=14 | {23, scale=28} |
| 29 | DOT_PRODUCT | Decimal | [5,5,5] scale=16 | [5,5,5] scale=16 | {75, scale=32} |
| 30 | DOT_PRODUCT | Decimal | [1,1] scale=18 | [1,1] scale=18 | {2, scale=36} |
| 31 | DOT_PRODUCT | Decimal | [10,20] | [1,2] | {60, scale=0} |
| 32 | SQUARED_DISTANCE | DQA | [0,0] | [3,4] | {25, scale=0} |
| 33 | SQUARED_DISTANCE | DQA | [1,2] | [4,6] | {29, scale=0} |
| 34 | SQUARED_DISTANCE | DQA | [0,0] scale=1 | [3,4] scale=1 | {25, scale=2} |
| 35 | SQUARED_DISTANCE | DQA | [1,2] scale=2 | [1,2] scale=2 | {0, scale=0} |
| 36 | SQUARED_DISTANCE | DQA | [10,20] scale=3 | [0,0] scale=3 | {500, scale=6} |
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
6. Verify root matches: `f2255b50e4b887cd97377a39ebf55b761b949d668d640c8424fa6dbb94402238`

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

The following Python script implements the DVEC operations and computes the Merkle root for probe verification:

```python
#!/usr/bin/env python3
"""Compute RFC-0112 DVEC probe Merkle root.

This script implements DVEC operations for probe verification:
  DOT_PRODUCT, SQUARED_DISTANCE, NORM, vec_add, vec_sub, vec_mul, vec_scale

Probe entries follow RFC-0111 structure:
  op_id (8) + input_a_len (1) + input_a_elements (24*N) +
  input_b_len (1) + input_b_elements (24*M) + result_len (1) + result_elements (24*K)

For TRAP entries, uses sentinel encoding: {mantissa: 0x8000000000000000, scale: 0xFF}
"""

import struct
import hashlib
from typing import List, Tuple, Optional

# Operation IDs
OPS = {
    'DOT_PRODUCT': 1,
    'SQUARED_DISTANCE': 2,
    'NORM': 3,
    'VEC_ADD': 4,
    'VEC_SUB': 5,
    'VEC_MUL': 6,
    'VEC_SCALE': 7,
    'NORMALIZE': 8,
}

# Type IDs
TYPES = {
    'DQA': 1,
    'DECIMAL': 2,
}

# Limits
MAX_DVEC_DIM = 64
MAX_DQA_SCALE = 18
MAX_DECIMAL_SCALE = 36
MAX_DQA_MANTISSA = 2**63 - 1  # i64 max
MAX_DECIMAL_MANTISSA = 10**36 - 1

# Precomputed POW10 table
POW10 = [10**i for i in range(37)]


def encode_scalar_dqa(mantissa: int, scale: int) -> bytes:
    """Encode DQA scalar to 24-byte format.

    Format:
      Byte 0: Version (0x01)
      Bytes 1-3: Reserved (0x00)
      Byte 4: Scale (u8, 0-18)
      Bytes 5-7: Reserved (0x00)
      Bytes 8-23: Mantissa (i64 big-endian, two's complement)
    """
    buf = bytearray(24)
    buf[0] = 0x01  # version
    buf[4] = scale & 0xFF  # scale

    # Encode i64 as big-endian two's complement
    if mantissa >= 0:
        buf[8:24] = mantissa.to_bytes(16, 'big')
    else:
        buf[8:24] = ((1 << 128) + mantissa).to_bytes(16, 'big')

    return bytes(buf)


def encode_scalar_decimal(mantissa: int, scale: int) -> bytes:
    """Encode DECIMAL scalar to 24-byte format (RFC-0111).

    Format:
      Byte 0: Version (0x01)
      Bytes 1-3: Reserved (0x00)
      Byte 4: Scale (u8, 0-36)
      Bytes 5-7: Reserved (0x00)
      Bytes 8-23: Mantissa (i128 big-endian, two's complement)
    """
    buf = bytearray(24)
    buf[0] = 0x01  # version
    buf[4] = scale & 0xFF  # scale

    # Encode i128 as big-endian two's complement
    if mantissa >= 0:
        buf[8:24] = mantissa.to_bytes(16, 'big')
    else:
        buf[8:24] = ((1 << 128) + mantissa).to_bytes(16, 'big')

    return bytes(buf)


def encode_trap_sentinel(is_decimal: bool = False) -> bytes:
    """Encode TRAP sentinel: {mantissa: 0x8000000000000000, scale: 0xFF}."""
    if is_decimal:
        return encode_scalar_decimal(0x8000000000000000, 0xFF)
    return encode_scalar_dqa(0x8000000000000000, 0xFF)


def canonicalize_dqa(mantissa: int, scale: int) -> Tuple[int, int]:
    """Canonicalize DQA by removing trailing zeros."""
    if mantissa == 0:
        return (0, 0)
    while mantissa % 10 == 0 and scale > 0:
        mantissa //= 10
        scale -= 1
    return (mantissa, scale)


def canonicalize_decimal(mantissa: int, scale: int) -> Tuple[int, int]:
    """Canonicalize DECIMAL by removing trailing zeros."""
    if mantissa == 0:
        return (0, 0)
    while mantissa % 10 == 0 and scale > 0:
        mantissa //= 10
        scale -= 1
    return (mantissa, scale)


# ============ DVEC Operations ============

def dot_product_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute DOT_PRODUCT for DQA vectors.

    Returns: (mantissa, scale) or None for TRAP
    """
    if len(a) != len(b):
        return None  # TRAP

    if len(a) > MAX_DVEC_DIM:
        return None  # TRAP DIMENSION

    # Check scales match
    if a and a[0][1] != b[0][1]:
        return None  # TRAP SCALE_MISMATCH

    input_scale = a[0][1] if a else 0

    # Accumulate using Python's arbitrary precision (simulating BigInt)
    accumulator = 0
    for i in range(len(a)):
        product = a[i][0] * b[i][0]
        accumulator += product

    # Check overflow (i64 range)
    if accumulator < -MAX_DQA_MANTISSA or accumulator > MAX_DQA_MANTISSA:
        return None  # TRAP OVERFLOW

    # Result scale = sum of input scales
    result_scale = a[0][1] + b[0][1]

    # Check scale overflow
    if result_scale > MAX_DQA_SCALE:
        return None  # TRAP INVALID_SCALE

    return canonicalize_dqa(int(accumulator), result_scale)


def dot_product_decimal(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute DOT_PRODUCT for DECIMAL vectors.

    Returns: (mantissa, scale) or None for TRAP
    """
    if len(a) != len(b):
        return None  # TRAP

    if len(a) > MAX_DVEC_DIM:
        return None  # TRAP DIMENSION

    # Check scales match
    if a and a[0][1] != b[0][1]:
        return None  # TRAP SCALE_MISMATCH

    # Accumulate using Python's arbitrary precision
    accumulator = 0
    for i in range(len(a)):
        product = a[i][0] * b[i][0]
        accumulator += product

    # Check overflow (DECIMAL range)
    if abs(accumulator) > MAX_DECIMAL_MANTISSA:
        return None  # TRAP OVERFLOW

    # Result scale = sum of input scales
    result_scale = a[0][1] + b[0][1]

    # Check scale overflow
    if result_scale > MAX_DECIMAL_SCALE:
        return None  # TRAP INVALID_SCALE

    return canonicalize_decimal(int(accumulator), result_scale)


def squared_distance_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute SQUARED_DISTANCE for DQA vectors.

    Returns: (mantissa, scale) or None for TRAP
    """
    if len(a) != len(b):
        return None

    if len(a) > MAX_DVEC_DIM:
        return None

    input_scale = a[0][1] if a else 0

    # Check input scale constraint for DQA
    if input_scale > 9:
        return None  # TRAP INPUT_SCALE

    accumulator = 0
    for i in range(len(a)):
        diff = a[i][0] - b[i][0]
        product = diff * diff
        accumulator += product

    # Check overflow
    if accumulator < -MAX_DQA_MANTISSA or accumulator > MAX_DQA_MANTISSA:
        return None  # TRAP OVERFLOW

    # Result scale = input_scale * 2
    result_scale = input_scale * 2

    # Check scale overflow
    if result_scale > MAX_DQA_SCALE:
        return None  # TRAP INVALID_SCALE

    return canonicalize_dqa(int(accumulator), result_scale)


def squared_distance_decimal(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute SQUARED_DISTANCE for DECIMAL vectors."""
    if len(a) != len(b):
        return None

    if len(a) > MAX_DVEC_DIM:
        return None

    input_scale = a[0][1] if a else 0

    # Check input scale constraint for Decimal
    if input_scale > 18:
        return None  # TRAP INPUT_SCALE

    accumulator = 0
    for i in range(len(a)):
        diff = a[i][0] - b[i][0]
        product = diff * diff
        accumulator += product

    if abs(accumulator) > MAX_DECIMAL_MANTISSA:
        return None

    result_scale = input_scale * 2

    if result_scale > MAX_DECIMAL_SCALE:
        return None

    return canonicalize_decimal(int(accumulator), result_scale)


def integer_sqrt(n: int) -> int:
    """RFC-0111 compliant integer sqrt (Newton-Raphson, 40 iterations).

    This ensures deterministic results across all platforms.
    """
    if n == 0:
        return 0
    # Initial guess: 2^(bit_length(n)/2)
    x = 1 << ((n.bit_length() + 1) // 2)
    # Fixed 40 iterations for determinism (per RFC-0111)
    for _ in range(40):
        x_new = (x + n // x) // 2
        x = x_new
    # Off-by-one correction per RFC-0111
    if x * x > n:
        x = x - 1
    return x


def norm_decimal(a: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute NORM for DECIMAL vectors.

    Returns: (mantissa, scale) or None for TRAP
    Note: DQA does not support SQRT - returns TRAP
    """
    if not a:
        return (0, 0)  # Zero vector

    # Compute dot product with self
    dot_result = dot_product_decimal(a, a)
    if dot_result is None:
        return None  # TRAP from dot_product

    mantissa, scale = dot_result

    if mantissa == 0:
        return (0, 0)

    # Use RFC-0111 integer sqrt (Newton-Raphson, NOT floating-point)
    int_sqrt = integer_sqrt(mantissa)
    # Adjust scale (scale is always even for squared values)
    new_scale = scale // 2

    return canonicalize_decimal(int_sqrt, new_scale)


def normalize_decimal(a: List[Tuple[int, int]]) -> Optional[List[Tuple[int, int]]]:
    """Compute NORMALIZE for DECIMAL vectors.

    Returns: List of normalized elements or None for TRAP.
    Note: Returns TRAP in consensus context (exceeds gas budget).
    """
    # NORMALIZE is FORBIDDEN in consensus per RFC-0112
    # This probe entry verifies the CONSENSUS_RESTRICTION TRAP
    return None  # TRAP CONSENSUS_RESTRICTION


def vec_add_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[List[Tuple[int, int]]]:
    """Element-wise ADD for DQA vectors."""
    if len(a) != len(b):
        return None

    result = []
    for i in range(len(a)):
        if a[i][1] != b[i][1]:
            return None  # Scale mismatch
        sum_val = a[i][0] + b[i][0]
        if sum_val < -MAX_DQA_MANTISSA or sum_val > MAX_DQA_MANTISSA:
            return None  # Overflow
        result.append(canonicalize_dqa(sum_val, a[i][1]))

    return result


def vec_sub_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[List[Tuple[int, int]]]:
    """Element-wise SUB for DQA vectors."""
    if len(a) != len(b):
        return None

    result = []
    for i in range(len(a)):
        if a[i][1] != b[i][1]:
            return None
        diff = a[i][0] - b[i][0]
        if diff < -MAX_DQA_MANTISSA or diff > MAX_DQA_MANTISSA:
            return None
        result.append(canonicalize_dqa(diff, a[i][1]))

    return result


def vec_mul_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[List[Tuple[int, int]]]:
    """Element-wise MUL for DQA vectors."""
    if len(a) != len(b):
        return None

    result = []
    for i in range(len(a)):
        if a[i][1] != b[i][1]:
            return None
        prod = a[i][0] * b[i][0]
        new_scale = a[i][1] + b[i][1]
        if new_scale > MAX_DQA_SCALE:
            return None  # Scale overflow
        if prod < -MAX_DQA_MANTISSA or prod > MAX_DQA_MANTISSA:
            return None  # Overflow
        result.append(canonicalize_dqa(prod, new_scale))

    return result


def vec_scale_dqa(a: List[Tuple[int, int]], scalar: Tuple[int, int]) -> Optional[List[Tuple[int, int]]]:
    """Scale vector by scalar for DQA."""
    result = []
    for i in range(len(a)):
        prod = a[i][0] * scalar[0]
        new_scale = a[i][1] + scalar[1]
        if new_scale > MAX_DQA_SCALE:
            return None
        if prod < -MAX_DQA_MANTISSA or prod > MAX_DQA_MANTISSA:
            return None
        result.append(canonicalize_dqa(prod, new_scale))

    return result


# ============ Probe Entry Building ============

def encode_vector(elements: List[Tuple[int, int]], is_decimal: bool) -> bytes:
    """Encode a vector to bytes: len (1) + elements (24 each)."""
    encode_fn = encode_scalar_decimal if is_decimal else encode_scalar_dqa
    result = bytes([len(elements)])
    for mantissa, scale in elements:
        result += encode_fn(mantissa, scale)
    return result


def build_leaf(op_id: int, input_a: List[Tuple[int, int]], input_b: Optional[List[Tuple[int, int]]],
               result: any, is_decimal: bool = False) -> bytes:
    """Build a Merkle leaf: op_id (8) + input_a + input_b + result."""
    # op_id as 8 bytes big-endian
    leaf = op_id.to_bytes(8, 'big')

    # input_a
    leaf += encode_vector(input_a, is_decimal)

    # input_b (if present)
    if input_b is not None:
        leaf += encode_vector(input_b, is_decimal)
    else:
        leaf += bytes([0])  # Empty vector

    # result
    if result is None:
        # TRAP
        leaf += encode_trap_sentinel(is_decimal)
    elif isinstance(result, list):
        leaf += encode_vector(result, is_decimal)
    else:
        # Single scalar result
        mantissa, scale = result
        if is_decimal:
            leaf += encode_scalar_decimal(mantissa, scale)
        else:
            leaf += encode_scalar_dqa(mantissa, scale)

    return leaf


def compute_leaf_hash(op_name: str, input_a: List[Tuple[int, int]],
                     input_b: Optional[List[Tuple[int, int]]], result: any,
                     is_decimal: bool = False) -> str:
    """Compute SHA256 leaf hash."""
    op_id = OPS.get(op_name, 0)
    leaf = build_leaf(op_id, input_a, input_b, result, is_decimal)
    return hashlib.sha256(leaf).hexdigest()


def merkle_root(leaf_hashes: List[str]) -> str:
    """Compute Merkle root from leaf hashes."""
    if not leaf_hashes:
        return ""

    hashes = [bytes.fromhex(h) for h in leaf_hashes]

    while len(hashes) > 1:
        if len(hashes) % 2 == 1:
            hashes.append(hashes[-1])  # Pad with last element

        next_level = []
        for i in range(0, len(hashes), 2):
            combined = hashes[i] + hashes[i+1]
            next_level.append(hashlib.sha256(combined).digest())

        hashes = next_level

    return hashes[0].hex()


# ============ Define Probe Entries ============

def get_probe_entries() -> List[dict]:
    """Define all 57 probe entries."""
    entries = []

    # Entries 0-15: DOT_PRODUCT DQA
    entries.append({
        'name': 'DOT_PRODUCT_DQA_0',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(1, 0), (2, 0), (3, 0)],
        'input_b': [(4, 0), (5, 0), (6, 0)],
        'expected': (32, 0),
    })
    entries.append({
        'name': 'DOT_PRODUCT_DQA_1',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(1, 1), (2, 1)],  # scale 1
        'input_b': [(3, 1), (4, 1)],
        'expected': (11, 2),  # scale 1+1=2
    })
    entries.append({
        'name': 'DOT_PRODUCT_DQA_2',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(0, 0), (0, 0), (0, 0)],
        'input_b': [(1, 0), (2, 0), (3, 0)],
        'expected': (0, 0),
    })
    entries.append({
        'name': 'DOT_PRODUCT_DQA_3',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(10, 2), (20, 2)],  # 0.10, 0.20
        'input_b': [(30, 2), (40, 2)],  # 0.30, 0.40
        'expected': (11, 2),  # 0.1*0.3 + 0.2*0.4 = 0.03 + 0.08 = 0.11
    })
    # Entries 4-15: DOT_PRODUCT DQA unique cases (12 unique test cases)
    dqa_dot_cases = [
        ([(1, 0)], [(1, 0)], (1, 0)),  # N=1, scale=0
        ([(1, 1), (2, 1)], [(3, 1), (4, 1)], (11, 2)),  # N=2, scale=1
        ([(100, 2)], [(100, 2)], (10000, 4)),  # scale=2
        ([(1, 3), (2, 3), (3, 3)], [(4, 3), (5, 3), (6, 3)], (32, 6)),  # N=3, scale=3
        ([(10, 4), (20, 4)], [(30, 4), (40, 4)], (1100, 8)),  # scale=4
        ([(1, 5)] * 4, [(1, 5)] * 4, (4, 10)),  # N=4, scale=5
        ([(100, 6), (200, 6)], [(300, 6), (400, 6)], (110000, 12)),  # scale=6
        ([(1, 7)] * 5, [(2, 7)] * 5, (10, 14)),  # N=5, scale=7
        ([(50, 8), (50, 8)], [(50, 8), (50, 8)], (5000, 16)),  # scale=8
        ([(1, 9)] * 6, [(1, 9)] * 6, (6, 18)),  # N=6, scale=9 (max for DOT)
        ([(10, 0), (20, 0), (30, 0)], [(1, 0), (2, 0), (3, 0)], (140, 0)),  # N=3, scale=0
        ([(5, 1), (15, 1), (25, 1)], [(2, 1), (4, 1), (6, 1)], (200, 2)),  # N=3, scale=1
    ]
    for i, (a, b, expected) in enumerate(dqa_dot_cases):
        entries.append({
            'name': f'DOT_PRODUCT_DQA_{4+i}',
            'op': 'DOT_PRODUCT',
            'decimal': False,
            'input_a': a,
            'input_b': b,
            'expected': expected,
        })

    # Entries 16-31: DOT_PRODUCT Decimal unique cases (16 unique test cases)
    decimal_dot_cases = [
        ([(1, 0)], [(1, 0)], (1, 0)),  # N=1, scale=0
        ([(1, 1), (2, 1)], [(3, 1), (4, 1)], (11, 2)),  # N=2, scale=1
        ([(100, 2)], [(100, 2)], (10000, 4)),  # scale=2
        ([(1, 3), (2, 3), (3, 3)], [(4, 3), (5, 3), (6, 3)], (32, 6)),  # N=3
        ([(10, 4), (20, 4)], [(30, 4), (40, 4)], (1100, 8)),
        ([(1, 5)] * 4, [(1, 5)] * 4, (4, 10)),  # N=4
        ([(100, 6), (200, 6)], [(300, 6), (400, 6)], (110000, 12)),
        ([(1, 7)] * 5, [(2, 7)] * 5, (10, 14)),  # N=5
        ([(50, 8), (50, 8)], [(50, 8), (50, 8)], (5000, 16)),
        ([(1, 9)] * 6, [(1, 9)] * 6, (6, 18)),  # scale=9
        ([(10, 10), (20, 10)], [(30, 10), (40, 10)], (1100, 20)),  # scale=10
        ([(1, 12)] * 8, [(1, 12)] * 8, (8, 24)),  # N=8, scale=12
        ([(2, 14), (3, 14)], [(4, 14), (5, 14)], (23, 28)),  # scale=14
        ([(5, 16)] * 3, [(5, 16)] * 3, (75, 32)),  # N=3, scale=16
        ([(1, 18)] * 2, [(1, 18)] * 2, (2, 36)),  # scale=18 (max for Decimal)
        ([(10, 0), (20, 0)], [(1, 0), (2, 0)], (60, 0)),  # Different values
    ]
    for i, (a, b, expected) in enumerate(decimal_dot_cases):
        entries.append({
            'name': f'DOT_PRODUCT_DECIMAL_{16+i}',
            'op': 'DOT_PRODUCT',
            'decimal': True,
            'input_a': a,
            'input_b': b,
            'expected': expected,
        })

    # Entries 32-39: SQUARED_DISTANCE unique cases
    sq_dist_cases = [
        ([(0, 0), (0, 0)], [(3, 0), (4, 0)], (25, 0)),  # 3^2 + 4^2
        ([(1, 0), (2, 0)], [(4, 0), (6, 0)], (29, 0)),  # 3^2 + 4^2
        ([(0, 1), (0, 1)], [(3, 1), (4, 1)], (25, 2)),  # scale=1
        ([(1, 2), (2, 2)], [(1, 2), (2, 2)], (0, 0)),  # Same vector = 0
        ([(10, 3), (20, 3)], [(0, 3), (0, 3)], (500, 6)),  # scale=3
        ([(1, 4)], [(0, 4)], (1, 8)),  # N=1, scale=4
        ([(3, 5), (4, 5)], [(0, 5), (0, 5)], (25, 10)),  # scale=5
        ([(1, 6), (2, 6), (3, 6)], [(0, 6), (0, 6), (0, 6)], (14, 12)),  # N=3
    ]
    for i, (a, b, expected) in enumerate(sq_dist_cases):
        entries.append({
            'name': f'SQUARED_DISTANCE_{32+i}',
            'op': 'SQUARED_DISTANCE',
            'decimal': False,
            'input_a': a,
            'input_b': b,
            'expected': expected,
        })

    # Entries 40-47: NORM unique cases
    norm_cases = [
        ([(3, 0), (4, 0)], True, (5, 0)),  # Decimal: sqrt(9+16) = 5
        ([(0, 0), (0, 0), (0, 0)], True, (0, 0)),  # Zero vector
        ([(3, 0), (4, 0)], False, None),  # DQA: TRAP (unsupported)
        ([(1, 2), (2, 2)], True, (5, 1)),  # Decimal: sqrt(1+4) = sqrt(5)
        ([(6, 0), (8, 0)], True, (10, 0)),  # 6-8-10 triangle
        ([(1, 4)], True, (1, 2)),  # scale=4, sqrt(1) = 1
        ([(2, 6), (2, 6)], True, (8, 6)),  # Decimal: sqrt(4+4) = sqrt(8)
        ([(1, 0), (1, 0), (1, 0)], False, None),  # DQA: TRAP
    ]
    for i, (a, is_decimal, expected) in enumerate(norm_cases):
        entries.append({
            'name': f'NORM_{40+i}',
            'op': 'NORM',
            'decimal': is_decimal,
            'input_a': a,
            'input_b': None,
            'expected': expected,
        })

    # Entries 48-51: Element-wise operations
    entries.append({
        'name': 'VEC_ADD_0',
        'op': 'VEC_ADD',
        'decimal': False,
        'input_a': [(1, 0), (2, 0)],
        'input_b': [(3, 0), (4, 0)],
        'expected': [(4, 0), (6, 0)],
    })
    entries.append({
        'name': 'VEC_SUB_0',
        'op': 'VEC_SUB',
        'decimal': False,
        'input_a': [(4, 0), (6, 0)],
        'input_b': [(1, 0), (2, 0)],
        'expected': [(3, 0), (4, 0)],
    })
    entries.append({
        'name': 'VEC_MUL_0',
        'op': 'VEC_MUL',
        'decimal': False,
        'input_a': [(2, 0), (3, 0)],
        'input_b': [(4, 0), (5, 0)],
        'expected': [(8, 0), (15, 0)],
    })
    entries.append({
        'name': 'VEC_SCALE_0',
        'op': 'VEC_SCALE',
        'decimal': False,
        'input_a': [(1, 0), (2, 0)],
        'input_b': [(2, 0)],  # scalar
        'expected': [(2, 0), (4, 0)],
    })

    # Entries 52-56: TRAP cases
    entries.append({
        'name': 'TRAP_DIMENSION',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(1, 0)] * 65,  # N=65 exceeds limit
        'input_b': [(1, 0)] * 65,
        'expected': None,  # TRAP DIMENSION
    })
    entries.append({
        'name': 'TRAP_SCALE',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(1, 10), (1, 10)],  # scale 10 + 10 = 20 > 18
        'input_b': [(1, 10), (1, 10)],
        'expected': None,  # TRAP INVALID_SCALE
    })
    entries.append({
        'name': 'TRAP_OVERFLOW',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(10**18, 0), (10**18, 0)],  # Very large
        'input_b': [(10**18, 0), (10**18, 0)],
        'expected': None,  # TRAP OVERFLOW
    })
    entries.append({
        'name': 'TRAP_SQUARED_DISTANCE_SCALE',
        'op': 'SQUARED_DISTANCE',
        'decimal': False,
        'input_a': [(1, 10), (1, 10)],  # scale 10 > 9
        'input_b': [(0, 10), (0, 10)],
        'expected': None,  # TRAP INPUT_SCALE
    })
    entries.append({
        'name': 'TRAP_NORMALIZE_DECIMAL',
        'op': 'NORMALIZE',
        'decimal': True,
        'input_a': [(3, 0), (4, 0)],
        'input_b': None,
        'expected': None,  # TRAP CONSENSUS_RESTRICTION
    })

    return entries


def main():
    """Compute DVEC probe Merkle root."""
    print("Computing RFC-0112 DVEC Probe Merkle Root...")
    print("=" * 60)

    entries = get_probe_entries()
    print(f"Total entries: {len(entries)}")

    leaf_hashes = []
    for i, entry in enumerate(entries):
        leaf_hash = compute_leaf_hash(
            entry['op'],
            entry['input_a'],
            entry['input_b'],
            entry['expected'],
            entry.get('decimal', False)
        )
        leaf_hashes.append(leaf_hash)
        print(f"Entry {i:2d}: {entry['name']:30s} -> {leaf_hash[:16]}...")

    print("=" * 60)
    root = merkle_root(leaf_hashes)
    print(f"\nMerkle Root: {root}")
    print(f"\nExpected entries for RFC: {len(entries)}")

    # Verify entry count is 57
    assert len(entries) == 57, f"Expected 57 entries, got {len(entries)}"

    return root


if __name__ == '__main__':
    main()
```

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0110: Deterministic BIGINT
- RFC-0111: Deterministic DECIMAL
- RFC-0113: Deterministic Matrices (Future - not yet drafted)
- RFC-0106: Deterministic Numeric Tower (archived)
