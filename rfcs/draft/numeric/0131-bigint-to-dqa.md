# RFC-0131 (Numeric/Math): BIGINT to DQA Conversion

## Status

**Version:** 1.27 (Draft)
**Status:** Draft
**Depends On:** RFC-0110 (BIGINT), RFC-0105 (DQA), RFC-0132 (DQA→BigInt for BigIntWithScale type)
**Category:** Numeric/Math

## Summary

This RFC specifies the conversion algorithm from BIGINT (RFC-0110, arbitrary-precision integer up to 4096 bits) to DQA (RFC-0105, i64 with 0-18 decimal scale). This conversion is necessary for the Numeric Tower to support operations that require BIGINT values to be used in DQA contexts, and for explicit CAST expressions between these types.

The conversion TRAPs if the BIGINT value exceeds the representable DQA range (i64::MIN to i64::MAX).

**Important: CANONICALIZE is applied in Step 4.** After multiplying by 10^scale, CANONICALIZE per RFC-0105 strips trailing decimal zeros from the mantissa, typically reducing output scale to 0. The `scale` parameter is an overflow threshold exponent — it controls the range limit for the intermediate scaled value, not the output scale. The output scale is always determined by CANONICALIZE. Callers needing a specific output scale (e.g., SQL columns) must re-apply it after conversion using operations defined in RFC-0105.

**Cross-RFC Conformance:** Full DQA → BigIntWithScale → DQA round-trip requires passing RFC-0132 V201-V203 (forward) and RFC-0131 V401-V410 (backward) suites.

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
pub enum BigIntToDqaError {
    /// BigInt value exceeds DQA's representable range (i64::MIN to i64::MAX).
    /// This can occur from two sources:
    /// (1) The BigInt itself exceeds i64 range before scaling
    /// (2) The scaled value (BigInt × 10^scale) exceeds i64 range
    OutOfRange {
        attempted_magnitude: String,  // Debug representation of the BigInt
        /// Overflow limit for comparison:
        /// - Positive overflow: i64::MAX = 9223372036854775807 (2^63 - 1)
        /// - Negative overflow: |i64::MIN| = 9223372036854775808 (2^63)
        /// Note: The negative limit is 1 larger than positive limit
        max_magnitude: u64,
        scale: u8,  // Scale that was applied when overflow occurred
    },
    /// Requested scale exceeds DQA's maximum scale (18)
    InvalidScale {
        requested: u8,
        max_scale: u8,
    },
}

/// BIGINT→DQA conversion result
pub type BigIntToDqaResult = Result<Dqa, BigIntToDqaError>;
```

### Function Signature

```rust
/// Convert BigInt to DQA with the given overflow scale.
///
/// TRAPs if the BigInt value does not fit in i64 range.
/// The overflow_scale parameter sets the range limit for the intermediate scaled value
/// (|b × 10^overflow_scale| must not exceed i64 bounds). The output scale is determined
/// by CANONICALIZE per RFC-0105, not by this parameter — trailing decimal zeros
/// are always stripped, typically reducing output scale to 0.
///
/// # Arguments
/// * `b` - The BigInt value to convert
/// * `overflow_scale` - Overflow threshold exponent (0-18); the actual output scale may be lower
///
/// # Errors
/// * `BigIntToDqaError::OutOfRange` if |b| > i64::MAX or |b × 10^overflow_scale| exceeds i64 range
/// * `BigIntToDqaError::InvalidScale` if overflow_scale > 18
///
/// # Example
/// BigInt(42) with overflow_scale 0 → Dqa { value: 42, scale: 0 }
/// BigInt(42) with overflow_scale 2 → Dqa { value: 42, scale: 0 }
/// Note: CANONICALIZE strips trailing zeros, so {4200, 2} becomes {42, 0}
pub fn bigint_to_dqa(b: &BigInt, overflow_scale: u8) -> Result<Dqa, BigIntToDqaError>
```

### Canonical Conversion Algorithm

```
BIGINT_TO_DQA(b: BigInt, overflow_scale: u8) -> Result<Dqa, BigIntToDqaError>

INPUT:  b (BigInt), overflow_scale (u8, 0 ≤ overflow_scale ≤ 18)
OUTPUT: Dqa { value: i64, scale: u8 } or error

CONVENTION: Per RFC-0110 §Limbs, BigInt uses little-endian limb encoding:
  lo = b.limbs[0]  // Least-significant 64 bits
  hi = b.limbs[1]  // Most-significant 64 bits (if present)
  Implementations MUST use this convention when reading limbs.

STEPS:

0. VERIFY_CANONICAL
   // Per RFC-0110 §Input Canonicalization Requirement, BigInt inputs MUST be canonical.
   // Non-canonical inputs are undefined behavior — implementations MUST TRAP.
   // Do NOT rely on upstream components having canonicalized.
   If b.limbs.length == 0:
     // Empty limb slice is non-canonical. RFC-0110 defines canonical zero as {limbs: [0], sign: false}.
     // Accessing b.limbs[0] on an empty slice would panic — guard first.
     TRAP
   If b.limbs.length == 2 and b.limbs[1] == 0:
     // Non-canonical: hi==0 means this should be a single-limb BigInt, regardless of sign.
     // Positive hi==0 is non-canonical (should use sign=false, lo=value);
     // Negative hi==0 is non-canonical (should be single-limb with sign=true, lo=magnitude).
     // Values with magnitude < 2^63 must use single-limb representation.
     TRAP
   If b.sign == true and b.limbs.length == 1 and b.limbs[0] == 0:
     // Negative zero (sign=true, limbs=[0]) is non-canonical per RFC-0110.
     // Canonical zero is always sign=false. Implementations MUST TRAP.
     TRAP
   // Note: RFC-0110 canonical form also requires that positive zero uses sign=false.
   // Single-limb with lo=0 and sign=false is canonical zero — no action needed.
   If b.limbs.length > 64:
     // Non-canonical: exceeds MAX_LIMBS per RFC-0110.
     // Note: This check and the Step 1 `> 2` check serve different purposes.
     // Step 0's >64 guards against non-canonical BigInts that violate RFC-0110's
     // maximum limb bound (pathological inputs). Step 1's >2 guards against values
     // that exceed i64's representable range (overflow). Any input with >2 limbs
     // fails both checks, but the >64 check catches non-canonical inputs earlier
     // in Step 0 (TRAP) rather than later in Step 1 (Error).
     TRAP

1. VALIDATE_INPUT
   If overflow_scale > 18:
     return Error(InvalidScale { requested: overflow_scale, max_scale: 18 })

   If b.limbs.length > 2:
     // BigInt requires more than 128 bits
     return Error(OutOfRange { attempted_magnitude: "<magnitude>", max_magnitude: i64::MAX as u64, overflow_scale })

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
       return Error(OutOfRange { attempted_magnitude: "<magnitude>", max_magnitude: i64::MAX as u64, overflow_scale })

   // Single-limb negative range check
   // Valid negative range: i64::MIN (0x8000_0000_0000_0000) to -1
   // i64::MIN magnitude = 2^63; anything larger overflows
   //
   // ⚠ CRITICAL: We use ">" (strictly greater) because lo == 0x8000... is EXACTLY i64::MIN,
   // which is valid and must pass through to Step 2 for special handling. The special case
   // lo == 0x8000... is handled in Step 2 where we need it for negation (because
   // -(i64::MIN) would overflow). Do NOT change this to ">=" or you will reject i64::MIN.
   If b.limbs.length == 1 and b.sign == true:
     If lo > 0x8000_0000_0000_0000:
       return Error(OutOfRange { attempted_magnitude: "<magnitude>", max_magnitude: 1u64 << 63, overflow_scale })

   // Two-limb range check
   // For positive two-limb values: ANY non-zero hi means magnitude >= 2^64 > i64::MAX
   // For negative two-limb values: ALL 2-limb negatives are rejected unconditionally because
   // the minimum magnitude is 2^64 (when lo=0, hi=1), which exceeds |i64::MIN| = 2^63.
   // Unlike positive 2-limb values (which need hi>0 check), ALL 2-limb negatives overflow
   // regardless of lo value. Note: i64::MIN's canonical BigInt representation is always
   // single-limb per RFC-0110.
   //
   // Note: Since Step 0 already traps on hi==0 (non-canonical), any two-limb input reaching
   // Step 1 has hi ≥ 1. The hi > 0 check for positive is therefore redundant but kept for
   // clarity and defense-in-depth.
   If b.limbs.length == 2:
     // Positive: any hi > 0 means magnitude >= 2^64 > i64::MAX
     If b.sign == false and hi > 0:
       return Error(OutOfRange { attempted_magnitude: "<magnitude>", max_magnitude: i64::MAX as u64, overflow_scale })
     // Negative: any 2-limb negative has magnitude >= 2^64 > |i64::MIN|
     If b.sign == true:
       return Error(OutOfRange { attempted_magnitude: "<magnitude>", max_magnitude: 1u64 << 63, overflow_scale })

2. EXTRACT_UNSCALED_I64
   // Step 1 validated the value fits in i64 range and rejected all non-canonical inputs.
   // Step 2 only handles single-limb extraction. (All 2-limb values are rejected in Step 1.)
   // Key invariant: lo == 0x8000... with sign=true passed Step 1's check (it uses ">" not ">=")
   // and is therefore exactly i64::MIN, which we handle specially here.

   // Extract the i64 value
   // Single-limb: value is lo (already range-checked in Step 1)
   // Apply sign: for negatives, negate the magnitude.
   // Special case: i64::MIN (0x8000_0000_0000_0000) cannot be negated directly
   // because -i64::MIN overflows in two's complement. Since Step 1 allows lo == 0x8000...
   // (via ">" not ">="), this case is exactly i64::MIN and is handled by direct assignment.
   If b.sign:
     If lo == 0x8000_0000_0000_0000:
       unscaled = i64::MIN  // Can't negate directly; this IS the correct value
     Else:
       unscaled = -(lo as i64)
   Else:
     unscaled = lo as i64

3. APPLY_SCALE_AND_CHECK_OVERFLOW
   // Multiply by 10^overflow_scale and check for overflow
   // i64::MAX = 9223372036854775807
   // i64::MIN = -9223372036854775808

   If overflow_scale == 0:
     scaled_value = unscaled
   Else:
     // Note: When unscaled = 0, the positive branch sets abs_unscaled = 0.
     // Zero passes the range check (0 × pow10 = 0 ≤ max_allowed) and
     // scaled_value = 0. This is correct — 0 × 10^overflow_scale = 0 for any overflow_scale.
     // POW10_TABLE[overflow_scale] = 10^overflow_scale as u64
     // Exact precomputed values:
     // overflow_scale:   0         1         2          3           4             5              6
     // value:            1         10        100        1000        10000        100000        1000000
     //
     // overflow_scale:   7             8             9              10                 11
     // value:            10000000       100000000      1000000000     10000000000      100000000000
     //
     // overflow_scale:  12                13                 14                  15
     // value:           1000000000000      10000000000000      100000000000000      1000000000000000
     //
     // overflow_scale:  16                   17                    18
     // value:           10000000000000000     100000000000000000    1000000000000000000
     //
     // All values fit in u64: max is 10^18 = 1000000000000000000 < u64::MAX
     pow10: u64 = POW10_TABLE[overflow_scale]

     // Use u128 intermediate arithmetic for both range check and final multiply.
     // pow10 as u128 is safe: u64→u128 is zero-extension, always positive and in range.

     abs_unscaled: u128  // Declare before use; both branches assign
     max_allowed: u128

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
       // Use the correct limit: i64::MAX for positive, |i64::MIN| for negative
       limit = if unscaled >= 0 { i64::MAX as u64 } else { 1u64 << 63 };
       return Error(OutOfRange { attempted_magnitude: "<magnitude>", max_magnitude: limit, overflow_scale })

     // Use i128 intermediate to avoid pow10→i64 cast overflow.
     // The range check above guarantees the result fits in i64.
     // pow10 as i128 is safe: u64→i128 zero-extends to positive value ≤ 10^18 < u64::MAX,
     // which is always representable in i128 (i128 holds up to ~1.7×10^38).
     scaled_value = ((unscaled as i128) * (pow10 as i128)) as i64

4. CONSTRUCT_DQA
   // Apply CANONICALIZE per RFC-0105 §Canonical Representation
   // This ensures trailing decimal zeros are stripped from the mantissa
   // while decrementing scale. Stripping follows this evaluation order at each step:
   // (a) If scale == 0, stop — scale=0 means no decimal places exist;
   // (b) If mantissa % 10 != 0, stop — no trailing decimal zero to strip;
   // (c) Else strip one trailing zero and decrement scale, then repeat.
   // Note: CANONICALIZE never produces negative scale.
   // ⚠ CANONICALIZE halts at scale=0: once scale reaches 0, no further stripping
   // occurs regardless of whether the mantissa contains digit zeros. This is because
   // scale=0 means no decimal places exist — trailing zeros in the integer (e.g., 10, 100)
   // are not decimal trailing zeros. For example, {100, 0} is canonical even though 100
   // ends in two zeros, because scale=0 means these are integer digits, not decimal places.
   dqa = Dqa { value: scaled_value, scale: overflow_scale }
   Return CANONICALIZE(dqa)
```

### Edge Cases

| BigInt Input | Scale | DQA Output | Notes |
|-------------|-------|------------|-------|
| 0 | any | Dqa { 0, 0 } | CANONICALIZE produces canonical zero |
| i64::MAX | 0 | Dqa { i64::MAX, 0 } | Maximum representable |
| i64::MIN | 0 | Dqa { i64::MIN, 0 } | Minimum representable |
| 42 | 2 | Dqa { 42, 0 } | CANONICALIZE strips trailing zeros (4200 → 42) |
| 42 | 18 | Error(OutOfRange) | 42 × 10^18 > i64::MAX |
| -42 | 3 | Dqa { -42, 0 } | CANONICALIZE strips trailing zeros (-42000 → -42) |
| BigInt with 3+ limbs | any | Error(OutOfRange) | Exceeds i64 |

### Error Handling

| Error | Condition | RFC Reference |
|-------|-----------|--------------|
| `BigIntToDqaError::OutOfRange` | Value exceeds i64 range (before or after scaling) | This RFC |
| `BigIntToDqaError::InvalidScale` | Scale > 18 | This RFC |

### Scale Context Propagation

The scale parameter in BIGINT→DQA conversion has specific semantics:

| Scale Context | Behavior |
|--------------|----------|
| Explicit scale provided | Value is multiplied by 10^scale, then CANONICALIZE strips trailing zeros |
| Default scale (0) | Integer representation, no decimal places |
| Scale 2 with BigInt(1999) | After CANONICALIZE: DQA{1999, 0} (not $19.99 — see note) |
| Scale 18 with BigInt(1) | After CANONICALIZE: DQA{1, 0} |

**Scale adjustment:** The BigInt value is multiplied by 10^scale to produce the DQA mantissa, then CANONICALIZE strips trailing decimal zeros. For example:
- `BigInt(1999)` with scale 2 → 1999 × 10^2 = 199900 → CANONICALIZE → `{1999, 0}`
- `BigInt(1)` with scale 18 → 1 × 10^18 = 1000000000000000000 → CANONICALIZE → `{1, 0}`

**⚠ SQL Use-Case Note:** The CANONICALIZE step means the output scale is often reduced to 0, destroying the caller's intended decimal precision. For SQL column assignment, callers MUST re-apply the target scale using operations defined in RFC-0105 (e.g., multiply by 10^target_scale). Example:
```sql
-- After RFC-0131 conversion, result is {1999, 0}, not {199900, 2}
-- To store as DQA(2) column, caller must multiply by 10^2:
SELECT CAST(bigint_col AS DQA(0)) * CAST(100 AS DQA(0)) FROM currency;
```

This is different from DECIMAL where scale is metadata about precision. Here, scale is part of the DQA type definition per RFC-0105 and affects the mantissa value directly.

## Round-Trip Asymmetry

This conversion is NOT the inverse of RFC-0132's DQA→BIGINT:

| Direction | Conversion | Result |
|-----------|------------|--------|
| Forward (RFC-0131) | `BigInt(1999), scale=2` → DQA | DQA{1999, 0} |
| Reverse (RFC-0132) | `DQA{1999, 0}` → BIGINT | BigInt(1999) |

Round-trip: `BigInt(1999), scale=2` → DQA{1999, 0} → BigInt(1999) — numerically lossless (same value), but **scale information is lost**.

**Note on CANONICALIZE:** After scale multiplication, CANONICALIZE strips trailing decimal zeros, often reducing scale to 0. For `BigInt(1999), scale=2`: 1999 × 10^2 = 199900 → canonicalizes to {1999, 0}. The round-trip recovers the value (1999) but not the scale (2).

### Lossless Round-Trip Cases

Round-trip is **lossless** when scale=0 or when the scaled mantissa has no trailing zeros:

| Condition | Example | Round-trip |
|----------|---------|------------|
| scale=0 | `BigInt(42), scale=0` → {42, 0} → BigInt(42) | ✓ Lossless |
| Scale > 0 | `BigInt(19), scale=2` → {19, 0} → BigInt(19) | ✓ Value recovered (scale lost) |

**For SQL currency use-cases:** Re-apply the target scale using RFC-0105 arithmetic operations after conversion.

## BigIntWithScale to DQA Conversion

The `BigIntWithScale` type from RFC-0132 preserves the numeric VALUE but NOT the scale through the round-trip. The scale may be reduced by CANONICALIZE in Step 4. To convert back to DQA:

```rust
/// Convert BigIntWithScale back to DQA.
///
/// This complements `dqa_to_bigint_with_scale` from RFC-0132, reversing the value extraction.
/// Mantissa-preserving: DQA → BigIntWithScale → DQA recovers the numeric mantissa.
/// The output DQA may have a different scale than the input due to CANONICALIZE stripping
/// trailing decimal zeros. For example, DQA{1999, 2} → BigIntWithScale{1999, 2} → DQA{1999, 0}.
/// ⚠ The scale may be reduced by CANONICALIZE — the output scale may differ from input.
///
/// # Input Assumptions
/// This function assumes `BigIntWithScale` was obtained from `dqa_to_bigint_with_scale` in
/// RFC-0132, which guarantees canonical DQA inputs. Direct construction of `BigIntWithScale`
/// with non-canonical values (e.g., `BigIntWithScale { value: BigInt(100), scale: 2 }`)
/// will produce unexpected results without error. Callers MUST ensure input conforms to
/// the canonical DQA invariants that `dqa_to_bigint_with_scale` enforces.
///
/// # Arguments
/// * `bws` - The BigIntWithScale containing value and original scale
///
/// # Returns
/// Ok(Dqa) on success, Err(BigIntToDqaError) on overflow
///
/// # Example
/// BigIntWithScale { value: BigInt(1999), scale: 2 } → Dqa { value: 1999, scale: 0 }
/// Note: CANONICALIZE strips trailing zeros, reducing scale from 2 to 0.
pub fn bigint_with_scale_to_dqa(bws: &BigIntWithScale) -> Result<Dqa, BigIntToDqaError> {
    // Note: BigIntWithScale.scale represents the original DQA scale, which is
    // semantically equivalent to overflow_scale for bounds checking purposes.
    // The original scale value is used directly as the overflow threshold.
    bigint_to_dqa(&bws.value, bws.scale)
}
```

**Note:** The value is recovered but the scale may be reduced by CANONICALIZE. To recover the original DQA scale, multiply the result by `10^(original_scale - canonical_scale)` using RFC-0105 arithmetic.

## Composition Semantics

Chaining BIGINT→DQA with DQA→BIGINT does NOT recover the original scale context:

```sql
-- Step 1: BIGINT → DQA (RFC-0131, scale applied then canonicalized)
SELECT bigint_to_dqa(bigint_col, 2) FROM accounts;
-- BigInt(1999), scale=2 → DQA{1999, 0}

-- Step 2: DQA → BIGINT (RFC-0132, scale ignored)
SELECT CAST(dqa_col AS BIGINT) FROM accounts;
-- DQA{1999, 0} → BigInt(1999)
```

**⚠ WARNING:** The composition `bigint_to_dqa(CAST(dqa_col AS BIGINT), 2)` produces `DQA{1999, 0}`, not the original DQA. This is a 100× magnitude error in financial calculations.

### Negative Round-Trip
```
Input:  BigInt(-42), overflow_scale = 0 → DQA → BigInt
Output: Dqa { value: -42, scale: 0 } → BigInt(-42) ✓
Note: BigInt(-42) × 10^0 = -42, mantissa preserved.
```

### SQL Integration

BIGINT→DQA conversion appears in SQL CAST expressions:

```sql
-- Explicit CAST from BIGINT to DQA with scale
-- Internally maps to: bigint_to_dqa(bigint_col, 6)
SELECT CAST(bigint_col AS DQA(6)) FROM account_balances;

-- This is VALID: BigInt value must fit in i64 range
-- If bigint_col = 9223372036854775807 (i64::MAX), conversion succeeds
-- If bigint_col = 9223372036854775808 (i64::MAX + 1), error
```

**Mapping:** `CAST(x AS DQA(n))` internally calls `bigint_to_dqa(x, n)` where `n` is the `overflow_scale` parameter.

```sql
-- Scale 2 for currency representation
SELECT CAST(bigint_col AS DQA(2)) FROM currency_amounts;
-- BigInt(1999) with scale 2 → DQA{1999, 0} (after CANONICALIZE)
-- ⚠ Note: Scale 2 intent ($19.99) is lost. To restore scale,
-- multiply by 10^2 using RFC-0105 arithmetic:
SELECT CAST(bigint_col AS DQA(0)) * CAST(100 AS DQA(0)) FROM currency_amounts;

-- FORBIDDEN: Explicit CAST from oversized BigInt
SELECT CAST(huge_bigint_col AS DQA(0)) FROM large_values;
-- Error: BigIntToDqaError::OutOfRange
```

#### Cast Semantics in Deterministic Context

| Source Type | Target Type | Behavior | Notes |
|-------------|-------------|----------|-------|
| BIGINT | DQA(n) | Error if value out of i64 range before or after scale | Overflow → TRAP |
| BIGINT | DQA(0) | Error if \|value\| > i64::MAX | No scale multiplication |
| BIGINT | DQA(18) | Error if \|value × 10^18\| > i64::MAX | Scale multiplication can overflow |

**Note:** Unlike DFP→DQA lowering (RFC-0124), BIGINT→DQA does not require rounding because BigInt is already an integer type. The only loss possible is range truncation (overflow).

### Constraints

| Constraint Type | Description |
|----------------|-------------|
| **Scale bounds** | 0 ≤ scale ≤ 18 (per RFC-0105 MAX_SCALE) |
| **Pre-scale range** | \|BigInt\| ≤ i64::MAX — checked in Step 1 before scaling |
| **Post-scale range** | \|BigInt × 10^scale\| ≤ i64::MAX (positive) or ≤ \|i64::MIN\| (negative) — checked in Step 3 |
| **Overflow policy** | Error on overflow (no truncation, no saturation) |
| **BigInt size** | 1-2 limbs. Step 0 rejects non-canonical hi==0 two-limb; Step 1 rejects 3+ limb and returns OutOfRange for canonical two-limb (overflow) |
| **Determinism** | Identical BigInt input always produces identical DQA output |
| **No rounding** | BIGINT→DQA does not round; it traps on overflow |
| **Canonical input** | Algorithm assumes canonical BigInt per RFC-0110. Non-canonical inputs (e.g., two-limb with hi=0, or negative-zero) are undefined behavior. Implementations MUST TRAP on non-canonical input per RFC-0110 §Input Canonicalization Requirement. |

## Relationship to Other RFCs

| RFC | Relationship | Precedence |
|-----|-------------|------------|
| RFC-0110 (BIGINT) | Input type | BIGINT operations apply before conversion. Note: RFC-0110's existing `bigint_to_dqa(i128)` function remains unchanged. |
| RFC-0105 (DQA) | Output type | DQA semantics apply after conversion |

**Precedence Rule:** This RFC does not override RFC-0105 or RFC-0110. All outputs satisfy RFC-0105's canonical form requirements. All inputs must satisfy RFC-0110's canonical form requirements.

**Cross-RFC Type Dependency:** `BigIntWithScale` is defined in RFC-0132 §Input/Output Contract. If RFC-0132 changes the `BigIntWithScale` definition (e.g., adds a field, changes semantics), RFC-0131 §Value-Preserving Conversion and §Composition Semantics must be reviewed and updated accordingly.

## Test Vectors

### V001: Zero Conversion
```
Input:  BigInt::zero(), overflow_scale = 0
Output: Dqa { value: 0, scale: 0 }
```

### V002: Small Positive Integer
```
Input:  BigInt::from(42i64), overflow_scale = 0
Output: Dqa { value: 42, scale: 0 }
```

### V003: Small Negative Integer
```
Input:  BigInt::from(-42i64), overflow_scale = 0
Output: Dqa { value: -42, scale: 0 }
```

### V004: Positive with Scale
```
Input:  BigInt::from(42i64), overflow_scale = 3
Output: Dqa { value: 42, scale: 0 }
Note: 42 × 10^3 = 42000. CANONICALIZE strips three trailing zeros: 42000 → 42, scale: 3 → 0.
```

### V005: i64::MAX
```
Input:  BigInt::from(i64::MAX), overflow_scale = 0
Output: Dqa { value: 9223372036854775807, scale: 0 }
```

### V006: i64::MIN
```
Input:  BigInt::from(i64::MIN), overflow_scale = 0
Output: Dqa { value: -9223372036854775808, scale: 0 }
```

### V007: Overflow — Too Large
```
Input:  BigInt { limbs: [0, 0, 1], sign: false }, overflow_scale = 0
Output: Error(OutOfRange)
Note: Requires 3 limbs (192 bits) > i64 range
```

### V008: Non-Canonical Two-Limb — hi=0 Overflow
```
Input:  BigInt { limbs: [0x0000000000000001, 0x0000000000000000], sign: false }, overflow_scale = 0
Note: Non-canonical form of value 1. RFC-0110 requires canonical single-limb for
values < 2^63. Non-canonical inputs are undefined behavior — implementations MUST TRAP.
```

### V009: Overflow — Negative 2^64 Magnitude
```
Input:  BigInt { limbs: [0, 1], sign: true }, overflow_scale = 0
Output: Error(OutOfRange)
Note: Magnitude 2^64 exceeds |i64::MIN| = 2^63 for negative values
```

### V010: Scale Adjustment for Currency
```
Input:  BigInt::from(1999i64), overflow_scale = 2
Output: Dqa { value: 1999, scale: 0 }
Note: 1999 × 10^2 = 199900. CANONICALIZE strips two trailing zeros: 199900 → 1999, scale: 2 → 0.
⚠ For SQL currency, caller must re-apply target scale using RFC-0105 arithmetic.
```

### V011: Maximum Scale (18)
```
Input:  BigInt::from(1i64), overflow_scale = 18
Output: Dqa { value: 1, scale: 0 }
Note: 1 × 10^18 = 1000000000000000000. CANONICALIZE strips 18 trailing zeros: 1000000000000000000 → 1, scale: 18 → 0.
```

### V012: Negative with Scale
```
Input:  BigInt::from(-100i64), overflow_scale = 4
Output: Dqa { value: -100, scale: 0 }
Note: -100 * 10^4 = -1000000. CANONICALIZE runs 4 iterations (bounded by scale=4), producing {-100, 0}.
Output mantissa -100 contains trailing zeros but CANONICALIZE halts at scale=0 — these are integer digits, not decimal trailing zeros.
```

### V013: i64 Boundary — One Less Than MAX
```
Input:  BigInt::from(9223372036854775806i64), overflow_scale = 0
Output: Dqa { value: 9223372036854775806, scale: 0 }
Note: i64::MAX - 1, still fits
```

### V014: i64 Boundary — One More Than MIN
```
Input:  BigInt::from(-9223372036854775807i64), overflow_scale = 0
Output: Dqa { value: -9223372036854775807, scale: 0 }
Note: i64::MIN + 1, still fits
```

### V015: Scale 1 Edge Case
```
Input:  BigInt::from(10i64), overflow_scale = 1
Output: Dqa { value: 10, scale: 0 }
Note: 10 * 10^1 = 100. CANONICALIZE strips trailing zero: 100 → 10, scale: 1 → 0.
CANONICALIZE halts at scale=0; the trailing zero in 10 is not a decimal trailing zero
since scale=0 means no decimal places.
```

### V016: Overflow — 128-bit Value (2 limbs, exceeds i64)
```
Input:  BigInt { limbs: [0x0000000000000000, 0x0000000000000001], sign: false }, overflow_scale = 0
Output: Error(OutOfRange)
Note: 2^64 = 18446744073709551616 > i64::MAX
Note: limbs[0]=0 (lo), limbs[1]=1 (hi) per RFC-0110 little-endian
```

### V017: Overflow — 2^63 Exactly
```
Input:  BigInt { limbs: [0x0000000000000000, 0x8000000000000000], sign: false }, overflow_scale = 0
Output: Error(OutOfRange)
Note: 2^63 = 9223372036854775808. This magnitude equals |i64::MIN| but as a
positive value it exceeds i64::MAX (9223372036854775807), causing overflow.
Note: limbs[0]=0 (lo), limbs[1]=0x8000... (hi) per RFC-0110 little-endian
```

### V018: Negative Overflow — Magnitude Exceeds MAX
```
Input:  BigInt { limbs: [0x0000000000000001, 0x0000000000000001], sign: true }, overflow_scale = 0
Output: Error(OutOfRange)
Note: (2^64 + 1) = 18446744073709551617 > i64::MAX
```

### V019: Single Limb Positive (Within i64 Range)
```
Input:  BigInt { limbs: [0x123456789ABCDEF0], sign: false }, overflow_scale = 0
Output: Dqa { value: 0x123456789ABCDEF0, scale: 0 }
Note: Value 1311768467294899440 < i64::MAX, fits in i64
```

### V020: Single Limb Negative
```
Input:  BigInt { limbs: [0x123456789ABCDEF0], sign: true }, overflow_scale = 0
Output: Dqa { value: -0x123456789ABCDEF0, scale: 0 }
Note: Fits in i64 range
```

### V021: Invalid Scale — Exceeds 18
```
Input:  BigInt::from(42i64), overflow_scale = 19
Output: Error(InvalidScale)
Note: DQA max scale is 18
```

### V022: Zero with Non-Zero Scale
```
Input:  BigInt::zero(), overflow_scale = 6
Output: Dqa { value: 0, scale: 0 }
Note: 0 × 10^6 = 0, which is within i64 range, so conversion succeeds.
CANONICALIZE produces canonical zero with scale=0 per RFC-0105.
Zero always canonicalizes to Dqa { 0, 0 } regardless of input scale.
```

### V035: Single Limb Positive — Overflow at 2^63
```
Input:  BigInt { limbs: [0x8000000000000001], sign: false }, overflow_scale = 0
Output: Error(OutOfRange)
Note: 2^63 + 1 = 9223372036854775809 > i64::MAX
This is the single-limb case: high bit set means magnitude > i64::MAX
```

### V036: Single Limb Positive Max — i64::MAX
```
Input:  BigInt { limbs: [0x7FFF_FFFF_FFFF_FFFF], sign: false }, overflow_scale = 0
Output: Dqa { value: 9223372036854775807, scale: 0 }
Note: i64::MAX exactly — canonical single-limb form
```

### V037: Single Limb Positive Overflow — i64::MAX + 1
```
Input:  BigInt { limbs: [0x8000_0000_0000_0000], sign: false }, overflow_scale = 0
Output: Error(OutOfRange)
Note: 2^63 = 9223372036854775808 = i64::MAX + 1, overflow
```

### V038: Single Limb Negative — i64::MIN Magnitude
```
Input:  BigInt { limbs: [0x8000_0000_0000_0000], sign: true }, overflow_scale = 0
Output: Dqa { value: -9223372036854775808, scale: 0 }
Note: i64::MIN exactly — valid negative value
```

### V039: Single Limb Negative Overflow — i64::MIN - 1
```
Input:  BigInt { limbs: [0x8000_0000_0000_0001], sign: true }, overflow_scale = 0
Output: Error(OutOfRange)
Note: Magnitude = 2^63 + 1 > 2^63 = |i64::MIN|, overflow
```

### V023: Large Currency Value
```
Input:  BigInt::from(1000000i64), overflow_scale = 2
Output: Dqa { value: 1000000, scale: 0 }
Note: 1000000 * 10^2 = 100000000. CANONICALIZE strips 2 scale units (bounded by scale=2), resulting in scale 0. The output mantissa 1000000 still contains trailing decimal zeros, but CANONICALIZE halts at scale=0.
```

### V024: i64::MIN Exactly
```
Input:  BigInt::from(i64::MIN), overflow_scale = 0
Output: Dqa { value: -9223372036854775808, scale: 0 }
Note: Special case -i64::MIN = i64::MIN in unsigned magnitude
```

### V025: Scale Multiplication Overflow — i64::MAX × 10^18
```
Input:  BigInt::from(9223372036854775807i64), overflow_scale = 18
Output: Error(OutOfRange)
Note: i64::MAX × 10^18 = 9.22... × 10^36 > i64::MAX (9.22... × 10^18)
```

### V026: Scale Boundary — 0 (Min)
```
Input:  BigInt::from(-9223372036854775808i64), overflow_scale = 0
Output: Dqa { value: -9223372036854775808, scale: 0 }
Note: Minimum value with minimum scale
```

### V027: Overflow — Exceeds i64::MAX by 1
```
Input:  BigInt { limbs: [1, 0x8000000000000000], sign: false }, overflow_scale = 0
Output: Error(OutOfRange)
Note: Magnitude = 2^127 + 1 > i64::MAX. Rejected by positive two-limb check: hi > 0.
```

### V028: Negative Overflow — Exceeds i64::MIN by 1
```
Input:  BigInt { limbs: [1, 0x8000000000000000], sign: true }, overflow_scale = 0
Output: Error(OutOfRange)
Note: |value| = 2^127 + 1 > |i64::MIN|. All 2-limb negatives are unconditionally rejected.
```

### V029: Scale Multiplication Overflow — 93 × 10^17
```
Input:  BigInt::from(93i64), overflow_scale = 17
Output: Error(OutOfRange)
Note: 93 × 10^17 = 9.3 × 10^18 > i64::MAX (9.2 × 10^18)
```

### V030: Scale Multiplication Overflow — 10 × 10^18
```
Input:  BigInt::from(10i64), overflow_scale = 18
Output: Error(OutOfRange)
Note: 10 × 10^18 = 10^19 > i64::MAX (9.2 × 10^18)
```

### V031: Scale Multiplication Overflow — Negative
```
Input:  BigInt::from(-93i64), overflow_scale = 17
Output: Error(OutOfRange)
Note: |-93| × 10^17 = 9.3 × 10^18 > 2^63 = |i64::MIN|
```

### V032: Scale Multiplication Edge — 9 × 10^18 (Fits)
```
Input:  BigInt::from(9i64), overflow_scale = 18
Output: Dqa { value: 9, scale: 0 }
Note: 9 × 10^18 = 9_000_000_000_000_000_000. CANONICALIZE strips 18 trailing zeros: 9_000_000_000_000_000_000 → 9, scale: 18 → 0.
```

### V033: Scale Multiplication Edge — 92 × 10^17 (Fits)
```
Input:  BigInt::from(92i64), overflow_scale = 17
Output: Dqa { value: 92, scale: 0 }
Note: 92 × 10^17 = 9.2 × 10^18 = 9200000000000000000. CANONICALIZE strips
17 trailing zeros: 9200000000000000000 → 92, scale: 17 → 0.
```

### V034: Negative with Scale > 0 — Success
```
Input:  BigInt::from(-1i64), overflow_scale = 1
Output: Dqa { value: -1, scale: 0 }
Note: |-1| × 10^1 = 10. CANONICALIZE strips trailing zero: 10 → 1, scale: 1 → 0.
```

### V401: BigIntWithScale Extraction — Positive
```
Input:  BigIntWithScale { value: BigInt::from(1999i64), scale: 2 }
Output: Ok(Dqa { value: 1999, scale: 0 })
Note: 1999 × 10^2 = 199900. CANONICALIZE strips two trailing zeros: 199900 → 1999, scale: 2 → 0.
Round-trip: DQA{1999, 2} → BigIntWithScale{1999, 2} → DQA{1999, 0}. Scale reduced by CANONICALIZE.
```

### V402: BigIntWithScale Extraction — Negative
```
Input:  BigIntWithScale { value: BigInt::from(-42i64), scale: 3 }
Output: Ok(Dqa { value: -42, scale: 0 })
Note: -42 × 10^3 = -42000. CANONICALIZE strips three trailing zeros: -42000 → -42, scale: 3 → 0.
Round-trip: DQA{-42, 3} → BigIntWithScale{-42, 3} → DQA{-42, 0}. Scale reduced by CANONICALIZE.
```

### V403: BigIntWithScale Extraction — Zero
```
Input:  BigIntWithScale { value: BigInt::zero(), scale: 0 }
Output: Ok(Dqa { value: 0, scale: 0 })
Note: Zero always canonicalizes to Dqa { 0, 0 } regardless of input scale per RFC-0105.
Round-trip: DQA{0, 0} → BigIntWithScale{0, 0} → DQA{0, 0}. Zero canonicalizes to scale 0.
```

### V404: Two-Limb Negative Overflow
```
Input:  BigInt { limbs: [1, 1], sign: true }, overflow_scale = 0
Output: Error(OutOfRange)
Note: Magnitude = 2^64 + 1 > |i64::MIN| = 2^63. All 2-limb negatives overflow.
```

### V405: BigIntWithScale Round-Trip — Overflow
```
Input:  BigIntWithScale { value: BigInt::from(93i64), scale: 17 }
Output: Error(OutOfRange)
Note: 93 × 10^17 = 9.3 × 10^18 > i64::MAX (9.2 × 10^18)
```

### V406: i64::MAX with Scale 1 — Overflow
```
Input:  BigInt::from(9223372036854775807i64), overflow_scale = 1
Output: Error(OutOfRange)
Note: 9223372036854775807 × 10 = 92233720368547758070 > i64::MAX
```

### V407: -1 with Scale 18 — Success
```
Input:  BigInt::from(-1i64), overflow_scale = 18
Output: Dqa { value: -1, scale: 0 }
Note: |-1| × 10^18 = 10^18 < |i64::MIN| = 2^63. CANONICALIZE strips 18 trailing zeros.
```

### V408: 9 with Scale 17 — Success
```
Input:  BigInt::from(9i64), overflow_scale = 17
Output: Dqa { value: 9, scale: 0 }
Note: 9 × 10^17 = 9×10^17 = 900000000000000000 < i64::MAX (9.2×10^18). Fits.
```

### V409: 10 with Scale 17 — Success
```
Input:  BigInt::from(10i64), overflow_scale = 17
Output: Dqa { value: 10, scale: 0 }
Note: 10 × 10^17 = 10^18 = 1000000000000000000 < i64::MAX (9.2×10^18). Fits.
```

### V410: 100 with Scale 17 — Overflow
```
Input:  BigInt::from(100i64), overflow_scale = 17
Output: Error(OutOfRange)
Note: 100 × 10^17 = 10^19 = 10000000000000000000 > i64::MAX. Overflow.
```

### V411: BigIntWithScale — Oversized Value (Direct Construction Error Path)
```
Input:  BigIntWithScale { value: BigInt { limbs: [0, 1], sign: false }, scale: 0 }
Output: Error(OutOfRange)
Note: Directly constructed BigIntWithScale with value exceeding i64 range. This error path
exercises the overflow check in bigint_with_scale_to_dqa when bws.value itself is too large
(even with scale=0). Such a BigIntWithScale cannot be produced by dqa_to_bigint_with_scale
since DQA values always fit in i64, but directly constructed inputs are still validated.
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
    pub fn to_dqa(&self, overflow_scale: u8) -> Result<Dqa, BigIntToDqaError> {
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

**Gas for error paths:** Gas is charged regardless of whether the conversion succeeds or returns Err. The fixed gas allocation of 12 applies to both success and error paths.

## Error Handling and Diagnostics

### Compile-Time Errors

When BIGINT→DQA conversion fails at compile time (e.g., explicit CAST), the compiler emits:

```
ERROR: Cannot convert BIGINT to DQA
  Expression: CAST(bigint_col AS DQA(0)) at line 42
  Reason: BigIntToDqaError::OutOfRange — raw value 9223372036854775808 exceeds i64::MAX
  Hint: Use BIGINT type or reduce the value

ERROR: Cannot convert BIGINT to DQA
  Expression: CAST(value AS DQA(19)) at line 15
  Reason: BigIntToDqaError::InvalidScale — overflow_scale 19 exceeds maximum (18)
  Hint: Use overflow_scale 0-18 for DQA type
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
| T3 | Scale Upper Bound | Output scale ≤ input scale | Required |
| T4 | Overflow Completeness | No false negatives: overflow always detected | Required |
| T5 | Scale Bounds | Scale validation is correct | Required |

### Theorem Specifications

**Theorem T1 (Determinism):** For identical BigInt input and scale, the conversion always produces identical DQA output or identical error.

**Theorem T2 (Range Preservation):** If `bigint_to_dqa(b, s) = Ok(dqa)`, then `|dqa.value| ≤ i64::MAX`.

**Theorem T3 (Scale Upper Bound):** If `bigint_to_dqa(b, s) = Ok(dqa)`, then `dqa.scale ≤ s`. CANONICALIZE may reduce scale by stripping trailing decimal zeros.

**Theorem T4 (Overflow Completeness):** For canonical BigInt inputs `b` with `0 ≤ s ≤ 18`: `bigint_to_dqa(b, s) = Err(OutOfRange) ⟺ (b × 10^s < i64::MIN OR b × 10^s > i64::MAX)`.

Note: Non-canonical inputs are outside this theorem's domain — they TRAP per Step 0, not return Err.

**Corollary T4a:** For any canonical BigInt with `b.limbs.length > 2`, `|b| ≥ 2^128 > i64::MAX`. The algorithm detects this in Step 1 (limb count check) before any scaled multiplication.

**Theorem T5 (Scale Bounds):** If `s > 18`, then `bigint_to_dqa(b, s) = Err(InvalidScale)`.

## Implementation Checklist

| Mission | Description | Status | Complexity |
|---------|-------------|--------|------------|
| M1 | `bigint_to_dqa` core algorithm | Pending | Medium |
| M2 | Scale validation (0-18 bounds) | Pending | Low |
| M3 | Limb inspection and range check | Pending | Medium |
| M4 | i64::MIN special case handling | Pending | Low |
| M5 | Error type construction | Pending | Low |
| M6 | Test vector suite (48 normative + 1 informational V008: V001-V034, V035-V039, V401-V410) | Pending | Medium |
| M7 | Integration with BigInt type | Pending | Medium |
| M8 | Fuzz testing for edge cases | Pending | Medium |
| M9 | `bigint_with_scale_to_dqa` with BigIntWithScale | Pending | Low |

## Future Work

- F1: DQA→BIGINT conversion (see RFC-0132)
- F2: BIGINT→DECIMAL conversion (see RFC-0133)
- F3: DECIMAL→BIGINT conversion (see RFC-0134)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.27 | 2026-03-24 | (Current) HIGH: Made CANONICALIZE stopping condition evaluation order explicit in Step 4 (R8H1). HIGH: Added ## Test Vectors section heading before V001 (R8H3). HIGH: Added V411 test vector for bigint_with_scale_to_dqa error path (R8H2). MEDIUM: Added TRAP to Step 0 b.limbs.length > 64 check (R8M1). MEDIUM: V012 note added scale=0 trailing-zero clarification (R8M2). MEDIUM: V407 note corrected bound to |i64::MIN| for negative (R8M3). MEDIUM: M6 checklist count corrected to 48 normative + 1 informational (R8M4). LOW: Merged Lossless Round-Trip table rows 2 and 3 (R8L1). LOW: Promoted bigint_with_scale_to_dqa to its own section (R8L2). LOW: V401-V403 headings changed to BigIntWithScale Extraction (R8X1). |
| 1.26 | 2026-03-24 | CRITICAL: CANONICALIZE stopping rule clarified — scale=0 means no decimal places, integer trailing zeros not stripped (R7C1). CRITICAL: V023 output {1000000,0} canonical per RFC-0105 explained (R7C2). HIGH: Two-limb hi>0 redundancy documented after Step 0 (R7H1). HIGH: bigint_with_scale_to_dqa input assumptions documented (R7H2). HIGH: Constraints table corrected to show 2-limb canonical handled in Step 1 (R7H3). MEDIUM: T4 biconditional domain clarified — canonical inputs only (R7M1). MEDIUM: V015 CANONICALIZE note clarified at scale=0 (R7M2). MEDIUM: V409 Ok() notation removed for consistency (R7M4). LOW: V033 note added trailing-zero count (R7L1). LOW: Step 0 >64 check vs Step 1 >2 check relationship documented (R7L2). Cross-RFC: Added BigIntWithScale change-propagation note (R7X2). |
| 1.25 | 2026-03-24 | CRITICAL: Added b.limbs.length > 64 check in Step 0 (R11-131-C1). HIGH: V403 input changed to scale: 0 (R11-131-H1). HIGH: Moved V021/V022 before V023 to restore sequential order (R11-131-H2). MEDIUM: T4 biconditional: Err ⟺ (canonical AND overflow) (R11-131-M1). MEDIUM: Changed to_dqa stub param from scale to overflow_scale (R11-131-M2). LOW: V012 note clarified CANONICALIZE 4 iterations bounded by scale=4 (R11-131-L2). Cross-RFC: Added round-trip conformance note (R11-X1). |
| 1.24 | 2026-03-24 | CRITICAL: Added empty limb slice check in Step 0 (R10-131-C1). CRITICAL: Added RFC-0132 to Depends On for BigIntWithScale type (R10-131-C2). HIGH: Fixed missing code fence in SQL Integration section (R10-131-H1). HIGH: Fixed V023 note — CANONICALIZE strips 2 scale units, not 8 decimal zeros (R10-131-H2). MEDIUM: Changed "value-preserving" to "mantissa-preserving" in bigint_with_scale_to_dqa (R10-131-M1). MEDIUM: Fixed M6 checklist description to include V035-V039 (R10-131-M4). |
| 1.23 | 2026-03-24 | CRITICAL: Renamed `scale` parameter to `overflow_scale` in `bigint_to_dqa` function to avoid confusion with PostgreSQL/rust_decimal semantics. Updated all 49 test vectors to use `overflow_scale` parameter. MEDIUM: Added CAST(x AS DQA(n)) ↔ bigint_to_dqa(x, n) mapping documentation. MEDIUM: Added explanatory comment for BigIntWithScale.scale → overflow_scale. LOW: Updated error messages to clarify "raw value" vs "overflow_scale" overflow. LOW: Added note that RFC-0110's existing bigint_to_dqa(i128) is unchanged. |
| 1.22 | 2026-03-24 | HIGH: Clarified BigIntWithScale round-trip preserves VALUE not SCALE (R17-131-H1). MEDIUM: Added test vectors V406-V410 (i64::MAX scale=1 overflow, -1 scale=18, boundary cases) (R17-131-M1). MEDIUM: Updated Implementation Checklist to 49 vectors (R17-131-L1). LOW: Fixed V409 — 10 × 10^17 fits in i64, not overflow (R18-131-L1). |
| 1.21 | 2026-03-24 | CRITICAL: Fixed POW10_TABLE scale 18 entry from 10^19 to 10^18 (R16-131-M2). CRITICAL: Strengthened i64::MIN Step 1/Step 2 dependency documentation with CRITICAL warning (R16-131-C2). CRITICAL: Renamed BigIntError to BigIntToDqaError for consistency with DqaToBigIntError (R16-XC3). HIGH: Documented two-limb negative unconditional rejection reasoning (R16-131-C3). HIGH: Clarified Step 0 hi==0 check covers both signs (R16-131-H3). MEDIUM: Changed "inverse" to "complement" for bigint_with_scale_to_dqa (R16-131-M1). MEDIUM: Added gas note for error paths (R16-131-M3). MEDIUM: Added V404 (two-limb negative overflow) and V405 (BigIntWithScale overflow) (R16-131-M4). LOW: Clarified V022 note about 0 × 10^6 = 0 (R16-131-L3). |
| 1.20 | 2026-03-24 | MEDIUM: Added test vectors V401-V403 for BigIntWithScale round-trip (R15-131-M1). LOW: Updated Implementation Checklist with M9 (bigint_with_scale_to_dqa) and corrected test vector count (42 vectors) (R15-131-L1). |
| 1.19 | 2026-03-24 | FIXED: Added bigint_with_scale_to_dqa function specification to complete round-trip safe variant (R14-131-F1). |
| 1.18 | 2026-03-24 | MEDIUM: Added Composition Semantics section documenting chained conversion behavior and 100× magnitude warning (R13-131-M1). |
| 1.17 | 2026-03-24 | MEDIUM: Improved Step 1/Step 2 maintainability — added notes explaining why Step 1 uses ">" (not ">=") for i64::MIN boundary and why Step 2's special case is safe (R12-131-M1). |
| 1.16 | 2026-03-24 | LOW: Fixed V023 note — "strips two trailing zeros" corrected to "strips eight trailing zeros" (math was wrong: 100000000 has 8 trailing zeros, not 2). LOW: Removed duplicate v1.9 version history entry (copy-paste artifact). |
| 1.15 | 2026-03-24 | MEDIUM: Addressed round 6 adversarial review issues (R6-131-M1 through R6-131-M5). |
| 1.14 | 2026-03-24 | CRITICAL: Fixed `-(lo as i64)` panic for i64::MIN — added i64::MIN special case in Step 2 (R9-131-C1). CRITICAL: Fixed Lossless Round-Trip Cases table — now shows {19,0} post-canonicalization (R9-131-C2). HIGH: Fixed two-limb hi==0 gap — added Step 0 VERIFY_CANONICAL (R9-131-H1). HIGH: Fixed malformed pseudocode syntax (R9-131-H2). LOW: Fixed function summary — scale is overflow threshold, not output precision (R9-131-L1). MEDIUM: Fixed version history citation (R9-131-M3). |
| 1.13 | 2026-03-23 | (Internal version — changes incorporated into v1.14) |
| 1.12 | 2026-03-23 | (Internal version — changes incorporated into v1.14) |
| 1.11 | 2026-03-23 | (Internal version — changes incorporated into v1.14) |
| 1.10 | 2026-03-23 | (Internal version — changes incorporated into v1.14) |
| 1.9 | 2026-03-23 | HIGH: Added missing single-limb negative range check (R5H2). MEDIUM: Replaced POW10_TABLE informal labels with exact u64 values (R5M1), added T4 corollary for 3+ limb BigInts (R5M3). LOW: Removed BigIntToDqaInput dead struct (R5L1), fixed pow10→i128 comment bound (R5L2), removed V008 normative output (R5H1 — UB cannot have expected output). |
| 1.8 | 2026-03-23 | LOW: Fixed V027/V028 rejection criterion notes — "hi > 0" not "hi ≥ 2^63". |
| 1.7 | 2026-03-23 | LOW: Added lossless round-trip case documentation — scale=0 preserves value exactly (R3L4). |
| 1.6 | 2026-03-23 | CRITICAL: Fixed `pow10 as i64` overflow — Step 3 now uses i128 intermediate for multiplication (R3C1). HIGH: Fixed T4 theorem to use signed range (R3H1). MEDIUM: Fixed function doc error comment (R3M2), Constraints table (R3M1), V008/V009 limb arrays (R3M3). LOW: V020b→V035, checklist count 35 (R3L1/M4), removed dead BigIntToDqaOutput enum (R3L2). |
| 1.5 | 2026-03-23 | CRITICAL: Added two-limb range check (positive hi>0, negative hi>0x8000...). MEDIUM: V027/V028 added. |
| 1.4 | 2026-03-23 | Critical fixes: Added explicit limb convention per RFC-0110 (CRITICAL-C1), fixed single-limb range check hole (CRITICAL-C2), fixed unscanned typo (CRITICAL-C3), fixed negative×scale overflow (HIGH-H3), fixed max_magnitude type (HIGH-H4), fixed V016/V017 limb arrays (LOW-L1/L2), added V020b and V034 test vectors, updated gas model |
| 1.3 | 2026-03-23 | Critical fix: Added sign-aware boundary check for positive 2^63 overflow (CRITICAL-1), fixed V025 which incorrectly claimed success for i64::MAX×scale-18, removed duplicate range check between Steps 1 and 2, fixed V033 note arithmetic |
| 1.2 | 2026-03-23 | Critical fix: Added scale multiplication step to algorithm (was missing), added overflow check for scaled values, fixed V011 and Edge Cases zero handling to be consistent, fixed V017 note, added V029-V033 for scale overflow test vectors, added scale field to OutOfRange error |
| 1.1 | 2026-03-23 | Enhanced: Added Input/Output Contract, Scale Context Propagation, SQL Integration, Constraints, Error Handling & Diagnostics, Formal Verification Framework (5 theorems), Implementation Checklist, expanded test vectors from 10 to 28 |
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
