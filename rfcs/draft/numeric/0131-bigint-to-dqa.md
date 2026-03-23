# RFC-0131 (Numeric/Math): BIGINT to DQA Conversion

## Status

**Version:** 1.2 (Draft)
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

### RFC-0105 and RFC-0110 Coverage Analysis

| Conversion | RFC-0105 (DQA) | RFC-0110 (BIGINT) | This RFC |
|------------|-----------------|-------------------|----------|
| i128 → DQA | Not specified | `bigint_to_dqa(i128)` | Not needed |
| DQA → i128 | Not specified | `dqa_to_bigint()` returns i128 | Not needed |
| BigInt → DQA | Not specified | Not specified | **This RFC** |
| DQA → BigInt | Not specified | Not specified | See RFC-0132 |

**Key insight:** The existing i128↔DQA conversions in RFC-0110 are insufficient because BigInt can represent values up to 4096 bits (128 decimal digits), while i128 only handles 39 decimal digits. BigInt→DQA requires range checking against i64 bounds.

### Why Not Reuse Existing Functions?

| Approach | Problem |
|----------|---------|
| Extend `bigint_to_dqa(i128)` | Would break RFC-0110 compliance |
| Add `ToDqa` trait to BigInt | Doesn't specify error handling |
| Inline conversion in Stoolap | Non-deterministic across implementations |

This RFC provides a canonical specification that:
1. Preserves existing RFC-0110 i128→DQA function
2. Adds new BigInt→DQA with proper range checking
3. Specifies deterministic error handling

## Input/Output Contract

```rust
/// Error variants for BIGINT→DQA conversion
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BigIntError {
    /// BigInt value exceeds DQA's representable range (i64::MIN to i64::MAX).
    /// This can occur from two sources:
    /// (1) The BigInt itself exceeds i64 range before scaling
    /// (2) The scaled value (BigInt × 10^scale) exceeds i64 range
    OutOfRange {
        attempted_magnitude: String,  // Debug representation of the BigInt
        max_magnitude: i64,
        scale: u8,  // Scale that was applied when overflow occurred
    },
    /// Requested scale exceeds DQA's maximum scale (18)
    InvalidScale {
        requested: u8,
        max_scale: u8,
    },
    /// Input BigInt is not in canonical form per RFC-0110
    NonCanonicalInput {
        reason: &'static str,
    },
}

/// BIGINT→DQA conversion result
pub type BigIntToDqaResult = Result<Dqa, BigIntError>;

/// Input to the conversion
pub struct BigIntToDqaInput {
    /// The BigInt value to convert
    pub value: BigInt,
    /// Target scale for the DQA result (0-18)
    pub scale: u8,
}

/// Output from the conversion
pub enum BigIntToDqaOutput {
    /// Successfully converted to DQA
    Success(Dqa),
    /// Conversion error with details
    Error(BigIntError),
}
```

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

1. VALIDATE_INPUT
   If scale > 18:
     return Error(InvalidScale)

   If b.limbs.length > 2:
     // BigInt requires more than 128 bits
     return Error(OutOfRange)

   // Special case: positive 2^63 (hi=0x8000..., lo=0) overflows i64
   // because i64::MAX = 2^63 - 1
   // Note: negative 2^63 (hi=0x8000..., lo=0, sign=true) is valid (i64::MIN)
   If b.limbs.length == 2 and b.sign == false:
     If hi == 0x8000_0000_0000_0000 and lo == 0:
       return Error(OutOfRange)

   If b.limbs.length == 2:
     // Check if magnitude fits in i64 range
     // i64::MAX = 2^63 - 1, i64::MIN = -2^63
     // Magnitude boundary: 2^63
     // For positive: magnitude must be < 2^63 (magnitude >= 2^63 overflows)
     // For negative: magnitude must be <= 2^63 (allows -2^63 = i64::MIN)
     //
     // Combined check: if hi > 0x8000... or (hi == 0x8000... and lo > 0)
     // This catches all values with magnitude >= 2^63
     If hi > 0x8000_0000_0000_0000:
       return Error(OutOfRange)
     If hi == 0x8000_0000_0000_0000 and lo > 0:
       return Error(OutOfRange)
     // Note: hi >= 0x8000_...0001 is already caught by hi > 0x8000_...

2. EXTRACT_UNSCALED_I64
   // Step 1 already validated that the value fits in i64 range
   // This step only extracts the i64 value

   // Extract the i64 value
   If b.sign == false:
     unscanned = lo | (hi << 64)
   Else:
     // Handle i64::MIN special case: |i64::MIN| = 2^63 which doesn't fit in u64
     If hi == 0x8000000000000000 and lo == 0:
       unscanned = i64::MIN  // -9223372036854775808
     Else:
       mag = lo | (hi << 64)
       // mag cannot be 0x8000000000000000 here because Step 1 would have caught it
       unscanned = -(mag as i64)

3. APPLY_SCALE_AND_CHECK_OVERFLOW
   // Multiply by 10^scale and check for overflow
   // i64::MAX = 9223372036854775807
   // i64::MIN = -9223372036854775808
   // Note: |i64::MIN| = i64::MAX + 1

   If scale == 0:
     scaled_value = unscanned
   Else:
     pow10 = 10^scale

     // Check overflow before multiplying
     // For positive: abs_unscanned * pow10 <= i64::MAX
     // For negative: abs_unscanned * pow10 <= i64::MAX + 1

     If unscanned >= 0:
       max_allowed = i64::MAX
     Else:
       // Can represent |i64::MIN| = 2^63 = i64::MAX + 1
       max_allowed = 9223372036854775808u128  // Use u128 for intermediate

     abs_unscanned = |unscanned| as u128
     If abs_unscanned * (pow10 as u128) > (max_allowed as u128):
       return Error(OutOfRange)

     scaled_value = unscanned * pow10

4. CONSTRUCT_DQA
   Return Dqa { value: scaled_value, scale: scale }
```

### Edge Cases

| BigInt Input | Scale | DQA Output | Notes |
|-------------|-------|------------|-------|
| 0 | any | Dqa { 0, scale } | Zero preserves scale |
| i64::MAX | 0 | Dqa { i64::MAX, 0 } | Maximum representable |
| i64::MIN | 0 | Dqa { i64::MIN, 0 } | Minimum representable |
| 42 | 2 | Dqa { 4200, 2 } | Scale adjustment (×10^2) |
| 42 | 18 | Dqa { 4200000000000000000, 18 } | Scale ×10^18 |
| -42 | 3 | Dqa { -42000, 3 } | Negative with scale |
| BigInt with 3+ limbs | any | Error(OutOfRange) | Exceeds i64 |

### Error Handling

| Error | Condition | RFC Reference |
|-------|-----------|--------------|
| `BigIntError::OutOfRange` | Value exceeds i64 range | This RFC |
| `BigIntError::InvalidScale` | Scale > 18 | This RFC |
| `BigIntError::NonCanonicalInput` | Input BigInt not canonical | RFC-0110 |

### Scale Context Propagation

The scale parameter in BIGINT→DQA conversion has specific semantics:

| Scale Context | Behavior |
|--------------|----------|
| Explicit scale provided | Value is multiplied by 10^scale |
| Default scale (0) | Integer representation, no decimal places |
| Scale 2 with BigInt(1999) | DQA{199900, 2} represents $19.99 |
| Scale 18 with BigInt(1) | DQA{1000000000000000000, 18} represents 1.0 |

**Scale adjustment:** The BigInt value is multiplied by 10^scale to produce the DQA mantissa. This is necessary because DQA's value = mantissa × 10^(-scale). For example:
- `BigInt(1999)` with scale 2 → DQA mantissa = 1999 × 10^2 = 199900
- DQA{199900, 2} = 199900 × 10^(-2) = 19.99

This is different from DECIMAL where scale is metadata about precision. Here, scale is part of the DQA type definition per RFC-0105 and affects the mantissa value directly.

## Round-Trip Asymmetry

This conversion is NOT the inverse of RFC-0132's DQA→BIGINT:

| Direction | Conversion | Result |
|-----------|------------|--------|
| Forward (RFC-0131) | `BigInt(1999), scale=2` → DQA | DQA{199900, 2} |
| Reverse (RFC-0132) | `DQA{1999, 2}` → BIGINT | BigInt(1999) |

Round-trip: `BigInt(1999), scale=2` → DQA{199900, 2} → BigInt(199900) ≠ original

This asymmetry is intentional because:
1. BIGINT→DQA (RFC-0131) **multiplies** the mantissa by 10^scale
2. DQA→BIGINT (RFC-0132) **ignores** the scale, extracting raw mantissa
3. Scale information is LOST in the DQA→BIGINT direction

**Implication:** You cannot round-trip a scaled value through both conversions and expect to recover the original. If you need to preserve scale, you must track it separately.

### SQL Integration

BIGINT→DQA conversion appears in SQL CAST expressions:

```sql
-- Explicit CAST from BIGINT to DQA with scale
SELECT CAST(bigint_col AS DQA(6)) FROM account_balances;

-- This is VALID: BigInt value must fit in i64 range
-- If bigint_col = 9223372036854775807 (i64::MAX), conversion succeeds
-- If bigint_col = 9223372036854775808 (i64::MAX + 1), error

-- Scale 2 for currency representation
SELECT CAST(bigint_col AS DQA(2)) FROM currency_amounts;
-- BigInt(1999) with scale 2 → DQA represents $19.99

-- FORBIDDEN: Explicit CAST from oversized BigInt
SELECT CAST(huge_bigint_col AS DQA(0)) FROM large_values;
-- Error: BigIntError::OutOfRange
```

#### Cast Semantics in Deterministic Context

| Source Type | Target Type | Behavior | Notes |
|-------------|-------------|----------|-------|
| BIGINT | DQA(n) | Truncate if \|value\| > i64::MAX | Overflow → error |
| BIGINT | DQA(0) | Truncate if \|value\| > i64::MAX | Integer representation |
| BIGINT | DQA(18) | Truncate if \|value\| > i64::MAX | Maximum scale |

**Note:** Unlike DFP→DQA lowering (RFC-0124), BIGINT→DQA does not require rounding because BigInt is already an integer type. The only loss possible is range truncation (overflow).

### Constraints

| Constraint Type | Description |
|----------------|-------------|
| **Scale bounds** | 0 ≤ scale ≤ 18 (per RFC-0105 MAX_SCALE) |
| **Value bounds (unscaled)** | \|BigInt\| ≤ i64::MAX for extraction |
| **Scaled value bounds** | \|BigInt × 10^scale\| ≤ i64::MAX (or i64::MAX+1 for negatives) |
| **Overflow policy** | Error on overflow (no truncation, no saturation) |
| **BigInt size** | 1-2 limbs (64-128 bits). 3+ limbs always overflow. |
| **Determinism** | Identical BigInt input always produces identical DQA output |
| **No rounding** | BIGINT→DQA does not round; it traps on overflow |

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

### V011: Maximum Scale (18)
```
Input:  BigInt::from(1i64), scale = 18
Output: Dqa { value: 1000000000000000000, scale: 18 }
Note: 1 × 10^18 = 1000000000000000000, fits in i64
```

### V012: Negative with Scale
```
Input:  BigInt::from(-100i64), scale = 4
Output: Dqa { value: -1000000, scale: 4 }
Note: -100 * 10^4 = -1000000
```

### V013: i64 Boundary — One Less Than MAX
```
Input:  BigInt::from(9223372036854775806i64), scale = 0
Output: Dqa { value: 9223372036854775806, scale: 0 }
Note: i64::MAX - 1, still fits
```

### V014: i64 Boundary — One More Than MIN
```
Input:  BigInt::from(-9223372036854775807i64), scale = 0
Output: Dqa { value: -9223372036854775807, scale: 0 }
Note: i64::MIN + 1, still fits
```

### V015: Scale 1 Edge Case
```
Input:  BigInt::from(10i64), scale = 1
Output: Dqa { value: 100, scale: 1 }
Note: 10 * 10^1 = 100, represents 10.0
```

### V016: Overflow — 128-bit Value (2 limbs, exceeds i64)
```
Input:  BigInt { limbs: [0x0000000000000001, 0x0000000000000000], sign: false }, scale = 0
Output: Error(OutOfRange)
Note: 2^64 = 18446744073709551616 > i64::MAX
```

### V017: Overflow — 2^63 Exactly
```
Input:  BigInt { limbs: [0x0000000000000000, 0x0000000000000001], sign: false }, scale = 0
Output: Error(OutOfRange)
Note: 2^63 = 9223372036854775808. This magnitude equals |i64::MIN| but as a
positive value it exceeds i64::MAX (9223372036854775807), causing overflow.
```

### V018: Negative Overflow — Magnitude Exceeds MAX
```
Input:  BigInt { limbs: [0x0000000000000001, 0x0000000000000001], sign: true }, scale = 0
Output: Error(OutOfRange)
Note: (2^64 + 1) = 18446744073709551617 > i64::MAX
```

### V019: Single Limb Positive
```
Input:  BigInt { limbs: [0x123456789ABCDEF0], sign: false }, scale = 0
Output: Dqa { value: 0x123456789ABCDEF0, scale: 0 }
Note: Single limb always fits in i64
```

### V020: Single Limb Negative
```
Input:  BigInt { limbs: [0x123456789ABCDEF0], sign: true }, scale = 0
Output: Dqa { value: -0x123456789ABCDEF0, scale: 0 }
Note: Fits in i64 range
```

### V021: Invalid Scale — Exceeds 18
```
Input:  BigInt::from(42i64), scale = 19
Output: Error(InvalidScale)
Note: DQA max scale is 18
```

### V022: Zero with Non-Zero Scale
```
Input:  BigInt::zero(), scale = 6
Output: Dqa { value: 0, scale: 6 }
Note: Canonical zero has value 0, scale preserved
```

### V023: Large Currency Value
```
Input:  BigInt::from(1000000i64), scale = 2
Output: Dqa { value: 100000000, scale: 2 }
Note: 1,000,000.00 in dollars = 100,000,000 cents
```

### V024: i64::MIN Exactly
```
Input:  BigInt::from(i64::MIN), scale = 0
Output: Dqa { value: -9223372036854775808, scale: 0 }
Note: Special case -i64::MIN = i64::MIN in unsigned magnitude
```

### V025: Scale Multiplication Overflow — i64::MAX × 10^18
```
Input:  BigInt::from(9223372036854775807i64), scale = 18
Output: Error(OutOfRange)
Note: i64::MAX × 10^18 = 9.22... × 10^36 > i64::MAX (9.22... × 10^18)
```

### V026: Scale Boundary — 0 (Min)
```
Input:  BigInt::from(-9223372036854775808i64), scale = 0
Output: Dqa { value: -9223372036854775808, scale: 0 }
Note: Minimum value with minimum scale
```

### V027: Overflow — Exceeds i64::MAX by 1
```
Input:  BigInt { limbs: [1, 0x8000000000000000], sign: false }, scale = 0
Output: Error(OutOfRange)
Note: Magnitude = 2^63 + 1 > i64::MAX
```

### V028: Negative Overflow — Exceeds i64::MIN by 1
```
Input:  BigInt { limbs: [1, 0x8000000000000000], sign: true }, scale = 0
Output: Error(OutOfRange)
Note: |value| = 2^63 + 1 > i64::MAX magnitude
```

### V029: Scale Multiplication Overflow — 93 × 10^17
```
Input:  BigInt::from(93i64), scale = 17
Output: Error(OutOfRange)
Note: 93 × 10^17 = 9.3 × 10^18 > i64::MAX (9.2 × 10^18)
```

### V030: Scale Multiplication Overflow — 10 × 10^18
```
Input:  BigInt::from(10i64), scale = 18
Output: Error(OutOfRange)
Note: 10 × 10^18 = 10^19 > i64::MAX (9.2 × 10^18)
```

### V031: Scale Multiplication Overflow — Negative
```
Input:  BigInt::from(-93i64), scale = 17
Output: Error(OutOfRange)
Note: |-93| × 10^17 = 9.3 × 10^18 > i64::MAX
```

### V032: Scale Multiplication Edge — 9 × 10^18 (Fits)
```
Input:  BigInt::from(9i64), scale = 18
Output: Dqa { value: 9000000000000000000, scale: 18 }
Note: 9 × 10^18 = 9 × 10^18, exactly equals i64::MAX - 2^63 + 9
```

### V033: Scale Multiplication Edge — 92 × 10^17 (Fits)
```
Input:  BigInt::from(92i64), scale = 17
Output: Dqa { value: 9200000000000000000, scale: 17 }
Note: 92 × 10^17 = 9.2 × 10^18 = 9200000000000000000, exactly fits in i64
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

## Error Handling and Diagnostics

### Compile-Time Errors

When BIGINT→DQA conversion fails at compile time (e.g., explicit CAST), the compiler emits:

```
ERROR: Cannot convert BIGINT to DQA
  Expression: CAST(bigint_col AS DQA(0)) at line 42
  Reason: BigIntError::OutOfRange — value 9223372036854775808 exceeds i64::MAX
  Hint: Use BIGINT type or reduce the value

ERROR: Cannot convert BIGINT to DQA
  Expression: CAST(value AS DQA(19)) at line 15
  Reason: BigIntError::InvalidScale — scale 19 exceeds maximum (18)
  Hint: Use scale 0-18 for DQA type
```

### Runtime Errors (Bytecode)

When BIGINT→DQA conversion fails at runtime (e.g., computed value exceeds range):

| Scenario | Behavior | Gas Consumed |
|----------|----------|--------------|
| Overflow | Transaction reverts | All gas up to and including failing opcode |
| Invalid scale | Transaction reverts | All gas up to and including failing opcode |

**Note:** Unlike DFP→DQA (RFC-0124), BIGINT→DQA conversion always succeeds for valid inputs. Errors only occur on overflow or invalid scale.

## Formal Verification Framework

### Theorem Hierarchy

| # | Theorem | Property | Status |
|---|---------|----------|--------|
| T1 | Determinism | Bit-identical results across platforms | Required |
| T2 | Range Preservation | If result is Ok, value is within i64 bounds | Required |
| T3 | Scale Preservation | Output scale equals input scale | Required |
| T4 | Overflow Completeness | No false negatives: overflow always detected | Required |
| T5 | Scale Bounds | Scale validation is correct | Required |

### Theorem Specifications

**Theorem T1 (Determinism):** For identical BigInt input and scale, the conversion always produces identical DQA output or identical error.

**Theorem T2 (Range Preservation):** If `bigint_to_dqa(b, s) = Ok(dqa)`, then `|dqa.value| ≤ i64::MAX`.

**Theorem T3 (Scale Preservation):** If `bigint_to_dqa(b, s) = Ok(dqa)`, then `dqa.scale = s`.

**Theorem T4 (Overflow Completeness):** If `|b| > i64::MAX`, then `bigint_to_dqa(b, s) = Err(OutOfRange)`.

**Theorem T5 (Scale Bounds):** If `s > 18`, then `bigint_to_dqa(b, s) = Err(InvalidScale)`.

## Implementation Checklist

| Mission | Description | Status | Complexity |
|---------|-------------|--------|------------|
| M1 | `bigint_to_dqa` core algorithm | Pending | Medium |
| M2 | Scale validation (0-18 bounds) | Pending | Low |
| M3 | Limb inspection and range check | Pending | Medium |
| M4 | i64::MIN special case handling | Pending | Low |
| M5 | Error type construction | Pending | Low |
| M6 | Test vector suite (28 vectors) | Pending | Medium |
| M7 | Integration with BigInt type | Pending | Medium |
| M8 | Fuzz testing for edge cases | Pending | Medium |

## Future Work

- F1: DQA→BIGINT conversion (see RFC-0132)
- F2: BIGINT→DECIMAL conversion (see RFC-0133)
- F3: DECIMAL→BIGINT conversion (see RFC-0134)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.3 | 2026-03-23 | Critical fix: Added sign-aware boundary check for positive 2^63 overflow (CRITICAL-1), fixed V025 which incorrectly claimed success for i64::MAX×scale-18, removed duplicate range check between Steps 1 and 2, fixed V033 note arithmetic |
| 1.2 | 2026-03-23 | Critical fix: Added scale multiplication step to algorithm (was missing), added overflow check for scaled values, fixed V011 and Edge Cases zero handling to be consistent, fixed V017 note, added V029-V033 for scale overflow test vectors, added scale field to OutOfRange error |
| 1.1 | 2026-03-23 | Enhanced: Added Input/Output Contract, Scale Context Propagation, SQL Integration, Constraints, Error Handling & Diagnostics, Formal Verification Framework (5 theorems), Implementation Checklist, expanded test vectors from 10 to 28 |
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
