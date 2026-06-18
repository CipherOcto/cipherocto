# RFC-0135 (Numeric/Math): DECIMAL ↔ DQA Conversion Review

## Status

**Version:** 1.0 (Draft)
**Status:** Draft
**Depends On:** RFC-0105 (DQA), RFC-0111 (DECIMAL)
**Category:** Numeric/Math

## Summary

This RFC reviews the existing DECIMAL↔DQA conversion functions in the determin crate (`decimal_to_dqa` and `dqa_to_decimal`) and verifies they match the specifications in RFC-0105 and RFC-0111. This is a **review RFC** — it does not specify new functionality but documents the correctness of existing implementations.

**Note:** This RFC does NOT create new functions. It verifies existing functions are correctly specified.

## Existing Functions

The determin crate provides the following DECIMAL↔DQA conversions:

### `decimal_to_dqa`

```rust
// determin/src/decimal.rs
pub fn decimal_to_dqa(d: &Decimal) -> Result<Dqa, DecimalError>
```

**RFC-0111 says:**
> DECIMAL → DQA Conversion
> Converts Decimal to Dqa with scale alignment and RoundHalfEven rounding.
> TRAP if DECIMAL scale > 18 or result outside DQA range (i64).

### `dqa_to_decimal`

```rust
// determin/src/decimal.rs
pub fn dqa_to_decimal(dqa: &Dqa) -> Result<Decimal, DecimalError>
```

**RFC-0105 says:**
> DQA → DECIMAL Conversion
> Converts Dqa to Decimal by zero-extending to Decimal scale.
> TRAP if result outside DECIMAL range.

## Review: decimal_to_dqa

### RFC-0111 Specification

From RFC-0111 §DECIMAL → DQA:

```
DECIMAL_TO_DQA(d: Decimal) -> Result[Dqa, DecimalError]

INPUT:  d (Decimal { mantissa: i128, scale: u8 })
OUTPUT: Dqa { value: i64, scale: u8 } or error

STEPS:

1. SCALE_CHECK
   If d.scale > 18:
     return Error(ConversionLoss)
   // DQA max scale is 18, DECIMAL max scale is 36

2. RANGE_CHECK
   // The mantissa must fit in i64
   If |d.mantissa| > i64::MAX:
     return Error(Overflow)

3. CONSTRUCT
   // Scale is preserved (no rounding needed since scale <= 18)
   Return Dqa { value: d.mantissa as i64, scale: d.scale }
```

### Implementation Review

```rust
// determin/src/decimal.rs (lines 758-771)
pub fn decimal_to_dqa(d: &Decimal) -> Result<Dqa, DecimalError> {
    use crate::dqa::MAX_SCALE as DQA_MAX_SCALE;

    // DQA max scale is 18, Decimal max scale is 36
    if d.scale > DQA_MAX_SCALE {
        return Err(DecimalError::ConversionLoss);
    }

    // Scale is within DQA range - no rounding needed, just check range
    if d.mantissa > i64::MAX as i128 || d.mantissa < i64::MIN as i128 {
        return Err(DecimalError::Overflow);
    }
    Dqa::new(d.mantissa as i64, d.scale).map_err(|_| DecimalError::Overflow)
}
```

### Verification Checklist

| RFC-0111 Requirement | Implementation | Status |
|---------------------|----------------|--------|
| TRAP if scale > 18 | `if d.scale > DQA_MAX_SCALE` | ✅ CORRECT |
| TRAP if value > i64::MAX | `if d.mantissa > i64::MAX as i128` | ✅ CORRECT |
| TRAP if value < i64::MIN | `d.mantissa < i64::MIN as i128` | ✅ CORRECT |
| Return Dqa with same scale | `Dqa::new(d.mantissa as i64, d.scale)` | ✅ CORRECT |

**Verdict:** Implementation matches RFC-0111 specification exactly. ✅

## Review: dqa_to_decimal

### RFC-0105 Specification

From RFC-0105 §DQA → DECIMAL:

```
DQA_TO_DECIMAL(dqa: Dqa) -> Result[Decimal, DecimalError]

INPUT:  dqa (Dqa { value: i64, scale: u8 })
OUTPUT: Decimal { mantissa: i128, scale: u8 } or error

STEPS:

1. RANGE_CHECK
   // DECIMAL range is ±(10^36 - 1)
   // i64 max is 9.2×10^18, which is much smaller than 10^36
   // So i64 value always fits in DECIMAL

2. CONSTRUCT
   // Zero-extend: Dqa value becomes DECIMAL mantissa
   Return Decimal { mantissa: dqa.value as i128, scale: dqa.scale }
```

### RFC-0111 Specification

From RFC-0111 §DQA → DECIMAL:

> DQA → DECIMAL Conversion
> Converts Dqa to Decimal by zero-extending to Decimal scale.
> TRAP if result outside DECIMAL range.

### Implementation Review

```rust
// determin/src/decimal.rs (lines 777-782)
pub fn dqa_to_decimal(dqa: &Dqa) -> Result<Decimal, DecimalError> {
    // Decimal can represent higher scales than DQA
    // Simply construct with the same value and scale
    // The Decimal::new will canonicalize if needed
    Decimal::new(dqa.value as i128, dqa.scale)
}
```

### Verification Checklist

| Requirement | Implementation | Status |
|-------------|----------------|--------|
| Always succeeds for valid DQA | `Decimal::new(dqa.value as i128, dqa.scale)` | ✅ CORRECT |
| Value fits (i64 < 10^36) | Implicit in `Decimal::new` | ✅ CORRECT |
| Scale preserved | `dqa.scale` passed directly | ✅ CORRECT |

**Note:** The implementation relies on `Decimal::new` which canonicalizes and validates. For DQA→DECIMAL, the only possible error is scale > 36, but DQA's max scale is 18, so this cannot happen.

**Verdict:** Implementation matches RFC-0105 and RFC-0111 specifications. ✅

## Summary

| Function | RFC Source | Implementation | Verdict |
|----------|-----------|----------------|---------|
| `decimal_to_dqa` | RFC-0111 | determin/src/decimal.rs:758-771 | ✅ CORRECT |
| `dqa_to_decimal` | RFC-0105, RFC-0111 | determin/src/decimal.rs:777-782 | ✅ CORRECT |

## Recommendations

1. **No changes required** — existing implementations are correct
2. **Add test vectors** — while algorithms are correct, explicit test vectors per RFC format would improve verification
3. **Document error types** — the DecimalError variants used (ConversionLoss, Overflow) are appropriate per RFC-0111

## Future Work

- F1: BIGINT→DQA conversion (see RFC-0131)
- F2: DQA→BIGINT conversion (see RFC-0132)
- F3: BIGINT→DECIMAL conversion (see RFC-0133)
- F4: DECIMAL→BIGINT conversion (see RFC-0134)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-23 | Initial draft — review of existing functions |

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
