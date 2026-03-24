# RFC-0132 (Numeric/Math): DQA to BIGINT Conversion

## Status

**Version:** 1.19 (Draft)
**Status:** Draft
**Depends On:** RFC-0110 (BIGINT), RFC-0105 (DQA)
**Category:** Numeric/Math

## Summary

This RFC specifies the conversion algorithm from DQA (RFC-0105, i64 value with 0-18 decimal scale) to BIGINT (RFC-0110, arbitrary-precision integer up to 4096 bits). This conversion is necessary for the Numeric Tower to support operations that require DQA values to be used in BIGINT contexts, and for explicit CAST expressions between these types.

This conversion always succeeds for **canonical** DQA inputs (valid i64 value, scale 0-18, canonical form per RFC-0105). Non-canonical inputs MUST TRAP. The i64 value trivially fits within BIGINT's arbitrary range.

## Motivation

### Problem Statement

DQA provides fixed-precision decimal arithmetic with i64 value and 0-18 scale. BIGINT provides arbitrary-precision integers up to 4096 bits. When a DQA value must be used in a BIGINT context (e.g., arithmetic with BIGINT operands, or explicit CAST), a conversion is required.

Without a rigorous specification:
- Two implementations could convert the same DQA to different BIGINT values
- Scale handling could differ (scale ignored vs rounding)

### Why This RFC Exists

RFC-0105 defines DQA but does not define DQA→BIGINT conversion. RFC-0110 defines BIGINT but its DQA interop section only covers i128↔DQA (not full Dqa↔BigInt). This RFC fills that gap.

### RFC-0105 and RFC-0110 Coverage Analysis

| Conversion | RFC-0105 (DQA) | RFC-0110 (BIGINT) | This RFC |
|------------|-----------------|-------------------|----------|
| DQA → i128 | Not specified | `dqa_to_bigint()` returns i128 | Not needed |
| DQA → BigInt | Not specified | Not specified | **This RFC** |

**Key insight:** DQA→BIGINT conversion always succeeds because:
1. DQA's i64 value trivially fits in any BigInt (which has arbitrary precision)
2. The scale is simply ignored — only the raw i64 mantissa is extracted
3. No range checking is needed

## Input/Output Contract

```rust
/// Error variants for DQA→BIGINT conversion
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DqaToBigIntError {
    /// DQA input is non-canonical per RFC-0105
    NonCanonical {
        reason: String,
    },
}

/// DQA→BIGINT conversion result
pub type DqaToBigIntResult = Result<BigInt, DqaToBigIntError>;

/// DQA with scale metadata for round-trip preservation.
///
/// This type pairs a BigInt value with its original DQA scale,
/// allowing callers to recover the scale context after conversion.
pub struct BigIntWithScale {
    /// The BigInt value (raw mantissa)
    pub value: BigInt,
    /// The original DQA scale (0-18)
    pub scale: u8,
}
```

**Contract:**
- **Precondition:** Input is canonical per RFC-0105 (scale ≤ 18, value=0 implies scale=0, value≠0 implies no trailing decimal zeros)
- **If precondition satisfied:** Always succeeds → returns `Ok(BigInt)`
- **If precondition violated:** Returns `Err` — caller can choose to TRAP or handle gracefully

The return type is `Result<BigInt, DqaToBigIntError>`, not `BigInt`. This allows callers to handle non-canonical input gracefully. For VM-level TRAP semantics, callers can `.unwrap()` or use the `?` operator. This design is not a breaking change because the RFC is still in Draft status with no existing users.

**BigIntWithScale variant:** For use cases requiring scale preservation, `dqa_to_bigint_with_scale` returns `BigIntWithScale` which pairs the BigInt value with the original DQA scale. This enables round-trip conversion back to DQA using `bigint_with_scale_to_dqa` from RFC-0131.

## Scale Context Propagation

The scale in DQA represents decimal places. When converting to BIGINT (an integer type), the scale is **ignored** — only the raw mantissa (i64 value) is extracted.

**Mathematical definition:** For a DQA value `dqa = {value, scale}`, the conversion to BigInt is defined as:
```
dqa_to_bigint({v, s}) = BigInt(v)
```
where `v` is the raw i64 mantissa and `s` is the decimal scale. The function does NOT compute `v × 10^(-s)`. It extracts the mantissa field directly without interpreting the value as a decimal number.

| DQA Value | Scale | BIGINT Output | Rationale |
|-----------|-------|---------------|-----------|
| {42, 0} | 0 | 42 | Raw mantissa extracted |
| {1999, 2} | 2 | 1999 | Raw mantissa (1999) extracted, scale ignored |
| {1, 18} | 18 | 1 | Raw mantissa (1) extracted, scale ignored |

**Important:** This is NOT truncation of a decimal value. DQA{1999, 2} represents 19.99, but we extract the raw mantissa (1999), not the decimal value (19). The conversion does not interpret the DQA as a decimal number — it simply copies the i64 value field.

**This is a lossy conversion:** The scale information is discarded. The result BigInt(42) cannot be converted back to DQA{42, 2} — only to DQA{42, 0}.

## BigIntWithScale Value-Preserving Variant

For use cases requiring the numeric value and original scale, the `BigIntWithScale` type (defined in §Input/Output Contract) is used:

**⚠ Important:** The `BigIntWithScale` round-trip preserves the numeric VALUE but NOT the scale. When converting back via `bigint_with_scale_to_dqa` from RFC-0131, CANONICALIZE may reduce the scale.

**Formal specification:**

| Field | Type | Description |
|-------|------|-------------|
| `value` | BigInt | The BigInt extracted from DQA mantissa |
| `scale` | u8 | The original DQA scale (0-18) |

**Conversion functions:**

```rust
/// Value-preserving conversion that retains scale metadata.
/// ⚠ The scale may be reduced by CANONICALIZE in the reverse conversion.
pub fn dqa_to_bigint_with_scale(dqa: &Dqa) -> Result<BigIntWithScale, DqaToBigIntError> {
    let bigint = dqa_to_bigint(dqa)?;
    Ok(BigIntWithScale { value: bigint, scale: dqa.scale })
}
```

**Constraints for BigIntWithScale:**
- `scale` is always 0-18 (same as DQA scale bounds)
- `value` is a canonical BigInt per RFC-0110
- The pair `(value, scale)` can be converted back to DQA using `bigint_with_scale_to_dqa` from RFC-0131, but the output scale may differ from the original

**Test vectors for BigIntWithScale:**

```
V201: Round-trip — Positive with scale
  Input:  Dqa { value: 1999, scale: 2 }
  Output: Ok(BigIntWithScale { value: BigInt(1999), scale: 2 })

V202: Round-trip — Zero
  Input:  Dqa { value: 0, scale: 0 }
  Output: Ok(BigIntWithScale { value: BigInt::zero(), scale: 0 })

V203: Round-trip — Negative
  Input:  Dqa { value: -1999, scale: 2 }
  Output: Ok(BigIntWithScale { value: BigInt(-1999), scale: 2 })
```

## Constraints

| Constraint Type | Description |
|----------------|-------------|
| **Canonical input required** | Non-canonical DQA input returns `Err` — caller chooses to TRAP or handle gracefully |
| **Canonical succeeds** | Any canonical DQA input produces a valid BIGINT output |
| **Scale ignored** | Scale is not preserved in BIGINT output |
| **Sign preserved** | Negative DQA produces negative BIGINT |
| **Zero canonicalization** | DQA{0, 0} → BigInt::zero(). Note: DQA{0, s} for s ≠ 0 is non-canonical and returns `Err` |
| **Determinism** | Identical DQA input always produces identical BIGINT output |
| **Regulated industries** | Scale is intentionally discarded. For financial audit requirements (e.g., MiFID II, Dodd-Frank), implementations should log original scale separately. |
| **BigIntWithScale** | Scale is preserved in `BigIntWithScale.scale` field when using `dqa_to_bigint_with_scale`. The pair `(value, scale)` can be converted back to DQA using `bigint_with_scale_to_dqa` from RFC-0131. |

## Canonicalization Policy

**Input Canonicalization:** Non-canonical DQA inputs are a precondition violation. Implementations MUST verify canonical form at function entry and return `Err` on non-canonical input (e.g., `Dqa { value: 1000, scale: 3 }` with trailing zeros, or `Dqa { value: 0, scale: 6 }` with non-zero scale). Do not rely on upstream components having canonicalized — the caller may bypass the VM canonicalization path (e.g., SQL CAST with non-canonical literal). Callers that require TRAP semantics can use the `?` operator or `.unwrap()` to propagate errors as VM exceptions.

RFC-0105 §VM Canonicalization Rule requires canonicalization before use, but explicit CAST expressions can receive values that haven't passed through that path. This function must defensively verify canonical form.

**Output:** This conversion produces a BigInt from a canonical DQA input. The resulting BigInt correctly represents the same numeric value as the input DQA.

## Round-Trip Asymmetry

This conversion is NOT the inverse of RFC-0131's BIGINT→DQA:

| Direction | Conversion | Result |
|-----------|------------|--------|
| Forward | `DQA{1999, 2}` → BIGINT | BigInt(1999) |
| Reverse | `BigInt(1999), scale=2` → DQA | DQA{1999, 0} |

Round-trip: `DQA{1999, 2}` → BigInt(1999) → `DQA{1999, 0}` ≠ original

This asymmetry is intentional because:
1. DQA→BIGINT extracts raw mantissa, ignoring scale
2. BIGINT→DQA applies scale multiplication, then CANONICALIZE strips trailing zeros
3. Scale information is LOST in the RFC-0132 direction and cannot be recovered

### Lossless Round-Trip Case

Despite the asymmetry above, round-trip IS lossless when **scale=0**:

| Direction | Conversion | Result |
|-----------|------------|--------|
| Forward (RFC-0132) | `DQA{42, 0}` → BIGINT | BigInt(42) |
| Reverse (RFC-0131) | `BigInt(42), scale=0` → DQA | DQA{42, 0} |

**Lossless condition:** For any DQA with scale=0 (i.e., `DQA{x, 0}`), the round-trip `DQA{x, 0} → BigInt(x) → DQA{x, 0}` is lossless. Since all DQA values satisfy `|x| ≤ i64::MAX` by definition, this holds for all scale-0 DQA values.

### Negative Round-Trip
```
Input:  Dqa { value: -42, scale: 0 } → BIGINT → DQA
Output: BigInt(-42) → Dqa { value: -42, scale: 0 } ✓
Note: BigInt(-42) × 10^0 = -42, mantissa preserved.
```

### Composition Semantics

Chaining DQA→BIGINT with BIGINT→DQA does NOT recover the original DQA when scale > 0:

```sql
-- Step 1: DQA → BIGINT (RFC-0132, scale ignored)
SELECT CAST(dqa_col AS BIGINT) FROM accounts;
-- DQA{1999, 2} ($19.99) → BigInt(1999)

-- Step 2: BIGINT → DQA (RFC-0131, scale applied then canonicalized)
SELECT bigint_to_dqa(bigint_col, 2) FROM accounts;
-- BigInt(1999), scale=2 → DQA{1999, 0} (NOT DQA{1999, 2})
```

**⚠ WARNING: Scale information is permanently lost in the RFC-0132 direction.** The composition `CAST(CAST(dqa_col AS BIGINT) AS DQA(2))` produces `DQA{1999, 0}`, not the original `DQA{1999, 2}`.

Example: A $19.99 price stored as DQA{1999, 2} converts to BigInt(1999). Converting back with scale=2 produces DQA{1999, 0} (representing $1999.00, not $19.99) — a 100× error that could cause catastrophic financial losses if the scale loss is not understood.

**For regulated industries:** Implementations should log the original scale separately if audit requirements mandate data provenance. The RFC-0132 conversion discards scale intentionally and irreversibly.

## SQL Integration

DQA→BIGINT conversion appears in SQL CAST expressions:

```sql
-- Explicit CAST from DQA to BIGINT
SELECT CAST(dqa_col AS BIGINT) FROM account_balances;

-- This is ALWAYS VALID: Any DQA value fits in BIGINT
-- Dqa{9223372036854775807, 0} → BigInt(9223372036854775807)
```

**⚠ WARNING: Non-Standard SQL Semantics**

This conversion does NOT follow standard SQL CAST behavior:

| Standard SQL | This RFC |
|-------------|----------|
| `CAST(DQA{1999, 2} AS BIGINT)` → `19` (integer part) | `CAST(DQA{1999, 2} AS BIGINT)` → `1999` (raw mantissa) |

Standard SQL interprets `CAST(DQA{1999, 2} AS BIGINT)` as extracting the integer part (19). This RFC extracts the **raw mantissa** (1999), ignoring the scale entirely.

This behavior is intentional for the Numeric Tower's internal operations but will surprise SQL-familiar developers. Use this conversion only when "raw mantissa extraction" is the desired semantics.

### Scale Context in Mixed BigInt + DQA Operations

When a DQA value must be used in a BIGINT context (e.g., arithmetic with BIGINT operands), the scale is **not used** — the raw i64 mantissa is extracted directly:

```rust
// DQA value with scale
let dqa = Dqa::new(1999, 2)?;  // Represents 19.99 in decimal

// Scale is ignored — raw mantissa extraction
let bigint = dqa_to_bigint(&dqa);  // Returns BigInt(1999), NOT 19

// Scale-aware conversion (if needed) requires explicit handling
// by the calling context — this RFC does not specify such a function.
```

**Scale sourcing responsibility:**
- For explicit CAST: `CAST(... AS BIGINT)` — no scale in target type, raw mantissa extracted
- For mixed arithmetic: The operation's type coercion rules must specify which scale to use
- For internal conversions: The calling context must handle scale explicitly if needed

This RFC does not specify scale-coercion rules for mixed BigInt + DQA operations — that is the responsibility of the Numeric Tower's type system specification.

```sql
-- Scale ignored — raw mantissa extraction
SELECT CAST(dqa_col AS BIGINT) FROM currency_amounts;
-- Dqa{1999, 2} → BigInt(1999) — NOT 19 as standard SQL would give
```

### SQL Compatibility Mode

By default, `CAST(dqa_col AS BIGINT)` uses Numeric Tower semantics (raw mantissa extraction). For standard SQL behavior, use the `STANDARD` modifier:

```sql
-- Default: Numeric Tower semantics (raw mantissa extraction)
SELECT CAST(dqa_col AS BIGINT) FROM accounts;
-- DQA{1999, 2} → BigInt(1999)

-- Explicit: Numeric Tower semantics
SELECT CAST(dqa_col AS BIGINT NUMERIC_TOWER) FROM accounts;

-- Standard SQL semantics (integer part extraction)
SELECT CAST(dqa_col AS BIGINT STANDARD) FROM accounts;
-- DQA{1999, 2} → BigInt(19)
```

**Standard SQL algorithm variant:**

```
DQA_TO_BIGINT_STANDARD(dqa: Dqa) -> Result<BigInt, DqaToBigIntError>

INPUT:  dqa (Dqa { value: i64, scale: u8 })
OUTPUT: Result<BigInt, DqaToBigIntError>

STEPS:

0. VERIFY_CANONICAL
   If dqa.scale > 18:
     Return Err(NonCanonical { reason: "scale exceeds maximum" })
   If dqa.value == 0 and dqa.scale != 0:
     Return Err(NonCanonical { reason: "zero with non-zero scale" })
   If dqa.scale > 0 and dqa.value != 0 and dqa.value % 10 == 0:
     // Note: The modulo operation uses truncating division (toward zero), consistent
     // with Rust, C, and Java. In these languages, (-10) % 10 == 0, so negative values
     // with trailing zeros (e.g., {-10, 1}) are correctly detected as non-canonical.
     Return Err(NonCanonical { reason: "trailing decimal zeros" })

1. EXTRACT_INTEGER_PART
   If dqa.scale == 0:
     integer_part = dqa.value
   Else:
     divisor = POW10[dqa.scale]
     integer_part = dqa.value / divisor  // Truncating division toward zero

2. TO_BIGINT
   If integer_part >= 0:
     sign = false
     magnitude = integer_part as u64
   Else:
     sign = true
     magnitude = (integer_part == i64::MIN) ? (1u64 << 63) : ((-integer_part) as u64)
   // Note: Rust integer division truncates toward zero, matching standard SQL CAST behavior

3. CONSTRUCT_BIGINT
   If magnitude == 0:
     Return Ok(BigInt::zero())
   limbs = [magnitude as u64]
   Return Ok(BigInt { limbs: limbs, sign: sign })
```

**Rust API for Standard SQL mode:**

```rust
/// Conversion mode for DQA→BIGINT
pub enum DqaToBigIntMode {
    /// Numeric Tower semantics: extract raw mantissa, ignore scale
    NumericTower,
    /// Standard SQL semantics: extract integer part of decimal value
    StandardSql,
}

/// Convert DQA to BigInt with explicit mode.
pub fn dqa_to_bigint_mode(dqa: &Dqa, mode: DqaToBigIntMode) -> DqaToBigIntResult {
    match mode {
        DqaToBigIntMode::NumericTower => dqa_to_bigint(dqa),
        DqaToBigIntMode::StandardSql => dqa_to_bigint_standard(dqa),
    }
}

/// Standard SQL mode conversion.
fn dqa_to_bigint_standard(dqa: &Dqa) -> DqaToBigIntResult {
    // Algorithm DQA_TO_BIGINT_STANDARD per RFC-0132
}
```

#### Cast Semantics in Deterministic Context

| Source Type | Target Type | Behavior | Notes |
|-------------|-------------|----------|-------|
| DQA(n) | BIGINT | Ok(BigInt) for canonical | Scale ignored — raw mantissa extracted |
| DQA(0) | BIGINT | Ok(BigInt) for canonical | Integer representation |
| DQA(18) | BIGINT | Ok(BigInt) for canonical | Scale 18 → BigInt ignores scale, raw mantissa extracted |

### Function Signature

```rust
/// Convert DQA to BigInt.
///
/// For canonical DQA inputs, always returns Ok(BigInt).
/// For non-canonical inputs, returns Err — caller can choose to TRAP
/// or handle gracefully.
///
/// # Arguments
/// * `dqa` - The DQA value to convert
///
/// # Returns
/// Ok(BigInt) for canonical inputs, Err(DqaToBigIntError) for non-canonical
///
/// # Example
/// Dqa { value: 42, scale: 0 } → Ok(BigInt(42))
/// Dqa { value: 1999, scale: 2 } → Ok(BigInt(1999)) — scale ignored
///
/// # Notes
/// The scale is ignored, not truncated or rounded. This is consistent
/// with BIGINT being an integer type. Non-canonical inputs return Err
/// allowing graceful error handling.
pub fn dqa_to_bigint(dqa: &Dqa) -> DqaToBigIntResult
```

### Canonical Conversion Algorithm

```
DQA_TO_BIGINT(dqa: Dqa) -> Result<BigInt, DqaToBigIntError>

INPUT:  dqa (Dqa { value: i64, scale: u8 })
OUTPUT: Result<BigInt, DqaToBigIntError>

STEPS:

0. VERIFY_CANONICAL
   // Non-canonical DQA inputs are a precondition violation per RFC-0105.
   // Do NOT rely on upstream canonicalization — verify at entry point.
   If dqa.scale > 18:
     // Malformed: scale exceeds DQA maximum per RFC-0105
     Return Err(NonCanonical { reason: "scale exceeds maximum (18)" })
   If dqa.value == 0 and dqa.scale != 0:
     // Non-canonical zero
     Return Err(NonCanonical { reason: "zero with non-zero scale" })
   If dqa.scale > 0 and dqa.value != 0 and dqa.value % 10 == 0:
     // Has trailing decimal zeros — non-canonical per RFC-0105.
     // Note: When scale=0, there are no decimal places, so trailing digit zeros
     // in the integer value (e.g., {10,0}) are not decimal trailing zeros.
     // Only check for trailing zeros when scale > 0.
     // Note: The modulo operation uses truncating division (toward zero), consistent
     // with Rust, C, and Java. In these languages, (-10) % 10 == 0, so negative values
     // with trailing zeros (e.g., {-10, 1}) are correctly detected as non-canonical.
     Return Err(NonCanonical { reason: "trailing decimal zeros" })

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
     // Note: BigInt::zero() returns canonical zero with sign=false.
     // The sign variable from Step 2 is discarded, which is correct
     // because DQA{0, s} should always produce canonical zero.
     Return Ok(BigInt::zero())

   // magnitude is always <= u64::MAX because it comes from an i64
   // i64::MIN's magnitude is 2^63 which fits in u64
   limbs = [magnitude as u64]

   Return Ok(BigInt { limbs: limbs, sign: sign })
```

### Edge Cases

| DQA Input | BIGINT Output | Notes |
|------------|---------------|-------|
| {0, 0} | BigInt::zero() | Canonical zero |
| {42, 0} | BigInt(42) | Simple positive |
| {-42, 0} | BigInt(-42) | Simple negative |
| {1999, 2} | BigInt(1999) | Scale ignored, raw mantissa extracted |
| {i64::MAX, 0} | BigInt(i64::MAX) | Maximum i64 |
| {i64::MIN, 0} | BigInt(i64::MIN) | Minimum i64 |
| {i64::MIN, 3} | BigInt(-9223372036854775808) | Scale ignored, raw mantissa extracted |
| {-1, 18} | BigInt(-1) | Scale ignored, raw mantissa extracted |

## Relationship to Other RFCs

| RFC | Relationship | Precedence |
|-----|-------------|------------|
| RFC-0105 (DQA) | Input type | DQA semantics preserved (scale ignored — raw mantissa extraction) |
| RFC-0110 (BIGINT) | Output type | BIGINT operations apply after conversion |

**Precedence Rule:** This RFC does not override RFC-0105 or RFC-0110. All inputs must satisfy RFC-0105's canonical form requirements (non-canonical inputs MUST TRAP). All outputs satisfy RFC-0110's output requirements.

## Test Vectors

### V001: Zero
```
Input:  Dqa { value: 0, scale: 0 }
Output: Ok(BigInt::zero())
```

### V002: Small Positive
```
Input:  Dqa { value: 42, scale: 0 }
Output: Ok(BigInt::from(42i64))
```

### V003: Small Negative
```
Input:  Dqa { value: -42, scale: 0 }
Output: Ok(BigInt::from(-42i64))
```

### V004: Positive with Scale (Raw Mantissa Extraction)
```
Input:  Dqa { value: 1999, scale: 2 }
Output: Ok(BigInt::from(1999i64))
Note: Raw mantissa (1999) extracted, scale (2) is ignored.
DQA{1999, 2} represents 19.99 but we extract raw mantissa 1999.
```

### V005: i64::MAX
```
Input:  Dqa { value: 9223372036854775807, scale: 0 }
Output: Ok(BigInt::from(i64::MAX))
```

### V006: i64::MIN
```
Input:  Dqa { value: -9223372036854775808, scale: 0 }
Output: Ok(BigInt::from(i64::MIN))
```

### V008: Negative with Scale
```
Input:  Dqa { value: -1999, scale: 2 }
Output: Ok(BigInt::from(-1999i64))
```

### V009: Maximum Scale (18)
```
Input:  Dqa { value: 1, scale: 18 }
Output: Ok(BigInt::from(1i64))
Note: Raw mantissa (1) extracted, scale ignored.
DQA{1, 18} represents 0.000000000000000001 but we extract raw mantissa 1.
```

### V010: i64::MAX with Non-Zero Scale
```
Input:  Dqa { value: 9223372036854775807, scale: 2 }
Output: Ok(BigInt::from(i64::MAX))
Note: Scale (2) is ignored — raw mantissa 9223372036854775807 extracted
```

### V011: Minimum DQA Value
```
Input:  Dqa { value: -9223372036854775808, scale: 0 }
Output: Ok(BigInt::from(i64::MIN))
```

### V012: i64::MIN with Non-Zero Scale
```
Input:  Dqa { value: -9223372036854775808, scale: 6 }
Output: Ok(BigInt::from(-9223372036854775808i64))
Note: Raw mantissa extracted, scale ignored.
```

### V012b: i64::MIN with Maximum Scale
```
Input:  Dqa { value: -9223372036854775808, scale: 18 }
Output: Ok(BigInt::from(-9223372036854775808i64))
Note: Maximum scale (18) with minimum value. Raw mantissa extracted, scale ignored.
This boundary case explicitly confirms that scale does not affect the output.
```

### V013: Positive Value with Max Scale
```
Input:  Dqa { value: 1234567890123456789, scale: 18 }
Output: Ok(BigInt::from(1234567890123456789i64))
Note: Raw mantissa extracted, scale ignored.
```

### V014: Negative Value with Max Scale
```
Input:  Dqa { value: -1234567890123456789, scale: 18 }
Output: Ok(BigInt::from(-1234567890123456789i64))
Note: Raw mantissa extracted, scale ignored, sign preserved.
```

### V015: Large Positive Value
```
Input:  Dqa { value: 9223372036854775807, scale: 18 }
Output: Ok(BigInt::from(9223372036854775807i64))
Note: Maximum i64 with max scale
```

### V016: Scale 1 — Raw Mantissa Extraction
```
Input:  Dqa { value: 33, scale: 1 }
Output: Ok(BigInt::from(33i64))
Note: Raw mantissa extracted, scale ignored. DQA{33,1} represents 3.3, not 33.
```

### V017: Scale 1 with Small Value
```
Input:  Dqa { value: 5, scale: 1 }
Output: Ok(BigInt::from(5i64))
Note: Raw mantissa (5) extracted, scale ignored.
DQA{5, 1} represents 0.5, but we extract raw mantissa 5, not 0.
```

### V018: Negative with Scale 1
```
Input:  Dqa { value: -1, scale: 1 }
Output: Ok(BigInt::from(-1i64))
Note: Raw mantissa (-1) extracted, scale ignored.
DQA{-1, 1} represents -0.1, but we extract raw mantissa -1.
```

### V101: Standard SQL — Positive with Scale
```
Input:  Dqa { value: 1999, scale: 2 }
Output: Ok(BigInt::from(19i64))
Mode:   STANDARD (integer part extraction)
Note: 1999 / 10^2 = 19 (integer part of 19.99)
```

### V102: Standard SQL — Negative with Scale
```
Input:  Dqa { value: -1999, scale: 2 }
Output: Ok(BigInt::from(-19i64))
Mode:   STANDARD (integer part extraction)
Note: -1999 / 10^2 = -19 (truncation toward zero)
```

### V103: Standard SQL — Scale 0 (no change)
```
Input:  Dqa { value: 42, scale: 0 }
Output: Ok(BigInt::from(42i64))
Mode:   STANDARD
Note: No decimal places, identical to Numeric Tower mode
```

### V104: Standard SQL — Small decimal
```
Input:  Dqa { value: 5, scale: 1 }
Output: Ok(BigInt::from(0i64))
Mode:   STANDARD
Note: 5 / 10 = 0 (integer part of 0.5)
```

### V105: Standard SQL — Maximum scale
```
Input:  Dqa { value: 1, scale: 18 }
Output: Ok(BigInt::from(0i64))
Mode:   STANDARD
Note: 1 / 10^18 = 0
```

### V106: Standard SQL — Non-canonical zero with scale
```
Input:  Dqa { value: 0, scale: 6 }
Output: Err(NonCanonical { reason: "zero with non-zero scale" })
Mode:   STANDARD
```

### V107: Standard SQL — Non-canonical trailing zeros
```
Input:  Dqa { value: 1000, scale: 3 }
Output: Err(NonCanonical { reason: "trailing decimal zeros" })
Mode:   STANDARD
Note: 1000 % 10 == 0, so trailing zeros exist
```

### V108: Standard SQL — Scale exceeds maximum
```
Input:  Dqa { value: 42, scale: 19 }
Output: Err(NonCanonical { reason: "scale exceeds maximum" })
Mode:   STANDARD
```

### V109: Standard SQL — i64::MIN with scale 1
```
Input:  Dqa { value: -9223372036854775808, scale: 1 }
Output: Ok(BigInt::from(-922337203685477580i64))
Mode:   STANDARD
Note: -9223372036854775808 / 10 = -922337203685477580 (truncating toward zero)
```

### V110: Standard SQL — Negative truncation edge case {-11, 1}
```
Input:  Dqa { value: -11, scale: 1 }
Output: Ok(BigInt::from(-1i64))
Mode:   STANDARD
Note: -11 / 10 = -1 (truncating toward zero, NOT floor which gives -2)
```

### V111: Standard SQL — Negative truncation edge case {-15, 1}
```
Input:  Dqa { value: -15, scale: 1 }
Output: Ok(BigInt::from(-1i64))
Mode:   STANDARD
Note: -15 / 10 = -1 (truncating toward zero, NOT floor which gives -2)
```

### V112: Standard SQL — Negative truncation {-1999, 2}
```
Input:  Dqa { value: -1999, scale: 2 }
Output: Ok(BigInt::from(-19i64))
Mode:   STANDARD
Note: -1999 / 100 = -19 (truncating toward zero)
```

## Implementation Notes

### In determin crate

This conversion is implemented in `determin/src/bigint.rs` as:

```rust
use crate::dqa::Dqa;

// BigIntWithScale is defined in §Input/Output Contract.

// Convert DQA to BigInt.
///
/// For canonical DQA inputs, always returns Ok(BigInt).
/// For non-canonical inputs, returns Err — caller can choose to TRAP
/// or handle gracefully.
///
/// This function exists in bigint.rs to keep conversion functions
/// near the target type, following RFC-0110's organization.
///
impl BigInt {
    /// Create a BigInt from a DQA value.
    ///
    /// Scale is ignored — raw mantissa extraction per RFC-0132.
    /// Returns Err for non-canonical inputs.
    pub fn from_dqa(dqa: &Dqa) -> DqaToBigIntResult {
        // Algorithm per RFC-0132
    }
}

/// Convert DQA to BigInt (free function form).
pub fn dqa_to_bigint(dqa: &Dqa) -> DqaToBigIntResult {
    BigInt::from_dqa(dqa)
}

/// Round-trip safe conversion that preserves scale metadata.
pub fn dqa_to_bigint_with_scale(dqa: &Dqa) -> Result<BigIntWithScale, DqaToBigIntError> {
    let bigint = dqa_to_bigint(dqa)?;
    Ok(BigIntWithScale { value: bigint, scale: dqa.scale })
}
```

### Gas Cost

DQA→BIGINT conversion is O(1) because i64 trivially fits in BigInt's arbitrary range. Gas cost depends on the conversion mode:

```
GAS_NUMERIC_TOWER = 5  // Fixed gas allocation — raw mantissa extraction, no division
GAS_STANDARD_SQL = 7   // Fixed gas allocation — requires division for integer part
```

**Numeric Tower mode (default):** Lower gas because Step 1 extracts the raw i64 value directly without any arithmetic.

**Standard SQL mode:** Higher gas because Step 1 requires integer division (`dqa.value / POW10[dqa.scale]`) to extract the integer part.

Both modes have fixed gas allocations because:
- No limb iteration needed (i64 always fits in 1 limb)
- Step 0 VERIFY_CANONICAL performs a constant number of O(1) operations (comparisons, one modulo)
- No scale adjustment beyond the optional division in Standard SQL mode
- Canonical inputs always succeed; non-canonical inputs return Err

**Note:** "O(1)" refers to algorithmic complexity (constant time), not the number of CPU operations. The modulo operation, comparisons, and division are constant-time for i64 values.

**Gas for error paths:** Gas is charged regardless of whether the conversion succeeds or returns Err. Both success and error paths consume the same fixed gas allocation.

## Error Handling and Diagnostics

### Compile-Time Errors

For **canonical** DQA inputs, conversion always succeeds. The compiler does not emit errors for canonical inputs.

```
-- Canonical DQA input — always valid:
SELECT CAST(dqa_col AS BIGINT) FROM any_table;
-- No error possible for canonical inputs
```

### Runtime Behavior

| Scenario | Behavior | Notes |
|----------|----------|-------|
| Canonical DQA | Returns Ok(BigInt) | No errors possible |
| Non-canonical DQA | Returns Err | Caller can choose to TRAP or handle gracefully |

**Note:** Unlike BIGINT→DQA (which returns Result with Error::OutOfRange), DQA→BIGINT for canonical inputs always returns Ok. Non-canonical inputs (e.g., `{0, s≠0}` or `{x, s}` where `x % 10 == 0`) return Err per Step 0. Callers requiring TRAP semantics can use `.unwrap()` or the `?` operator.

## Formal Verification Framework

### Theorem Hierarchy

| # | Theorem | Property | Status |
|---|---------|----------|--------|
| T1 | Determinism | Bit-identical results across platforms | Required |
| T2 | Range Preservation | Output BigInt can represent input value | Required |
| T3 | Raw Mantissa Extraction | Scale is ignored — raw i64 value extracted | Required |
| T4 | Sign Preservation | Negative DQA produces negative BigInt | Required |
| T5 | Zero Canonicalization | DQA{0, 0} → BigInt::zero() | Required |

### Theorem Specifications

**Theorem T1 (Determinism):** For identical DQA input, the conversion always produces identical BIGINT output.

**Theorem T2 (Range Preservation):** For any valid DQA input, the output BigInt can represent the same integer value (i64 always fits in BigInt).

**Theorem T3 (Raw Mantissa Extraction):** The output BigInt equals `dqa.value` as an integer, without interpretation of the scale.

**Theorem T4 (Sign Preservation):** If `dqa.value < 0`, then `result.sign = true`; if `dqa.value ≥ 0`, then `result.sign = false`. For zero, T5 additionally canonicalizes to `BigInt::zero()`.

**Theorem T5 (Zero Canonicalization):** `dqa_to_bigint(Dqa { value: 0, scale: 0 }) = BigInt::zero()`. Note: `Dqa { value: 0, scale: s }` for s ≠ 0 is non-canonical per RFC-0105 and MUST TRAP.

## Implementation Checklist

| Mission | Description | Status | Complexity |
|---------|-------------|--------|------------|
| M1 | `dqa_to_bigint` core algorithm | Pending | Low |
| M2 | i64::MIN special case handling | Pending | Low |
| M3 | Scale ignored (raw mantissa extraction) | Pending | Low |
| M4 | Sign handling | Pending | Low |
| M5 | Test vector suite (33 vectors: V001-V018, V101-V112, V201-V203) | Pending | Low |
| M6 | Integration with BigInt type | Pending | Low |
| M7 | `dqa_to_bigint_mode` with DqaToBigIntMode enum | Pending | Low |
| M8 | `dqa_to_bigint_with_scale` with BigIntWithScale | Pending | Low |

## Future Work

- F1: BIGINT→DECIMAL conversion (see RFC-0133)
- F2: DECIMAL→BIGINT conversion (see RFC-0134)

**Note:** BIGINT→DQA conversion is specified in companion RFC-0131.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.19 | 2026-03-24 | (Current) HIGH: Clarified BigIntWithScale round-trip preserves VALUE not SCALE (R17-132-H1). MEDIUM: Added Standard SQL negative truncation test vectors V110-V112 (R17-132-M1). MEDIUM: Updated Implementation Checklist to 33 vectors (R17-132-L1). |
| 1.18 | 2026-03-24 | CRITICAL: Removed misleading i64::MIN note in Standard SQL Step 1 (R16-132-C2). HIGH: Standardized canonical example to {1999, 2} throughout (R16-132-C1). HIGH: Added normative note about modulo truncating division for negative values (R16-132-H1). HIGH: Consolidated BigIntWithScale to single definition location (R16-132-H2). MEDIUM: Changed DQA(19.99) notation to DQA{1999, 2} (R16-132-M1). MEDIUM: Added non-canonical test vectors V106-V108 and i64::MIN test V109 (R16-132-M2). MEDIUM: Added gas note for error paths (R16-132-M3). MEDIUM: Strengthened round-trip asymmetry warning with concrete financial example (R16-132-M4). LOW: Updated Implementation Checklist to reflect 30 vectors (R16-132-L2). |
| 1.17 | 2026-03-24 | MEDIUM: Added BigIntWithScale to Input/Output Contract and Constraints (R15-132-M1). LOW: Updated Gas Cost section with separate GAS_NUMERIC_TOWER (5) and GAS_STANDARD_SQL (7) costs (R15-132-L1). LOW: Updated Implementation Checklist with M7 (dqa_to_bigint_mode) and M8 (dqa_to_bigint_with_scale) and corrected test vector count (26 vectors) (R15-132-L2). |
| 1.16 | 2026-03-24 | FIXED: 15 malformed test vectors — added missing closing parentheses. FIXED: Standard SQL algorithm — completed Step 3 (CONSTRUCT_BIGINT) and added error propagation. FIXED: Added Rust API for Standard SQL mode (DqaToBigIntMode enum, dqa_to_bigint_mode function). FIXED: Added formal specification for BigIntWithScale (struct, constraints, test vectors). |
| 1.15 | 2026-03-24 | CRITICAL: Changed return type from BigInt to Result<BigInt, DqaToBigIntError> — not a breaking change in draft status (R13-132-C1). MEDIUM: Added SQL Compatibility Mode with STANDARD modifier for standard SQL semantics (R13-132-M1). MEDIUM: Added BigIntWithScale round-trip safe variant (R13-132-M2). |
| 1.14 | 2026-03-24 | MEDIUM: Added V012b test vector for {i64::MIN, 18} — explicit boundary case for maximum scale with minimum value. MEDIUM: Added Composition Semantics section documenting chained conversion behavior and 100× magnitude warning. LOW: Added regulated industries constraint row (MiFID II, Dodd-Frank) noting scale should be logged separately. |
| 1.13 | 2026-03-24 | LOW: Clarified "scale ignored" mathematical definition — added formal definition and note that conversion does NOT compute v × 10^(-s). MEDIUM: Added language-agnostic modulo specification to trailing-zero check (truncating division, consistent with Rust/C/Java). MEDIUM: Clarified gas cost wording — O(1) is constant-time algorithmic complexity, not same number of operations. |
| 1.12 | 2026-03-24 | LOW: Removed duplicate v1.9 version history entry (copy-paste artifact). |
| 1.11 | 2026-03-24 | CRITICAL: Fixed trailing-zero check — must be `scale > 0 and value % 10 == 0` not `value % 10 == 0` (R6C4). MEDIUM: Fixed Input/Output Contract — now states precondition explicitly (R6M5). |
| 1.10 | 2026-03-24 | (Internal version — changes incorporated into v1.11) |
| 1.9 | 2026-03-24 | CRITICAL: Fixed Round-Trip Asymmetry reverse row — {199900,2} → {1999,0} (R9-132-C2). HIGH: Fixed T5 theorem table — DQA{0,0} not DQA{0,any} (R9-132-H1). HIGH: Fixed Gas Cost rationale — Step 0 has O(1) checks (R9-132-H2). MEDIUM: Fixed Error Handling section — canonical succeeds, non-canonical TRAPs (R9-132-M1). MEDIUM: Fixed V004/V007 duplicate — V007 merged into V004 (R9-132-M2). MEDIUM: Added scale>18 check to Step 0 (R9-132-M3). LOW: Fixed version history ordering — v1.7 after v1.8 (R9-132-L1). |
| 1.8 | 2026-03-23 | CRITICAL: Clarified return type semantics — DqaToBigIntResult=BigInt means TRAP is panic/VM abort, not Result return (R8-132-C1). HIGH: Fixed docstring example — {4200,2} is non-canonical, changed to {1999,2} (R8-132-H1). MEDIUM: Added Step 0 VERIFY_CANONICAL to algorithm (R8-132-M1). MEDIUM: Fixed V004 — {4200,2} is non-canonical, changed to {1999,2} (R8-132-M2). MEDIUM: Fixed V016 — {100,1} is non-canonical, changed to {33,1} (R8-132-M3). LOW: Fixed Edge Cases and Scale Context Propagation tables — removed non-canonical {4200,2} and {42000,3} rows (R8-132-L1). |
| 1.7 | 2026-03-23 | (Internal version — changes incorporated into v1.8) |
| 1.6 | 2026-03-23 | Process: Version header now matches history entry (R4H4). |
| 1.5 | 2026-03-23 | MEDIUM: Fixed version header (was 1.3, now 1.4) (R3M5). Removed dangling DqaToBigIntInput struct (R3M6). LOW: Fixed relationship table "scale truncation" wording (R3L3). |
| 1.4 | 2026-03-23 | MEDIUM: Changed "truncation" to "raw mantissa extraction" throughout. |
| 1.3 | 2026-03-23 | Critical fixes: Removed unreachable dead code from Step 3 (HIGH-H5), added non-standard SQL semantics warning (HIGH-H6), fixed version header (1.1→1.2), removed RFC-0131 from Future Work |
| 1.2 | 2026-03-23 | Critical fix: Changed "truncation" to "raw mantissa extraction" throughout (CRITICAL-1), fixed V004/V017/V018 notes that contradicted output (CRITICAL-2/MEDIUM-1), added canonicalization policy section (HIGH-1), added round-trip asymmetry documentation |
| 1.1 | 2026-03-23 | Enhanced: Added Input/Output Contract, Scale Context Propagation, SQL Integration, Constraints, Error Handling & Diagnostics, Formal Verification Framework (5 theorems), Implementation Checklist, expanded test vectors from 8 to 18 |
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
