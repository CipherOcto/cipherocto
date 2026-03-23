# RFC-0131 (Numeric/Math): BIGINT to DQA Conversion

## Status

**Version:** 1.0 (Draft)
**Status:** Draft
**Depends On:** RFC-0110 (BIGINT), RFC-0105 (DQA)
**Category:** Numeric/Math

## Summary

This RFC specifies the conversion algorithm from BIGINT (RFC-0110, arbitrary-precision integer up to 4096 bits) to DQA (RFC-0105, i64 with 0-18 decimal scale). This conversion is necessary for the Numeric Tower to support operations that require BIGINT values to be used in DQA contexts, and for explicit CAST expressions between these types.

The conversion TRAPs if the BIGINT value exceeds the representable DQA range (i64::MIN to i64::MAX).

## Motivation

### Problem Statement

BIGINT provides arbitrary-precision integers up to 4096 bits. DQA provides fixed-precision decimal arithmetic with i64 value and 0-18 scale. When a BIGINT value must be used in a DQA context (e.g., arithmetic with DQA operands, or explicit CAST), a conversion is required.

Without a rigorous specification:
- Two implementations could convert the same BIGINT to different DQA values
- Range violations could be handled inconsistently
- Scale handling could differ

### Why This RFC Exists

RFC-0105 defines DQA but does not define BIGINT→DQA conversion. RFC-0110 defines BIGINT but its DQA interop section only covers i128↔DQA (not full BigInt↔DQA). This RFC fills that gap.

## Specification

### Function Signature

```rust
/// Convert BigInt to DQA with the given decimal scale.
///
/// TRAPs if the BigInt value does not fit in i64 range.
/// The scale parameter determines the decimal precision of the DQA result.
///
/// # Arguments
/// * `b` - The BigInt value to convert
/// * `scale` - Decimal scale for the DQA result (0-18)
///
/// # Errors
/// * `BigIntError::OutOfRange` if |b| > i64::MAX
///
/// # Example
/// BigInt(42) with scale 0 → Dqa { value: 42, scale: 0 }
/// BigInt(42) with scale 2 → Dqa { value: 4200, scale: 2 }
pub fn bigint_to_dqa(b: &BigInt, scale: u8) -> Result<Dqa, BigIntError>
```

### Canonical Conversion Algorithm

```
BIGINT_TO_DQA(b: BigInt, scale: u8) -> Result<Dqa, BigIntError>

INPUT:  b (BigInt), scale (u8, 0 ≤ scale ≤ 18)
OUTPUT: Dqa { value: i64, scale: u8 } or error

STEPS:

1. RANGE_CHECK
   If scale > 18:
     return Error(InvalidScale)

   If b.limbs.length > 2:
     // BigInt requires more than 128 bits
     return Error(OutOfRange)

   If b.limbs.length == 2:
     // Check if value fits in i64 (128-bit value in 2 limbs)
     // For positive: if hi > 0x8000_0000_0000_0000, overflow
     // For negative: if hi > 0x8000_0000_0000_0000, overflow
     // If hi == 0x8000_0000_0000_0000 and lo > 0, overflow (for positive)
     // If hi >= 0x8000_0000_0000_0001, overflow
     Check magnitude against i64 boundary
     If overflow: return Error(OutOfRange)

2. EXTRACT_I64
   Convert b to i64:
   - If b.sign == false: value = lo | (hi << 64)
   - If b.sign == true: value = -(|lo| | (|hi| << 64))
   (Two's complement handling for negative values)

3. CONSTRUCT_DQA
   Return Dqa { value: i64, scale: scale }
```

### Edge Cases

| BigInt Input | Scale | DQA Output | Notes |
|-------------|-------|------------|-------|
| 0 | any | Dqa { 0, 0 } | Canonical zero has scale 0 |
| i64::MAX | 0 | Dqa { i64::MAX, 0 } | Maximum representable |
| i64::MIN | 0 | Dqa { i64::MIN, 0 } | Minimum representable |
| i64::MAX + 1 | 0 | Error(OutOfRange) | Overflow |
| i64::MIN - 1 | 0 | Error(OutOfRange) | Overflow |
| 42 | 2 | Dqa { 4200, 2 } | Scale adjustment |
| -42 | 3 | Dqa { -42000, 3 } | Negative with scale |
| BigInt with 3+ limbs | any | Error(OutOfRange) | Exceeds i64 |

### Error Handling

| Error | Condition | RFC Reference |
|-------|-----------|--------------|
| `BigIntError::OutOfRange` | Value exceeds i64 range | This RFC |
| `BigIntError::NonCanonicalInput` | Input BigInt not canonical | RFC-0110 |

## Relationship to Other RFCs

| RFC | Relationship | Precedence |
|-----|-------------|------------|
| RFC-0110 (BIGINT) | Input type | BIGINT operations apply before conversion |
| RFC-0105 (DQA) | Output type | DQA semantics apply after conversion |

**Precedence Rule:** In case of conflict between this RFC and RFC-0105 or RFC-0110, this RFC takes precedence for the BIGINT→DQA conversion operation.

## Test Vectors

### V001: Zero Conversion
```
Input:  BigInt::zero(), scale = 0
Output: Dqa { value: 0, scale: 0 }
```

### V002: Small Positive Integer
```
Input:  BigInt::from(42i64), scale = 0
Output: Dqa { value: 42, scale: 0 }
```

### V003: Small Negative Integer
```
Input:  BigInt::from(-42i64), scale = 0
Output: Dqa { value: -42, scale: 0 }
```

### V004: Positive with Scale
```
Input:  BigInt::from(42i64), scale = 3
Output: Dqa { value: 42000, scale: 3 }
```

### V005: i64::MAX
```
Input:  BigInt::from(i64::MAX), scale = 0
Output: Dqa { value: 9223372036854775807, scale: 0 }
```

### V006: i64::MIN
```
Input:  BigInt::from(i64::MIN), scale = 0
Output: Dqa { value: -9223372036854775808, scale: 0 }
```

### V007: Overflow — Too Large
```
Input:  BigInt { limbs: [0, 0, 1], sign: false }, scale = 0
Output: Error(OutOfRange)
Note: Requires 3 limbs (192 bits) > i64 range
```

### V008: Overflow — i64::MAX + 1
```
Input:  BigInt { limbs: [0, 0x8000000000000001], sign: false }, scale = 0
Output: Error(OutOfRange)
Note: Magnitude exceeds i64::MAX
```

### V009: Overflow — Negative Beyond i64::MIN
```
Input:  BigInt { limbs: [0, 0x8000000000000001], sign: true }, scale = 0
Output: Error(OutOfRange)
Note: |value| > i64::MAX after sign adjustment
```

### V010: Scale Adjustment for Currency
```
Input:  BigInt::from(1999i64), scale = 2
Output: Dqa { value: 199900, scale: 2 }
Note: Represents $19.99 in cents
```

## Implementation Notes

### In determin crate

This conversion should be implemented in `determin/src/bigint.rs` as:

```rust
use crate::dqa::Dqa;

impl BigInt {
    /// Convert BigInt to DQA.
    ///
    /// TRAPs if the BigInt value exceeds DQA's representable range.
    pub fn to_dqa(&self, scale: u8) -> Result<Dqa, BigIntError> {
        // Algorithm per RFC-0131
    }
}
```

### Gas Cost

BIGINT→DQA conversion is a O(n) operation where n = number of limbs. Gas cost should be:
```
GAS = 10 + 2 * num_limbs
```

This accounts for:
- 10 base cost (fixed overhead)
- 2 per limb (memory access and range check)

## Future Work

- F1: DQA→BIGINT conversion (see RFC-0132)
- F2: BIGINT→DECIMAL conversion (see RFC-0133)
- F3: DECIMAL→BIGINT conversion (see RFC-0134)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
