# RFC-0134 (Numeric/Math): DECIMAL to BIGINT Conversion

## Status

**Version:** 1.0 (Draft)
**Status:** Draft
**Depends On:** RFC-0110 (BIGINT), RFC-0111 (DECIMAL)
**Category:** Numeric/Math

## Summary

This RFC specifies the conversion algorithm from DECIMAL (RFC-0111, i128 mantissa with 0-36 decimal scale) to BIGINT (RFC-0110, arbitrary-precision integer up to 4096 bits). This conversion is necessary for the Numeric Tower to support operations that require DECIMAL values to be used in BIGINT contexts, and for explicit CAST expressions between these types.

The conversion TRAPs if the DECIMAL has a non-zero fractional part (scale > 0), as this would result in precision loss.

## Motivation

### Problem Statement

DECIMAL provides high-precision decimal arithmetic with i128 mantissa and 0-36 scale, representing values up to ±(10^36 - 1). BIGINT provides arbitrary-precision integers. When a DECIMAL value must be used in a BIGINT context (e.g., arithmetic with BIGINT operands), a conversion is required.

Without a rigorous specification:
- Two implementations could convert the same DECIMAL to different BIGINT values
- Precision loss from fractional truncation could be handled inconsistently
- Error handling for scale > 0 could differ

### Why This RFC Exists

RFC-0111 specifies `decimal_to_bigint(d: &Decimal) -> Result<i128, DecimalError>` which converts DECIMAL→i128 (not BigInt) and requires scale = 0. This function is for i128-range DECIMAL values. This RFC specifies the full DECIMAL→BIGINT conversion for arbitrary DECIMAL values.

## Specification

### Relationship to Existing Functions

RFC-0111 specifies `decimal_to_bigint(d: &Decimal) -> Result<i128, DecimalError>` which:
- Returns `i128` (not `BigInt`)
- Requires `d.scale == 0`
- TRAPs with `DecimalError::ConversionLoss` if scale > 0

This existing function is for i128-range DECIMAL values and MUST NOT be changed. This RFC specifies the full BigInt-range DECIMAL→BIGINT conversion.

```rust
/// Convert DECIMAL to arbitrary-precision BigInt.
///
/// TRAPs if the DECIMAL has a non-zero fractional part (scale > 0),
/// as this would result in precision loss.
///
/// This is the arbitrary-precision version that handles any DECIMAL
/// value, not just i128-range values.
///
/// # Arguments
/// * `d` - The DECIMAL value to convert
///
/// # Errors
/// * `DecimalError::ConversionLoss` if d.scale > 0 (precision loss)
///   Note: The DECIMAL type stores scale, so this error indicates
///   truncation would occur.
///
/// # Example
/// Decimal { mantissa: 42, scale: 0 } → BigInt(42)
/// Decimal { mantissa: 42000, scale: 3 } → Error(ConversionLoss)
///   Note: 42.000 = 42, but scale 3 indicates 3 decimal places
///   were intentional and truncating to 42 loses information
pub fn decimal_to_bigint_full(d: &Decimal) -> Result<BigInt, DecimalError>
```

### Canonical Conversion Algorithm

```
DECIMAL_TO_BIGINT_FULL(d: Decimal) -> Result<BigInt, DecimalError>

INPUT:  d (Decimal { mantissa: i128, scale: u8 })
OUTPUT: BigInt or error

STEPS:

1. CHECK_SCALE
   If d.scale > 0:
     return Error(ConversionLoss)
   // Scale of 0 means integer, no fractional part

2. CONVERT_TO_BIGINT
   // mantissa is already an integer (scale = 0)
   // Just need to convert i128 to BigInt

   If d.mantissa == 0:
     Return BigInt::zero()

   If d.mantissa >= 0:
     sign = false
     abs_value = d.mantissa as u128
   Else:
     sign = true
     // Handle i128::MIN specially
     If d.mantissa == i128::MIN:
       abs_value = (1u128 << 127)  // 2^127
     Else:
       abs_value = (-d.mantissa) as u128

3. CONSTRUCT_BIGINT
   If abs_value fits in 64 bits:
     limbs = [abs_value as u64]
   Else:
     // 65-128 bits
     lo = abs_value & 0xFFFFFFFFFFFFFFFF
     hi = abs_value >> 64
     limbs = [lo, hi]

   Return BigInt { limbs: limbs, sign: sign }
```

### Scale Handling

DECIMAL's scale indicates the number of decimal places. For example:
- Decimal {42, 0} = 42 (integer)
- Decimal {4200, 2} = 42.00 (two decimal places)
- Decimal {420, 1} = 42.0 (one decimal place)

When converting to BIGINT:
- Scale 0: Direct conversion (integer)
- Scale > 0: **ERROR** — precision loss

The error is `ConversionLoss` because:
1. The scale was explicitly set, indicating fractional precision matters
2. BIGINT cannot represent the fractional part
3. Truncation would silently discard data

### Edge Cases

| DECIMAL Input | BIGINT Output | Notes |
|--------------|---------------|-------|
| {0, 0} | BigInt::zero() | Canonical zero |
| {42, 0} | BigInt(42) | Scale 0, direct |
| {-42, 0} | BigInt(-42) | Negative, scale 0 |
| {42000, 3} | Error(ConversionLoss) | Scale 3 means fractional part exists |
| {MAX_DECIMAL_MANTISSA, 0} | BigInt(MAX_DECIMAL_MANTISSA) | Maximum, fits in BigInt |
| {i128::MAX, 0} | BigInt(i128::MAX) | i128::MAX fits in 2 limbs |
| {i128::MIN, 0} | BigInt(i128::MIN) | i128::MIN fits in 2 limbs |
| {10^36 - 1, 36} | Error(ConversionLoss) | Scale 36 = fractional, even if mantissa is integer |

### Why Scale > 0 Is An Error

Consider: `Decimal { mantissa: 42000, scale: 3 }` represents `42.000` (42 with 3 decimal places of precision).

Converting to BIGINT by truncation gives `BigInt(42000)`. But this loses the information that the original value had fractional precision — `42.000` is numerically equal to `42`, but the scale metadata is lost.

For DECIMAL→BIGINT conversion, we have two options:
1. **TRUNCATE** (lose scale metadata): `42.000` → `42`
2. **ERROR** (preserve scale semantics): `42.000` → Error

This RFC chooses **ERROR** because:
- DECIMAL with scale > 0 indicates fractional context
- Losing that context silently is dangerous
- Users should explicitly ROUND or CAST to integer first

## Relationship to Other RFCs

| RFC | Relationship | Precedence |
|-----|-------------|------------|
| RFC-0111 (DECIMAL) | Input type | DECIMAL semantics preserved (scale check) |
| RFC-0110 (BIGINT) | Output type | BIGINT operations apply after conversion |

**Precedence Rule:** In case of conflict between this RFC and RFC-0110 or RFC-0111, this RFC takes precedence for the DECIMAL→BIGINT conversion operation.

**Note:** The existing `decimal_to_bigint(d: &Decimal) -> Result<i128, DecimalError>` function in RFC-0111 is unaffected by this RFC. It provides DECIMAL→i128 for scale-0 values and is NOT replaced by this RFC.

## Test Vectors

### V001: Zero
```
Input:  Decimal { mantissa: 0, scale: 0 }
Output: BigInt::zero()
```

### V002: Simple Positive
```
Input:  Decimal { mantissa: 42, scale: 0 }
Output: BigInt::from(42i64)
```

### V003: Simple Negative
```
Input:  Decimal { mantissa: -42, scale: 0 }
Output: BigInt::from(-42i64)
```

### V004: Maximum i128
```
Input:  Decimal { mantissa: i128::MAX, scale: 0 }
Output: BigInt::from(i128::MAX)
```

### V005: Minimum i128
```
Input:  Decimal { mantissa: i128::MIN, scale: 0 }
Output: BigInt::from(i128::MIN)
```

### V006: Maximum DECIMAL Mantissa
```
Input:  Decimal { mantissa: MAX_DECIMAL_MANTISSA (10^36 - 1), scale: 0 }
Output: BigInt { limbs: [MAX_DECIMAL_MANTISSA as u64, (MAX_DECIMAL_MANTISSA >> 64) as u64], sign: false }
Note: Requires 2 limbs since 10^36 > 2^64
```

### V007: Scale > 0 — Error
```
Input:  Decimal { mantissa: 42000, scale: 3 }
Output: Error(ConversionLoss)
Note: Represents 42.000, scale 3 means fractional precision exists
```

### V008: Scale 1 with Integer Mantissa — Error
```
Input:  Decimal { mantissa: 42, scale: 1 }
Output: Error(ConversionLoss)
Note: Represents 4.2, scale 1 = fractional part
```

### V009: Scale 36 with Integer Mantissa — Error
```
Input:  Decimal { mantissa: 1000000, scale: 6 }
Output: Error(ConversionLoss)
Note: Even though mantissa is multiple of 10^6, scale indicates fractional context
```

## Implementation Notes

### In determin crate

This conversion should be implemented in `determin/src/decimal.rs` as:

```rust
use crate::bigint::BigInt;

/// Convert DECIMAL to arbitrary-precision BigInt.
///
/// TRAPs if scale > 0 (precision loss would occur).
///
/// This is the full-precision version that handles any DECIMAL
/// value, not just i128-range values.
///
/// Algorithm per RFC-0134.
pub fn decimal_to_bigint_full(d: &Decimal) -> Result<BigInt, DecimalError> {
    if d.scale > 0 {
        return Err(DecimalError::ConversionLoss);
    }
    // Convert mantissa to BigInt
    // ...
}
```

### Gas Cost

DECIMAL→BIGINT conversion cost:
```
BASE_GAS = 15  // Scale check + BigInt construction
```

## Future Work

- F1: BIGINT→DQA conversion (see RFC-0131)
- F2: DQA→BIGINT conversion (see RFC-0132)
- F3: BIGINT→DECIMAL conversion (see RFC-0133)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
