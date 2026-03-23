# RFC-0133 (Numeric/Math): BIGINT to DECIMAL Conversion

## Status

**Version:** 1.0 (Draft)
**Status:** Draft
**Depends On:** RFC-0110 (BIGINT), RFC-0111 (DECIMAL)
**Category:** Numeric/Math

## Summary

This RFC specifies the conversion algorithm from BIGINT (RFC-0110, arbitrary-precision integer up to 4096 bits) to DECIMAL (RFC-0111, i128 mantissa with 0-36 decimal scale). This conversion is necessary for the Numeric Tower to support operations that require BIGINT values to be used in DECIMAL contexts, and for explicit CAST expressions between these types.

The conversion TRAPs if the BIGINT value's decimal representation exceeds DECIMAL's representable range (|mantissa| ≤ 10^36 - 1).

## Motivation

### Problem Statement

BIGINT provides arbitrary-precision integers up to 4096 bits. DECIMAL provides high-precision decimal arithmetic with i128 mantissa and 0-36 scale, representing values up to ±(10^36 - 1). When a BIGINT value must be used in a DECIMAL context, a conversion is required.

Without a rigorous specification:
- Two implementations could convert the same BIGINT to different DECIMAL values
- Range violations could be handled inconsistently
- Scale handling could differ

### Why This RFC Exists

RFC-0111 defines DECIMAL and includes a `bigint_to_decimal(value: i128)` function for i128→DECIMAL conversion. However, BIGINT can represent values up to 4096 bits (128 decimal digits), which far exceeds i128 (39 decimal digits). This RFC specifies the full BIGINT→DECIMAL conversion for arbitrary-precision integers.

## Specification

### Relationship to Existing Functions

RFC-0111 specifies `bigint_to_decimal(value: i128) -> Result<Decimal, DecimalError>` which converts i128 values to DECIMAL with scale 0. This existing function MUST NOT be changed as it provides i128↔DECIMAL interoperability per RFC-0110.

This RFC specifies a new function for arbitrary BigInt conversion:

```rust
/// Convert arbitrary BigInt to DECIMAL with the given scale.
///
/// This is the arbitrary-precision version that handles BigInt values
/// potentially exceeding i128 range. Unlike the i128-based
/// `bigint_to_decimal` in RFC-0111, this function can convert
/// any BigInt value within DECIMAL's range.
///
/// TRAPs if:
/// - scale > 36
/// - |value| > MAX_DECIMAL_MANTISSA (10^36 - 1)
///
/// # Arguments
/// * `b` - The BigInt value to convert
/// * `scale` - Decimal scale for the result (0-36)
///
/// # Errors
/// * `BigIntError::InvalidScale` if scale > 36
/// * `BigIntError::OutOfRange` if |b| > MAX_DECIMAL_MANTISSA
///
/// # Example
/// BigInt(42) with scale 0 → Decimal { mantissa: 42, scale: 0 }
/// BigInt(42) with scale 3 → Decimal { mantissa: 42000, scale: 3 }
pub fn bigint_to_decimal_full(b: BigInt, scale: u8) -> Result<Decimal, BigIntError>
```

### Canonical Conversion Algorithm

```
BIGINT_TO_DECIMAL_FULL(b: BigInt, scale: u8) -> Result<Decimal, BigIntError>

INPUT:  b (BigInt), scale (u8, 0 ≤ scale ≤ 36)
OUTPUT: Decimal { mantissa: i128, scale: u8 } or error

STEPS:

1. VALIDATE_SCALE
   If scale > 36:
     return Error(InvalidScale)

2. COMPUTE_DECIMAL_VALUE
   // BigInt value = b.significand * 2^(b.exponent) for BigInt
   // BigInt is pure integer, so exponent = 0
   // The decimal value = BigInt value * 10^scale

   If scale == 0:
     // No scaling needed, just convert BigInt to i128
     Let decimal_mantissa = BigInt_to_i128(b)
     If Error: return Error(OutOfRange)

   Else:
     // Multiply BigInt by 10^scale
     Let pow10 = BigInt::from(10^i) where i = scale
     Let scaled = BigInt_mul(b, pow10)
     If Error: return Error(OutOfRange)  // Overflow

     // Check if scaled fits in DECIMAL range
     Let abs_scaled = |scaled|
     If abs_scaled > MAX_DECIMAL_MANTISSA (10^36 - 1):
       return Error(OutOfRange)

     Let decimal_mantissa = BigInt_to_i128(scaled)
     If Error: return Error(OutOfRange)

3. CONSTRUCT_DECIMAL
   Return Decimal { mantissa: decimal_mantissa, scale: scale }
```

### Scale Handling

The scale parameter works as follows:
- Scale 0: Integer representation, no decimal places
- Scale N: Value = mantissa × 10^(-N)

Example: BigInt(42) → Decimal {42, 0} = 42
Example: BigInt(42) → Decimal {42000, 3} = 42.000

### Edge Cases

| BigInt Input | Scale | DECIMAL Output | Notes |
|-------------|-------|----------------|-------|
| 0 | any | Decimal { 0, 0 } | Canonical zero |
| 42 | 0 | Decimal { 42, 0 } | Scale 0 |
| 42 | 3 | Decimal { 42000, 3 } | Scale adjustment |
| MAX_DECIMAL | 0 | Decimal { 10^36-1, 0 } | Maximum DECIMAL |
| -(MAX_DECIMAL) | 0 | Decimal { -(10^36-1), 0 } | Minimum DECIMAL |
| 10^37 | 0 | Error(OutOfRange) | Exceeds MAX_DECIMAL |
| 10^35 | 2 | Decimal { 10^37, 2 } | Overflow after scaling |

### Range Check Algorithm

```
CHECK_BIGINT_FITS_DECIMAL(b: BigInt, scale: u8) -> bool

// Maximum decimal value = 10^36 - 1
// After scaling: |b| * 10^scale <= 10^36 - 1
// So: |b| <= (10^36 - 1) / 10^scale

If scale == 0:
  return |b| <= MAX_DECIMAL_MANTISSA

If scale >= 36:
  // 10^scale >= 10^36, so b must be 0 or 1
  return |b| <= 1

// General case: |b| <= floor((10^36 - 1) / 10^scale)
// Pre-computed table for efficiency:
max_b_for_scale[0] = 10^36 - 1
max_b_for_scale[1] = 10^35 - 1
...
max_b_for_scale[36] = 0
```

## Relationship to Other RFCs

| RFC | Relationship | Precedence |
|-----|-------------|------------|
| RFC-0110 (BIGINT) | Input type | BIGINT operations apply before conversion |
| RFC-0111 (DECIMAL) | Output type | DECIMAL semantics apply after conversion |

**Precedence Rule:** In case of conflict between this RFC and RFC-0110 or RFC-0111, this RFC takes precedence for the BIGINT→DECIMAL conversion operation.

**Note:** The existing `bigint_to_decimal(value: i128)` function in RFC-0111 is unaffected by this RFC. It provides i128↔DECIMAL interoperability and is NOT replaced by this RFC.

## Test Vectors

### V001: Zero with Scale
```
Input:  BigInt::zero(), scale = 5
Output: Decimal { mantissa: 0, scale: 0 }
Note: Canonical zero has scale 0
```

### V002: Small Positive with Scale 0
```
Input:  BigInt::from(42i64), scale = 0
Output: Decimal { mantissa: 42, scale: 0 }
```

### V003: Small Positive with Scale
```
Input:  BigInt::from(42i64), scale = 3
Output: Decimal { mantissa: 42000, scale: 3 }
```

### V004: Small Negative
```
Input:  BigInt::from(-42i64), scale = 2
Output: Decimal { mantissa: -4200, scale: 2 }
```

### V005: Maximum DECIMAL
```
Input:  BigInt::from(MAX_DECIMAL_MANTISSA), scale = 0
Output: Decimal { mantissa: 999999999999999999999999999999999999, scale: 0 }
```

### V006: Overflow — Exceeds MAX_DECIMAL
```
Input:  BigInt::from(10_i128.pow(36)), scale = 0
Output: Error(OutOfRange)
Note: 10^36 exceeds MAX_DECIMAL_MANTISSA (10^36 - 1)
```

### V007: Overflow After Scale Multiplication
```
Input:  BigInt::from(10_i128.pow(34)), scale = 3
Output: Error(OutOfRange)
Note: 10^34 * 10^3 = 10^37 > 10^36 - 1
```

### V008: Currency Representation
```
Input:  BigInt::from(1999i64), scale = 2
Output: Decimal { mantissa: 199900, scale: 2 }
Note: Represents $1,999.00 in cents with cents
```

### V009: Large BigInt (Exceeds i128)
```
Input:  BigInt with limbs > 2, value = 10^38
Output: Error(OutOfRange)
Note: Even with scale 0, exceeds DECIMAL range
```

## Implementation Notes

### In determin crate

This conversion should be implemented in `determin/src/decimal.rs` as:

```rust
use crate::bigint::BigInt;

/// Convert arbitrary BigInt to DECIMAL with the given scale.
///
/// This is the full-precision version that handles any BigInt
/// value within DECIMAL's range.
///
/// Algorithm per RFC-0133.
pub fn bigint_to_decimal_full(b: BigInt, scale: u8) -> Result<Decimal, BigIntError> {
    if scale > MAX_DECIMAL_SCALE {
        return Err(BigIntError::InvalidScale);
    }

    // For scale 0, just check if fits in i128 and create Decimal
    // For scale > 0, multiply by 10^scale first, then check range
    // ...
}
```

### Gas Cost

BIGINT→DECIMAL conversion cost depends on scale:
```
BASE_GAS = 20  // BigInt to i128 conversion
SCALE_GAS = 5 * scale  // Multiplication by POW10[scale]
Total: BASE_GAS + SCALE_GAS
```

## Future Work

- F1: BIGINT→DQA conversion (see RFC-0131)
- F2: DQA→BIGINT conversion (see RFC-0132)
- F3: DECIMAL→BIGINT conversion (see RFC-0134)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
