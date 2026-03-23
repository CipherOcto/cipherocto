# RFC-0132 (Numeric/Math): DQA to BIGINT Conversion

## Status

**Version:** 1.7 (Draft)
**Status:** Draft
**Depends On:** RFC-0110 (BIGINT), RFC-0105 (DQA)
**Category:** Numeric/Math

## Summary

This RFC specifies the conversion algorithm from DQA (RFC-0105, i64 value with 0-18 decimal scale) to BIGINT (RFC-0110, arbitrary-precision integer up to 4096 bits). This conversion is necessary for the Numeric Tower to support operations that require DQA values to be used in BIGINT contexts, and for explicit CAST expressions between these types.

This conversion always succeeds for well-formed DQA inputs (valid i64 value, scale 0-18 per RFC-0105) because DQA's i64 value trivially fits within BIGINT's arbitrary range.

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
/// DQA→BIGINT conversion result
/// Note: Unlike most conversions, this ALWAYS succeeds for well-formed DQA inputs
pub type DqaToBigIntResult = BigInt;
```

**Important:** DQA→BIGINT conversion cannot fail for well-formed DQA inputs (valid i64 value, scale 0-18 per RFC-0105).

## Scale Context Propagation

The scale in DQA represents decimal places. When converting to BIGINT (an integer type), the scale is **ignored** — only the raw mantissa (i64 value) is extracted:

| DQA Value | Scale | BIGINT Output | Rationale |
|-----------|-------|---------------|-----------|
| {42, 0} | 0 | 42 | Raw mantissa extracted |
| {42, 2} | 2 | 42 | Raw mantissa (42) extracted, scale ignored |
| {4200, 2} | 2 | 4200 | Raw mantissa (4200) extracted, scale ignored |
| {42000, 3} | 3 | 42000 | Raw mantissa (42000) extracted, scale ignored |

**Important:** This is NOT truncation of a decimal value. DQA{42, 2} represents 0.42, but we extract the raw mantissa (42), not the decimal value (0.42). The conversion does not interpret the DQA as a decimal number — it simply copies the i64 value field.

**This is a lossy conversion:** The scale information is discarded. The result BigInt(42) cannot be converted back to DQA{42, 2} — only to DQA{42, 0}.

## Constraints

| Constraint Type | Description |
|----------------|-------------|
| **Always succeeds** | Any valid DQA input produces a valid BIGINT output |
| **Scale ignored** | Scale is not preserved in BIGINT output |
| **Sign preserved** | Negative DQA produces negative BIGINT |
| **Zero canonicalization** | DQA{0, any} → BigInt::zero() |
| **Determinism** | Identical DQA input always produces identical BIGINT output |
| **Canonical input required** | Non-canonical DQA input is a precondition violation — implementations MUST TRAP per RFC-0105 and RFC-0110 |

## Canonicalization Policy

**Input Canonicalization:** RFC-0105 guarantees all DQA values are canonical at all input boundaries. The VM mandates canonicalization before use in comparison, serialization, hashing, storage, and control-flow evaluation. Non-canonical DQA inputs are a precondition violation — implementations MUST TRAP on non-canonical DQA input per RFC-0105 §VM Canonicalization Rule and RFC-0110 §Input Canonicalization Requirement.

This conversion operates on DQA values that have already passed the VM's canonicalization check. Since canonical DQA values have unique representations, no additional canonicalization is needed before conversion.

**Output:** This conversion produces a BigInt from a canonical DQA input. The resulting BigInt correctly represents the same numeric value as the input DQA.

## Round-Trip Asymmetry

This conversion is NOT the inverse of RFC-0131's BIGINT→DQA:

| Direction | Conversion | Result |
|-----------|------------|--------|
| Forward | `DQA{1999, 2}` → BIGINT | BigInt(1999) |
| Reverse | `BigInt(1999), scale=2` → DQA | DQA{199900, 2} |

Round-trip: `DQA{1999, 2}` → BigInt(1999) → `DQA{199900, 2}` ≠ original

This asymmetry is intentional because:
1. DQA→BIGINT extracts raw mantissa, ignoring scale
2. BIGINT→DQA applies scale multiplication to the mantissa
3. Scale information is LOST in the forward direction and cannot be recovered

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
| `CAST(DQA(19.99) AS BIGINT)` → `19` (integer part) | `CAST(DQA(19.99) AS BIGINT)` → `1999` (raw mantissa) |

Standard SQL interprets `CAST(19.99 AS BIGINT)` as extracting the integer part (19). This RFC extracts the **raw mantissa** (1999), ignoring the scale entirely.

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

#### Cast Semantics in Deterministic Context

| Source Type | Target Type | Behavior | Notes |
|-------------|-------------|----------|-------|
| DQA(n) | BIGINT | Always succeeds | Scale ignored — raw mantissa extracted |
| DQA(0) | BIGINT | Always succeeds | Integer representation |
| DQA(18) | BIGINT | Always succeeds | Scale 18 → BigInt ignores scale, raw mantissa extracted |

### Function Signature

```rust
/// Convert DQA to BigInt.
///
/// This conversion always succeeds because DQA's i64 value fits
/// in any BigInt. The decimal scale is ignored — only the raw
/// i64 mantissa is extracted.
///
/// # Arguments
/// * `dqa` - The DQA value to convert
///
/// # Returns
/// BigInt representation of the DQA value (scale is ignored)
///
/// # Example
/// Dqa { value: 42, scale: 0 } → BigInt(42)
/// Dqa { value: 4200, scale: 2 } → BigInt(4200) — scale ignored
///
/// # Notes
/// The scale is ignored, not truncated or rounded. This is consistent
/// with BIGINT being an integer type.
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
     // Note: BigInt::zero() returns canonical zero with sign=false.
     // The sign variable from Step 2 is discarded, which is correct
     // because DQA{0, s} should always produce canonical zero.
     Return BigInt::zero()

   // magnitude is always <= u64::MAX because it comes from an i64
   // i64::MIN's magnitude is 2^63 which fits in u64
   limbs = [magnitude as u64]

   Return BigInt { limbs: limbs, sign: sign }
```

### Edge Cases

| DQA Input | BIGINT Output | Notes |
|------------|---------------|-------|
| {0, 0} | BigInt::zero() | Canonical zero |
| {42, 0} | BigInt(42) | Simple positive |
| {-42, 0} | BigInt(-42) | Simple negative |
| {4200, 2} | BigInt(4200) | Scale ignored, raw mantissa extracted |
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

### V004: Positive with Scale (Raw Mantissa Extraction)
```
Input:  Dqa { value: 4200, scale: 2 }
Output: BigInt::from(4200i64)
Note: Raw mantissa (4200) extracted, scale (2) is ignored.
DQA{4200, 2} represents 42.00 but we extract raw mantissa 4200.
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
Note: Raw mantissa (1) extracted, scale ignored.
DQA{1, 18} represents 0.000000000000000001 but we extract raw mantissa 1.
```

### V010: i64::MAX with Non-Zero Scale
```
Input:  Dqa { value: 9223372036854775807, scale: 2 }
Output: BigInt::from(i64::MAX)
Note: Scale (2) is ignored — raw mantissa 9223372036854775807 extracted
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
Note: Raw mantissa extracted, scale ignored.
```

### V013: Positive Value with Max Scale
```
Input:  Dqa { value: 1234567890123456789, scale: 18 }
Output: BigInt::from(1234567890123456789i64)
Note: Raw mantissa extracted, scale ignored.
```

### V014: Negative Value with Max Scale
```
Input:  Dqa { value: -1234567890123456789, scale: 18 }
Output: BigInt::from(-1234567890123456789i64)
Note: Raw mantissa extracted, scale ignored, sign preserved.
```

### V015: Large Positive Value
```
Input:  Dqa { value: 9223372036854775807, scale: 18 }
Output: BigInt::from(9223372036854775807i64)
Note: Maximum i64 with max scale
```

### V016: Scale 1 with Integer Value
```
Input:  Dqa { value: 100, scale: 1 }
Output: BigInt::from(100i64)
Note: Raw mantissa extracted, scale ignored.
```

### V017: Scale 1 with Small Value
```
Input:  Dqa { value: 5, scale: 1 }
Output: BigInt::from(5i64)
Note: Raw mantissa (5) extracted, scale ignored.
DQA{5, 1} represents 0.5, but we extract raw mantissa 5, not 0.
```

### V018: Negative with Scale 1
```
Input:  Dqa { value: -1, scale: 1 }
Output: BigInt::from(-1i64)
Note: Raw mantissa (-1) extracted, scale ignored.
DQA{-1, 1} represents -0.1, but we extract raw mantissa -1.
```

## Implementation Notes

### In determin crate

This conversion is implemented in `determin/src/bigint.rs` as:

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
    /// Scale is ignored — raw mantissa extraction per RFC-0132.
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

### Gas Cost

DQA→BIGINT conversion is O(1) because i64 trivially fits in BigInt's arbitrary range. Gas cost should be:
```
GAS = 5  // Fixed cost, no variable component
```

This is a fixed cost because:
- No limb iteration needed (i64 always fits in 1 limb)
- No range checking needed (always succeeds)
- No scale adjustment needed (scale is ignored)

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
| T3 | Raw Mantissa Extraction | Scale is ignored — raw i64 value extracted | Required |
| T4 | Sign Preservation | Negative DQA produces negative BigInt | Required |
| T5 | Zero Canonicalization | DQA{0, any} → BigInt::zero() | Required |

### Theorem Specifications

**Theorem T1 (Determinism):** For identical DQA input, the conversion always produces identical BIGINT output.

**Theorem T2 (Range Preservation):** For any valid DQA input, the output BigInt can represent the same integer value (i64 always fits in BigInt).

**Theorem T3 (Raw Mantissa Extraction):** The output BigInt equals `dqa.value` as an integer, without interpretation of the scale.

**Theorem T4 (Sign Preservation):** If `dqa.value < 0`, then `result.sign = true`; if `dqa.value ≥ 0`, then `result.sign = false`. For zero, T5 additionally canonicalizes to `BigInt::zero()`.

**Theorem T5 (Zero Canonicalization):** `dqa_to_bigint(Dqa { value: 0, scale: s }) = BigInt::zero()` for any valid scale s.

## Implementation Checklist

| Mission | Description | Status | Complexity |
|---------|-------------|--------|------------|
| M1 | `dqa_to_bigint` core algorithm | Pending | Low |
| M2 | i64::MIN special case handling | Pending | Low |
| M3 | Scale ignored (raw mantissa extraction) | Pending | Low |
| M4 | Sign handling | Pending | Low |
| M5 | Test vector suite (18 vectors) | Pending | Low |
| M6 | Integration with BigInt type | Pending | Low |

## Future Work

- F1: BIGINT→DECIMAL conversion (see RFC-0133)
- F2: DECIMAL→BIGINT conversion (see RFC-0134)

**Note:** BIGINT→DQA conversion is specified in companion RFC-0131.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.7 | 2026-03-23 | CRITICAL: Rewrote Canonicalization Policy section — non-canonical input MUST TRAP per RFC-0105/RFC-0110 (R6-132-C1, C2). HIGH: Removed all "truncation" language — replaced with "scale ignored" throughout (R6-132-H2, H3). MEDIUM: Fixed v1.4 changelog accuracy; removed precedence override clause (R6-132-M2, R6-X1). LOW: Added negative round-trip test vector (R6-X2). |
| 1.6 | 2026-03-23 | Process: Version header now matches history entry (R4H4). |
| 1.5 | 2026-03-23 | MEDIUM: Fixed version header (was 1.3, now 1.4) (R3M5). Removed dangling DqaToBigIntInput struct (R3M6). LOW: Fixed relationship table "scale truncation" wording (R3L3). |
| 1.4 | 2026-03-23 | MEDIUM: Changed "truncation" to "raw mantissa extraction" throughout. |
| 1.3 | 2026-03-23 | Critical fixes: Removed unreachable dead code from Step 3 (HIGH-H5), added non-standard SQL semantics warning (HIGH-H6), fixed version header (1.1→1.2), removed RFC-0131 from Future Work |
| 1.2 | 2026-03-23 | Critical fix: Changed "truncation" to "raw mantissa extraction" throughout (CRITICAL-1), fixed V004/V017/V018 notes that contradicted output (CRITICAL-2/MEDIUM-1), added canonicalization policy section (HIGH-1), added round-trip asymmetry documentation |
| 1.1 | 2026-03-23 | Enhanced: Added Input/Output Contract, Scale Context Propagation, SQL Integration, Constraints, Error Handling & Diagnostics, Formal Verification Framework (5 theorems), Implementation Checklist, expanded test vectors from 8 to 18 |
| 1.0 | 2026-03-23 | Initial draft |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
