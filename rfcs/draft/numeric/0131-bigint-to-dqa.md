# RFC-0131 (Numeric/Math): BIGINT to DQA Conversion

## Status

**Version:** 1.7 (Draft)
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
        max_magnitude: u64,  // i64::MAX = 9223372036854775807 as u64 for comparison
        scale: u8,  // Scale that was applied when overflow occurred
    },
    /// Requested scale exceeds DQA's maximum scale (18)
    InvalidScale {
        requested: u8,
        max_scale: u8,
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
/// * `BigIntError::OutOfRange` if |b| > i64::MAX or |b × 10^scale| exceeds i64 range
/// * `BigIntError::InvalidScale` if scale > 18
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

CONVENTION: Per RFC-0110 §Limbs, BigInt uses little-endian limb encoding:
  lo = b.limbs[0]  // Least-significant 64 bits
  hi = b.limbs[1]  // Most-significant 64 bits (if present)
  Implementations MUST use this convention when reading limbs.

STEPS:

1. VALIDATE_INPUT
   If scale > 18:
     return Error(InvalidScale { requested: scale, max_scale: 18 })

   If b.limbs.length > 2:
     // BigInt requires more than 128 bits
     return Error(OutOfRange { attempted_magnitude: b.to_string(), max_magnitude: i64::MAX as u64, scale })

   // Extract limb values (per RFC-0110 little-endian convention defined above)
   lo = b.limbs[0]  // u64

   If b.limbs.length == 2:
     hi = b.limbs[1]  // u64
   Else:
     hi = 0  // Single-limb case

   // Range check for single-limb positive values
   // A positive single limb with high bit set (>= 2^63) overflows i64::MAX
   // i64::MAX = 0x7FFF_FFFF_FFFF_FFFF (2^63 - 1)
   If b.limbs.length == 1 and b.sign == false:
     If lo > 0x7FFF_FFFF_FFFF_FFFF:
       return Error(OutOfRange { attempted_magnitude: b.to_string(), max_magnitude: i64::MAX as u64, scale })

   // Two-limb range check
   // For positive two-limb values: ANY non-zero hi means magnitude >= 2^64 > i64::MAX
   // For negative two-limb values: magnitude > 2^63 overflows; magnitude == 2^63 is i64::MIN (valid)
   If b.limbs.length == 2:
     // Positive: any hi > 0 means magnitude >= 2^64 > i64::MAX
     If b.sign == false and hi > 0:
       return Error(OutOfRange { attempted_magnitude: b.to_string(), max_magnitude: i64::MAX as u64, scale })
     // Negative: magnitude > 2^63 overflows
     If b.sign == true:
       If hi > 0x8000_0000_0000_0000:
         return Error(OutOfRange { attempted_magnitude: b.to_string(), max_magnitude: i64::MAX as u64, scale })
       If hi == 0x8000_0000_0000_0000 and lo > 0:
         return Error(OutOfRange { attempted_magnitude: b.to_string(), max_magnitude: i64::MAX as u64, scale })

2. EXTRACT_UNSCALED_I64
   // Step 1 already validated that the value fits in i64 range
   // This step only extracts the i64 value

   // Extract the i64 value
   If b.sign == false:
     // For positive values that pass Step 1:
     // - Single-limb: unscaled = lo (already range-checked)
     // - Two-limb: hi must be 0 (any hi > 0 for positive was caught in Step 1)
     unscaled = lo as i64
   Else:
     // Handle i64::MIN special case: |i64::MIN| = 2^63 which doesn't fit in u64
     If b.limbs.length == 2 and hi == 0x8000000000000000 and lo == 0:
       unscaled = i64::MIN  // -9223372036854775808
     Else:
       // Use u128 for shift to avoid u64 shift-by-64 UB
       mag = (lo as u128) | ((hi as u128) << 64)
       // mag cannot be 0x8000000000000000 here because Step 1 would have caught it
       unscaled = -(mag as i64)

3. APPLY_SCALE_AND_CHECK_OVERFLOW
   // Multiply by 10^scale and check for overflow
   // i64::MAX = 9223372036854775807
   // i64::MIN = -9223372036854775808

   If scale == 0:
     scaled_value = unscaled
   Else:
     // POW10_TABLE[scale] = 10^scale as u64
     // Precomputed constant table: [1, 10, 100, ..., 10^18]
     // Type is u64 because 10^18 = 10000000000000000000 < u64::MAX
     pow10: u64 = POW10_TABLE[scale]

     // Use u128 intermediate arithmetic for both range check and final multiply
     // This avoids overflow when casting pow10 to i64

     If unscaled >= 0:
       max_allowed = i64::MAX as u128  // 2^63 - 1
       abs_unscaled = unscaled as u128
     Else:
       // For negative, max magnitude is |i64::MIN| = 2^63
       // i64::MIN = -9223372036854775808 has magnitude 2^63 which fits in u128
       max_allowed = 1u128 << 63  // 2^63 = |i64::MIN|
       // Handle i64::MIN specially: its magnitude as u128 is 1 << 63
       // For other negatives: magnitude is (-unscaled) as u128
       If unscaled == i64::MIN:
         abs_unscaled = 1u128 << 63
       Else:
         abs_unscaled = (-unscaled) as u128

     If abs_unscaled * (pow10 as u128) > max_allowed:
       return Error(OutOfRange { attempted_magnitude: b.to_string(), max_magnitude: i64::MAX as u64, scale })

     // Use i128 intermediate to avoid pow10→i64 cast overflow
     // The range check above guarantees the result fits in i64
     scaled_value = ((unscaled as i128) * (pow10 as i128)) as i64

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
| `BigIntError::OutOfRange` | Value exceeds i64 range (before or after scaling) | This RFC |
| `BigIntError::InvalidScale` | Scale > 18 | This RFC |

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

### Lossless Round-Trip Case

Despite the asymmetry above, round-trip IS lossless when **scale=0**:

| Direction | Conversion | Result |
|-----------|------------|--------|
| Forward (RFC-0131) | `BigInt(42), scale=0` → DQA | DQA{42, 0} |
| Reverse (RFC-0132) | `DQA{42, 0}` → BIGINT | BigInt(42) |

**Lossless condition:** `BigInt(x) × 10^0 = x` and DQA extracts raw mantissa, so `BigInt(x)` is recovered exactly when `|x| ≤ i64::MAX` (DQA range).

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
| BIGINT | DQA(n) | Error if \|value\| > i64::MAX | Overflow → TRAP |
| BIGINT | DQA(0) | Error if \|value\| > i64::MAX | Integer representation |
| BIGINT | DQA(18) | Error if \|value\| > i64::MAX | Maximum scale |

**Note:** Unlike DFP→DQA lowering (RFC-0124), BIGINT→DQA does not require rounding because BigInt is already an integer type. The only loss possible is range truncation (overflow).

### Constraints

| Constraint Type | Description |
|----------------|-------------|
| **Scale bounds** | 0 ≤ scale ≤ 18 (per RFC-0105 MAX_SCALE) |
| **Pre-scale range** | \|BigInt\| ≤ i64::MAX — checked in Step 1 before scaling |
| **Post-scale range** | \|BigInt × 10^scale\| ≤ i64::MAX (positive) or ≤ \|i64::MIN\| (negative) — checked in Step 3 |
| **Overflow policy** | Error on overflow (no truncation, no saturation) |
| **BigInt size** | 1-2 limbs (64-128 bits). 3+ limbs always rejected in Step 1 |
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

### V008: Overflow — 2^64 Magnitude
```
Input:  BigInt { limbs: [0, 1], sign: false }, scale = 0
Output: Error(OutOfRange)
Note: 2^64 > i64::MAX — little-endian: limbs[0]=0 (lo), limbs[1]=1 (hi)
```

### V009: Overflow — Negative 2^64 Magnitude
```
Input:  BigInt { limbs: [0, 1], sign: true }, scale = 0
Output: Error(OutOfRange)
Note: |2^64| > i64::MAX after sign adjustment
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
Input:  BigInt { limbs: [0x0000000000000000, 0x0000000000000001], sign: false }, scale = 0
Output: Error(OutOfRange)
Note: 2^64 = 18446744073709551616 > i64::MAX
Note: limbs[0]=0 (lo), limbs[1]=1 (hi) per RFC-0110 little-endian
```

### V017: Overflow — 2^63 Exactly
```
Input:  BigInt { limbs: [0x0000000000000000, 0x8000000000000000], sign: false }, scale = 0
Output: Error(OutOfRange)
Note: 2^63 = 9223372036854775808. This magnitude equals |i64::MIN| but as a
positive value it exceeds i64::MAX (9223372036854775807), causing overflow.
Note: limbs[0]=0 (lo), limbs[1]=0x8000... (hi) per RFC-0110 little-endian
```

### V018: Negative Overflow — Magnitude Exceeds MAX
```
Input:  BigInt { limbs: [0x0000000000000001, 0x0000000000000001], sign: true }, scale = 0
Output: Error(OutOfRange)
Note: (2^64 + 1) = 18446744073709551617 > i64::MAX
```

### V019: Single Limb Positive (Within i64 Range)
```
Input:  BigInt { limbs: [0x123456789ABCDEF0], sign: false }, scale = 0
Output: Dqa { value: 0x123456789ABCDEF0, scale: 0 }
Note: Value 1311768467294899440 < i64::MAX, fits in i64
```

### V020: Single Limb Negative
```
Input:  BigInt { limbs: [0x123456789ABCDEF0], sign: true }, scale = 0
Output: Dqa { value: -0x123456789ABCDEF0, scale: 0 }
Note: Fits in i64 range
```

### V035: Single Limb Positive — Overflow at 2^63
```
Input:  BigInt { limbs: [0x8000000000000001], sign: false }, scale = 0
Output: Error(OutOfRange)
Note: 2^63 + 1 = 9223372036854775809 > i64::MAX
This is the single-limb case: high bit set means magnitude > i64::MAX
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
Note: |-93| × 10^17 = 9.3 × 10^18 > 2^63 = |i64::MIN|
```

### V032: Scale Multiplication Edge — 9 × 10^18 (Fits)
```
Input:  BigInt::from(9i64), scale = 18
Output: Dqa { value: 9000000000000000000, scale: 18 }
Note: 9 × 10^18 = 9 × 10^18 = 9000000000000000000, fits in i64
```

### V033: Scale Multiplication Edge — 92 × 10^17 (Fits)
```
Input:  BigInt::from(92i64), scale = 17
Output: Dqa { value: 9200000000000000000, scale: 17 }
Note: 92 × 10^17 = 9.2 × 10^18 = 9200000000000000000, exactly fits in i64
```

### V034: Negative with Scale > 0 — Success
```
Input:  BigInt::from(-1i64), scale = 1
Output: Dqa { value: -10, scale: 1 }
Note: |-1| × 10^1 = 10, which fits in i64 range
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

BIGINT→DQA conversion is a constant-time operation regardless of BigInt size because the algorithm only inspects the first 2 limbs. Gas cost should be:
```
GAS = 12  // Fixed cost, no variable component
```

This accounts for:
- Constant-time limb inspection (only 1-2 limbs accessed)
- Range checks and scale validation
- Note: BigInts with more than 2 limbs are rejected early without iterating

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

**Theorem T4 (Overflow Completeness):** If `b × 10^s < i64::MIN` OR `b × 10^s > i64::MAX`, then `bigint_to_dqa(b, s) = Err(OutOfRange)`.

**Theorem T5 (Scale Bounds):** If `s > 18`, then `bigint_to_dqa(b, s) = Err(InvalidScale)`.

## Implementation Checklist

| Mission | Description | Status | Complexity |
|---------|-------------|--------|------------|
| M1 | `bigint_to_dqa` core algorithm | Pending | Medium |
| M2 | Scale validation (0-18 bounds) | Pending | Low |
| M3 | Limb inspection and range check | Pending | Medium |
| M4 | i64::MIN special case handling | Pending | Low |
| M5 | Error type construction | Pending | Low |
| M6 | Test vector suite (35 vectors) | Pending | Medium |
| M7 | Integration with BigInt type | Pending | Medium |
| M8 | Fuzz testing for edge cases | Pending | Medium |

## Future Work

- F1: DQA→BIGINT conversion (see RFC-0132)
- F2: BIGINT→DECIMAL conversion (see RFC-0133)
- F3: DECIMAL→BIGINT conversion (see RFC-0134)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.7 | 2026-03-23 | LOW: Added lossless round-trip case documentation — scale=0 preserves value exactly (R3L4). |
| 1.6 | 2026-03-23 | CRITICAL: Fixed `pow10 as i64` overflow — Step 3 now uses i128 intermediate for multiplication (R3C1). HIGH: Fixed T4 theorem to use signed range (R3H1). MEDIUM: Fixed function doc error comment (R3M2), Constraints table (R3M1), V008/V009 limb arrays (R3M3). LOW: V020b→V035, checklist count 35 (R3L1/M4), removed dead BigIntToDqaOutput enum (R3L2). |
| 1.4 | 2026-03-23 | Critical fixes: Added explicit limb convention per RFC-0110 (CRITICAL-C1), fixed single-limb range check hole (CRITICAL-C2), fixed unscanned typo (CRITICAL-C3), fixed negative×scale overflow (HIGH-H3), fixed max_magnitude type (HIGH-H4), fixed V016/V017 limb arrays (LOW-L1/L2), added V020b and V034 test vectors, updated gas model |
| 1.3 | 2026-03-23 | Critical fix: Added sign-aware boundary check for positive 2^63 overflow (CRITICAL-1), fixed V025 which incorrectly claimed success for i64::MAX×scale-18, removed duplicate range check between Steps 1 and 2, fixed V033 note arithmetic |
| 1.2 | 2026-03-23 | Critical fix: Added scale multiplication step to algorithm (was missing), added overflow check for scaled values, fixed V011 and Edge Cases zero handling to be consistent, fixed V017 note, added V029-V033 for scale overflow test vectors, added scale field to OutOfRange error |
| 1.1 | 2026-03-23 | Enhanced: Added Input/Output Contract, Scale Context Propagation, SQL Integration, Constraints, Error Handling & Diagnostics, Formal Verification Framework (5 theorems), Implementation Checklist, expanded test vectors from 10 to 28 |
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
