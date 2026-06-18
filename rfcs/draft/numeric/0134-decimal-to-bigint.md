# RFC-0134 (Numeric/Math): DECIMAL to BIGINT Conversion

## Status

**Version:** 1.1 (Draft)
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

### RFC-0110 and RFC-0111 Coverage Analysis

| Conversion | RFC-0111 (DECIMAL) | RFC-0110 (BIGINT) | This RFC |
|------------|--------------------|--------------------|----------|
| DECIMAL → i128 | `decimal_to_bigint()` | Not specified | Not needed |
| DECIMAL → BigInt | Not specified | Not specified | **This RFC** |

**Key insight:** The existing `decimal_to_bigint` returns i128, not BigInt. This RFC specifies the arbitrary-precision version that handles the full DECIMAL range.

## Input/Output Contract

```rust
/// Error variants for DECIMAL→BIGINT conversion
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecimalError {
    /// DECIMAL has non-zero fractional part (scale > 0)
    ConversionLoss {
        scale: u8,
        mantissa: String,
        reason: &'static str,
    },
}

/// DECIMAL→BIGINT conversion result
pub type DecimalToBigIntResult = Result<BigInt, DecimalError>;

/// Input to the conversion
pub struct DecimalToBigIntInput {
    /// The DECIMAL value to convert
    pub value: Decimal,
}

/// Output from the conversion
pub enum DecimalToBigIntOutput {
    /// Successfully converted to BIGINT
    Success(BigInt),
    /// Conversion error with details
    Error(DecimalError),
}
```

## Scale Context Propagation

The scale in DECIMAL represents decimal places. When converting to BIGINT:

| DECIMAL | Scale | BIGINT Output | Rationale |
|---------|-------|---------------|-----------|
| {42, 0} | 0 | 42 | Integer, no fractional part |
| {42, 2} | 2 | Error | 4.2 has fractional part |
| {4200, 2} | 2 | Error | 42.00 has fractional part |
| {42000, 3} | 3 | Error | 42.000 has fractional part |

**Critical:** Scale > 0 always fails because DECIMAL's scale indicates the value has fractional precision. Converting to BIGINT would lose that information.

## SQL Integration

DECIMAL→BIGINT conversion appears in SQL CAST expressions:

```sql
-- Explicit CAST from DECIMAL to BIGINT
SELECT CAST(decimal_col AS BIGINT) FROM account_balances;

-- This is VALID only when scale = 0:
-- Decimal{42, 0} → BigInt(42)
-- Decimal{-1999, 0} → BigInt(-1999)

-- FORBIDDEN: DECIMAL with scale > 0
SELECT CAST(decimal_col AS BIGINT) FROM currency_amounts;
-- Decimal{1999, 2} represents $19.99 — error!
-- Error: DecimalError::ConversionLoss

-- Recommended: ROUND or CAST to integer first
SELECT CAST(ROUND(decimal_col, 0) AS BIGINT) FROM currency_amounts;
```

#### Cast Semantics in Deterministic Context

| Source Type | Target Type | Behavior | Notes |
|-------------|-------------|----------|-------|
| DECIMAL(36, 0) | BIGINT | Always succeeds | Integer representation |
| DECIMAL(36, n) where n > 0 | BIGINT | **Error** | Fractional precision would be lost |

## Constraints

| Constraint Type | Description |
|----------------|-------------|
| **Scale must be 0** | scale > 0 → ConversionLoss error |
| **Value bounds** | Any DECIMAL mantissa fits in BigInt |
| **Determinism** | Identical DECIMAL input always produces identical BIGINT output |
| **No truncation** | Scale > 0 is an error, not truncated |

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

### V010: Minimum DECIMAL Mantissa (Negative)
```
Input:  Decimal { mantissa: -MAX_DECIMAL_MANTISSA, scale: 0 }
Output: BigInt { limbs: [MAX_DECIMAL_MANTISSA as u64, (MAX_DECIMAL_MANTISSA >> 64) as u64], sign: true }
Note: Negative, requires 2 limbs
```

### V011: Small Positive with Scale 1 — Error
```
Input:  Decimal { mantissa: 5, scale: 1 }
Output: Error(ConversionLoss)
Note: Represents 0.5, fractional part exists
```

### V012: Small Negative with Scale 1 — Error
```
Input:  Decimal { mantissa: -5, scale: 1 }
Output: Error(ConversionLoss)
Note: Represents -0.5, fractional part exists
```

### V013: Scale 18 (DQA max) — Error
```
Input:  Decimal { mantissa: 42, scale: 18 }
Output: Error(ConversionLoss)
Note: Even though mantissa is integer, scale 18 indicates fractional context
```

### V014: Maximum DECIMAL with Scale 0
```
Input:  Decimal { mantissa: MAX_DECIMAL_MANTISSA, scale: 0 }
Output: BigInt { limbs: [MAX_DECIMAL_MANTISSA as u64, (MAX_DECIMAL_MANTISSA >> 64) as u64], sign: false }
Note: Maximum value, 2 limbs needed
```

### V015: i128::MIN with Scale 0
```
Input:  Decimal { mantissa: i128::MIN, scale: 0 }
Output: BigInt { limbs: [0x8000000000000000, 0], sign: true }
Note: i128::MIN = -2^127, special case
```

### V016: i128::MAX with Scale 0
```
Input:  Decimal { mantissa: i128::MAX, scale: 0 }
Output: BigInt { limbs: [i128::MAX as u64, (i128::MAX >> 64) as u64], sign: false }
Note: i128::MAX fits in 2 limbs
```

### V017: One with Scale 36 — Error
```
Input:  Decimal { mantissa: 1, scale: 36 }
Output: Error(ConversionLoss)
Note: Represents 0.000...001 (36 zeros), fractional part exists
```

### V018: Large Value with Scale 0
```
Input:  Decimal { mantissa: 10_i128.pow(35), scale: 0 }
Output: BigInt { limbs: [10_i128.pow(35) as u64, (10_i128.pow(35) >> 64) as u64], sign: false }
Note: 10^35 fits in BigInt
```

### V019: Negative Large Value with Scale 0
```
Input:  Decimal { mantissa: -10_i128.pow(35), scale: 0 }
Output: BigInt { limbs: [10_i128.pow(35) as u64, (10_i128.pow(35) >> 64) as u64], sign: true }
Note: Negative large value
```

### V020: Scale 2 Currency — Error
```
Input:  Decimal { mantissa: 199900, scale: 2 }
Output: Error(ConversionLoss)
Note: Represents $1,999.00 — must ROUND first before BIGINT
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

This is a fixed cost because:
- Scale check is O(1)
- i128 to BigInt conversion is O(1) (i128 always fits in 2 limbs)

## Error Handling and Diagnostics

### Compile-Time Errors

When DECIMAL→BIGINT conversion fails at compile time:

```
ERROR: Cannot convert DECIMAL to BIGINT
  Expression: CAST(decimal_col AS BIGINT) at line 42
  Reason: DecimalError::ConversionLoss — scale=3 indicates fractional part
  Hint: Use ROUND(decimal_col, 0) or CAST(decimal_col AS BIGINT) with scale=0 column
```

### Runtime Errors (Bytecode)

When DECIMAL→BIGINT conversion fails at runtime:

| Scenario | Behavior | Gas Consumed |
|----------|----------|--------------|
| Scale > 0 | Transaction reverts | All gas up to failing opcode |

## Formal Verification Framework

### Theorem Hierarchy

| # | Theorem | Property | Status |
|---|---------|----------|--------|
| T1 | Determinism | Bit-identical results across platforms | Required |
| T2 | Scale Zero Requirement | scale > 0 always produces error | Required |
| T3 | Value Preservation | Valid conversion preserves mantissa value | Required |
| T4 | Sign Preservation | Negative mantissa produces negative BigInt | Required |
| T5 | Zero Canonicalization | Decimal{0, 0} → BigInt::zero() | Required |
| T6 | i128 Range | All i128 mantissas fit in BigInt | Required |

### Theorem Specifications

**Theorem T1 (Determinism):** For identical DECIMAL input, the conversion always produces identical BIGINT output or identical error.

**Theorem T2 (Scale Zero Requirement):** If `d.scale > 0`, then `decimal_to_bigint_full(d) = Err(ConversionLoss)`.

**Theorem T3 (Value Preservation):** If `decimal_to_bigint_full(d) = Ok(b)`, then `b` represents the same integer value as `d.mantissa`.

**Theorem T4 (Sign Preservation):** If `d.mantissa < 0`, then `result.sign = true`.

**Theorem T5 (Zero Canonicalization):** `decimal_to_bigint_full(Decimal { mantissa: 0, scale: 0 }) = Ok(BigInt::zero())`.

**Theorem T6 (i128 Range):** For any i128 mantissa `m`, `decimal_to_bigint_full(Decimal { mantissa: m, scale: 0 })` succeeds (i128 always fits in BigInt).

## Implementation Checklist

| Mission | Description | Status | Complexity |
|---------|-------------|--------|------------|
| M1 | `decimal_to_bigint_full` core algorithm | Pending | Medium |
| M2 | Scale validation (must be 0) | Pending | Low |
| M3 | i128 to BigInt conversion | Pending | Medium |
| M4 | i128::MIN special case handling | Pending | Low |
| M5 | Error type construction | Pending | Low |
| M6 | Test vector suite (20 vectors) | Pending | Medium |
| M7 | Integration with Decimal and BigInt types | Pending | Medium |

## Future Work

- F1: BIGINT→DQA conversion (see RFC-0131)
- F2: DQA→BIGINT conversion (see RFC-0132)
- F3: BIGINT→DECIMAL conversion (see RFC-0133)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.1 | 2026-03-23 | Enhanced: Added Input/Output Contract, Scale Context Propagation, SQL Integration, Constraints, Error Handling & Diagnostics, Formal Verification Framework (6 theorems), Implementation Checklist, expanded test vectors from 9 to 20 |
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
