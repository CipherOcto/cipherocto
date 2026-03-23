# RFC-0132 (Numeric/Math): DQA to BIGINT Conversion

## Status

**Version:** 1.1 (Draft)
**Status:** Draft
**Depends On:** RFC-0110 (BIGINT), RFC-0105 (DQA)
**Category:** Numeric/Math

## Summary

This RFC specifies the conversion algorithm from DQA (RFC-0105, i64 value with 0-18 decimal scale) to BIGINT (RFC-0110, arbitrary-precision integer up to 4096 bits). This conversion is necessary for the Numeric Tower to support operations that require DQA values to be used in BIGINT contexts, and for explicit CAST expressions between these types.

This conversion always succeeds because DQA's i64 value trivially fits within BIGINT's arbitrary range.

## Motivation

### Problem Statement

DQA provides fixed-precision decimal arithmetic with i64 value and 0-18 scale. BIGINT provides arbitrary-precision integers up to 4096 bits. When a DQA value must be used in a BIGINT context (e.g., arithmetic with BIGINT operands, or explicit CAST), a conversion is required.

Without a rigorous specification:
- Two implementations could convert the same DQA to different BIGINT values
- Scale handling could differ (truncation vs rounding)

### Why This RFC Exists

RFC-0105 defines DQA but does not define DQA→BIGINT conversion. RFC-0110 defines BIGINT but its DQA interop section only covers i128↔DQA (not full Dqa↔BigInt). This RFC fills that gap.

### RFC-0105 and RFC-0110 Coverage Analysis

| Conversion | RFC-0105 (DQA) | RFC-0110 (BIGINT) | This RFC |
|------------|-----------------|-------------------|----------|
| DQA → i128 | Not specified | `dqa_to_bigint()` returns i128 | Not needed |
| DQA → BigInt | Not specified | Not specified | **This RFC** |

**Key insight:** DQA→BIGINT conversion always succeeds because:
1. DQA's i64 value trivially fits in any BigInt (which has arbitrary precision)
2. The scale is simply truncated (BigInt is an integer type)
3. No range checking is needed

## Input/Output Contract

```rust
/// DQA→BIGINT conversion result
/// Note: Unlike most conversions, this ALWAYS succeeds
pub type DqaToBigIntResult = BigInt;

/// Input to the conversion
pub struct DqaToBigIntInput {
    /// The DQA value to convert
    pub value: Dqa,
}

/// Output from the conversion
pub enum DqaToBigIntOutput {
    /// Successfully converted to BigInt
    Success(BigInt),
}
```

**Important:** DQA→BIGINT conversion cannot fail. Any DQA value (including i64::MIN, i64::MAX) fits in BigInt. This is different from BIGINT→DQA which can fail on overflow.

## Scale Context Propagation

The scale in DQA represents decimal places. When converting to BIGINT (an integer type), the scale is **truncated**:

| DQA Value | Scale | BIGINT Output | Rationale |
|-----------|-------|---------------|-----------|
| {42, 0} | 0 | 42 | Integer, no fractional part |
| {42, 2} | 2 | 42 | Scale 2 means 0.42, truncated to 42 |
| {4200, 2} | 2 | 4200 | Scale 2 means 42.00, truncated to 4200 |
| {42000, 3} | 3 | 42000 | Scale 3 means 42.000, truncated to 42000 |

**Truncation vs Rounding:** BIGINT is an integer type, so scale truncation is the only sensible option. Rounding would imply the value is approximate, which contradicts BigInt being exact.

## Constraints

| Constraint Type | Description |
|----------------|-------------|
| **Always succeeds** | Any valid DQA input produces a valid BIGINT output |
| **Scale truncation** | Scale is ignored (not preserved) in BIGINT |
| **Sign preserved** | Negative DQA produces negative BIGINT |
| **Zero canonicalization** | DQA{0, any} → BigInt::zero() |
| **Determinism** | Identical DQA input always produces identical BIGINT output |

## SQL Integration

DQA→BIGINT conversion appears in SQL CAST expressions:

```sql
-- Explicit CAST from DQA to BIGINT
SELECT CAST(dqa_col AS BIGINT) FROM account_balances;

-- This is ALWAYS VALID: Any DQA value fits in BIGINT
-- Dqa{9223372036854775807, 0} → BigInt(9223372036854775807)

-- Scale truncation in action
SELECT CAST(dqa_col AS BIGINT) FROM currency_amounts;
-- Dqa{1999, 2} → BigInt(1999) represents $19.99 → 1999 cents
```

#### Cast Semantics in Deterministic Context

| Source Type | Target Type | Behavior | Notes |
|-------------|-------------|----------|-------|
| DQA(n) | BIGINT | Always succeeds | Scale truncated |
| DQA(0) | BIGINT | Always succeeds | Integer representation |
| DQA(18) | BIGINT | Always succeeds | Scale 18 → BigInt truncates |

### Function Signature

```rust
/// Convert DQA to BigInt.
///
/// This conversion always succeeds because DQA's i64 value fits
/// in any BigInt. The decimal scale is ignored (the value is
/// treated as an integer).
///
/// # Arguments
/// * `dqa` - The DQA value to convert
///
/// # Returns
/// BigInt representation of the DQA value (scale is truncated)
///
/// # Example
/// Dqa { value: 42, scale: 0 } → BigInt(42)
/// Dqa { value: 4200, scale: 2 } → BigInt(4200) — scale truncated
///
/// # Notes
/// The scale is truncated (not rounded). This is consistent with
/// BIGINT being an integer type.
pub fn dqa_to_bigint(dqa: &Dqa) -> BigInt
```

### Canonical Conversion Algorithm

```
DQA_TO_BIGINT(dqa: Dqa) -> BigInt

INPUT:  dqa (Dqa { value: i64, scale: u8 })
OUTPUT: BigInt

STEPS:

1. EXTRACT_VALUE
   Let i64_val = dqa.value

2. TO_BIGINT
   If i64_val >= 0:
     sign = false
     magnitude = i64_val as u64
   Else:
     sign = true
     magnitude = (i64_val == i64::MIN) ? (1u64 << 63) : ((-i64_val) as u64)

   // Handle i64::MIN specially since -i64::MIN overflows i64
   // i64::MIN = -9223372036854775808
   // |i64::MIN| = 9223372036854775808 = 2^63

3. CONSTRUCT_BIGINT
   If magnitude == 0:
     Return BigInt::zero()

   If magnitude <= u64::MAX:
     limbs = [magnitude as u64]
   Else:
     // magnitude has 2 limbs
     lo = magnitude & 0xFFFFFFFFFFFFFFFF
     hi = magnitude >> 64
     limbs = [lo, hi]

   Return BigInt { limbs: limbs, sign: sign }
```

### Edge Cases

| DQA Input | BIGINT Output | Notes |
|------------|---------------|-------|
| {0, 0} | BigInt::zero() | Canonical zero |
| {42, 0} | BigInt(42) | Simple positive |
| {-42, 0} | BigInt(-42) | Simple negative |
| {4200, 2} | BigInt(4200) | Scale truncated |
| {i64::MAX, 0} | BigInt(i64::MAX) | Maximum i64 |
| {i64::MIN, 0} | BigInt(i64::MIN) | Minimum i64 |
| {i64::MIN, 3} | BigInt(-9223372036854775808) | Scale truncated |
| {-1, 18} | BigInt(-1) | Scale truncated |

## Relationship to Other RFCs

| RFC | Relationship | Precedence |
|-----|-------------|------------|
| RFC-0105 (DQA) | Input type | DQA semantics preserved (scale truncation) |
| RFC-0110 (BIGINT) | Output type | BIGINT operations apply after conversion |

**Precedence Rule:** In case of conflict between this RFC and RFC-0105 or RFC-0110, this RFC takes precedence for the DQA→BIGINT conversion operation.

## Test Vectors

### V001: Zero
```
Input:  Dqa { value: 0, scale: 0 }
Output: BigInt::zero()
```

### V002: Small Positive
```
Input:  Dqa { value: 42, scale: 0 }
Output: BigInt::from(42i64)
```

### V003: Small Negative
```
Input:  Dqa { value: -42, scale: 0 }
Output: BigInt::from(-42i64)
```

### V004: Positive with Scale (Truncated)
```
Input:  Dqa { value: 4200, scale: 2 }
Output: BigInt::from(4200i64)
Note: Scale 2 means value represents 42.00, but BIGINT truncates to 4200
```

### V005: i64::MAX
```
Input:  Dqa { value: 9223372036854775807, scale: 0 }
Output: BigInt::from(i64::MAX)
```

### V006: i64::MIN
```
Input:  Dqa { value: -9223372036854775808, scale: 0 }
Output: BigInt::from(i64::MIN)
```

### V007: Currency Representation
```
Input:  Dqa { value: 1999, scale: 2 }  // Represents $19.99
Output: BigInt::from(1999i64)
Note: Scale is truncated, not rounded
```

### V008: Negative Scale Truncation
```
Input:  Dqa { value: -1999, scale: 2 }
Output: BigInt::from(-1999i64)
```

### V009: Maximum Scale (18)
```
Input:  Dqa { value: 1, scale: 18 }
Output: BigInt::from(1i64)
Note: Scale 18 (0.000000000000000001) truncated to 1
```

### V010: Maximum DQA Value
```
Input:  Dqa { value: 9223372036854775807, scale: 0 }
Output: BigInt::from(i64::MAX)
```

### V011: Minimum DQA Value
```
Input:  Dqa { value: -9223372036854775808, scale: 0 }
Output: BigInt::from(i64::MIN)
```

### V012: i64::MIN with Non-Zero Scale
```
Input:  Dqa { value: -9223372036854775808, scale: 6 }
Output: BigInt::from(-9223372036854775808i64)
Note: Scale is truncated, value unchanged
```

### V013: Positive Value with Max Scale
```
Input:  Dqa { value: 1234567890123456789, scale: 18 }
Output: BigInt::from(1234567890123456789i64)
Note: Scale 18 truncated
```

### V014: Negative Value with Max Scale
```
Input:  Dqa { value: -1234567890123456789, scale: 18 }
Output: BigInt::from(-1234567890123456789i64)
Note: Scale 18 truncated, sign preserved
```

### V015: Large Positive Value
```
Input:  Dqa { value: 9223372036854775807, scale: 18 }
Output: BigInt::from(9223372036854775807i64)
Note: Maximum i64 with max scale
```

### V016: Scale 1 Truncation
```
Input:  Dqa { value: 100, scale: 1 }
Output: BigInt::from(100i64)
Note: 100.0 → 100
```

### V017: Scale 1 with Small Value
```
Input:  Dqa { value: 5, scale: 1 }
Output: BigInt::from(5i64)
Note: 0.5 → 0 (truncated)
```

### V018: Negative Scale Truncation to Zero
```
Input:  Dqa { value: -1, scale: 1 }
Output: BigInt::from(-1i64)
Note: -0.1 → -1 (truncated toward negative infinity)
```

## Implementation Notes

### In determin crate

This conversion should be implemented in `determin/src/bigint.rs` as:

```rust
use crate::dqa::Dqa;

/// Convert DQA to BigInt (always succeeds).
///
/// This function exists in bigint.rs to keep conversion functions
/// near the target type, following RFC-0110's organization.
///
impl BigInt {
    /// Create a BigInt from a DQA value.
    ///
    /// The scale is truncated (not rounded).
    /// This always succeeds since i64 fits in BigInt.
    pub fn from_dqa(dqa: &Dqa) -> BigInt {
        // Algorithm per RFC-0132
    }
}

/// Convert DQA to BigInt (free function form).
pub fn dqa_to_bigint(dqa: &Dqa) -> BigInt {
    BigInt::from_dqa(dqa)
}
```

Or alternatively in `determin/src/dqa.rs`:

```rust
use crate::bigint::BigInt;

impl Dqa {
    /// Convert DQA to BigInt.
    ///
    /// Scale is truncated. Always succeeds.
    pub fn to_bigint(&self) -> BigInt {
        // Algorithm per RFC-0132
    }
}
```

### Gas Cost

DQA→BIGINT conversion is O(1) because i64 trivially fits in BigInt's arbitrary range. Gas cost should be:
```
GAS = 5  // Fixed cost, no variable component
```

This is a fixed cost because:
- No limb iteration needed (i64 is always 1-2 limbs)
- No range checking needed (always succeeds)
- No scale adjustment needed (truncation is identity for integer types)

## Error Handling and Diagnostics

### Compile-Time Errors

DQA→BIGINT conversion **cannot fail**. The compiler does not emit errors for this conversion.

```
-- This is always valid:
SELECT CAST(dqa_col AS BIGINT) FROM any_table;
-- No error possible
```

### Runtime Behavior

| Scenario | Behavior | Notes |
|----------|----------|-------|
| Any valid DQA | Always succeeds | No errors possible |

**Note:** Unlike BIGINT→DQA (which can overflow), DQA→BIGINT always succeeds because BigInt has arbitrary precision.

## Formal Verification Framework

### Theorem Hierarchy

| # | Theorem | Property | Status |
|---|---------|----------|--------|
| T1 | Determinism | Bit-identical results across platforms | Required |
| T2 | Range Preservation | Output BigInt can represent input value | Required |
| T3 | Scale Truncation | Scale is ignored (not rounded) | Required |
| T4 | Sign Preservation | Negative DQA produces negative BigInt | Required |
| T5 | Zero Canonicalization | DQA{0, any} → BigInt::zero() | Required |

### Theorem Specifications

**Theorem T1 (Determinism):** For identical DQA input, the conversion always produces identical BIGINT output.

**Theorem T2 (Range Preservation):** For any valid DQA input, the output BigInt can represent the same integer value (i64 always fits in BigInt).

**Theorem T3 (Scale Truncation):** The output BigInt is the integer part of the DQA value (scale is discarded).

**Theorem T4 (Sign Preservation):** If `dqa.value < 0`, then `result.sign = true`.

**Theorem T5 (Zero Canonicalization):** `dqa_to_bigint(Dqa { value: 0, scale: s }) = BigInt::zero()` for any valid scale s.

## Implementation Checklist

| Mission | Description | Status | Complexity |
|---------|-------------|--------|------------|
| M1 | `dqa_to_bigint` core algorithm | Pending | Low |
| M2 | i64::MIN special case handling | Pending | Low |
| M3 | Scale truncation (discard scale) | Pending | Low |
| M4 | Sign handling | Pending | Low |
| M5 | Test vector suite (18 vectors) | Pending | Low |
| M6 | Integration with BigInt type | Pending | Low |

## Future Work

- F1: BIGINT→DQA conversion (see RFC-0131)
- F2: BIGINT→DECIMAL conversion (see RFC-0133)
- F3: DECIMAL→BIGINT conversion (see RFC-0134)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.1 | 2026-03-23 | Enhanced: Added Input/Output Contract, Scale Context Propagation, SQL Integration, Constraints, Error Handling & Diagnostics, Formal Verification Framework (5 theorems), Implementation Checklist, expanded test vectors from 8 to 18 |
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
