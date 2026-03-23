# RFC-0132 (Numeric/Math): DQA to BIGINT Conversion

## Status

**Version:** 1.0 (Draft)
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

## Specification

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

## Future Work

- F1: BIGINT→DQA conversion (see RFC-0131)
- F2: BIGINT→DECIMAL conversion (see RFC-0133)
- F3: DECIMAL→BIGINT conversion (see RFC-0134)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
