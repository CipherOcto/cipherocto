# Adversarial Review: Mission 0202-b-bigint-decimal-schema-value (Round 2)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-b-bigint-decimal-schema-value.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 2

---

## Executive Summary

Round 1 review identified 6 issues (C1 HIGH, C2 HIGH, C3/C4 MODERATE, C5/C6 LOW). Round 1 fixes were applied and committed. This Round 2 review verifies the fixes and finds **two new issues: one MODERATE (as_int64/as_float64 missing from this mission's scope) and one LOW (Display/AsString specification overlap)**.

---

## Status of Round 1 Issues

| ID | Severity | Issue | Fix Applied | Assessment |
|---|---|---|---|---|
| C1 | HIGH | BIGINT variable-length vs DECIMAL fixed-length not in AC | ✅ Added explicit notes to AC-3/AC-5 | ✅ Correct — distinction is now explicit |
| C2 | HIGH | stoolap_parse_decimal() missing from AC | ✅ Added as explicit AC item with full spec | ✅ Correct — parser spec is complete |
| C3 | MODERATE | cast_to_type traps use unspecified error types | ✅ Split into AC-9a (BIGINT→INTEGER) and AC-9b (DECIMAL→BIGINT) | ✅ Correct — error types specified |
| C4 | MODERATE | DECIMAL→INTEGER returns Error not NULL — contract deviation | ✅ Added to Mission Notes | ✅ Correct — note is present |
| C5 | LOW | as_string() not explicit in AC | ✅ Added explicit AC | ✅ Correct — now separate from Display |
| C6 | LOW | compare_same_type() wildcard arms not explicit | ✅ Added wildcard arm requirement to AC-13 | ✅ Correct — now explicit |

All round 1 fixes are correct and complete.

---

## NEW ISSUES (Round 2)

### C1 · MODERATE: `as_int64()` and `as_float64()` Extension cases belong in 0202-b, not deferred

**Location:** Mission scope, RFC §6.13

**Problem:** RFC-0202-A §6.13 specifies Extension cases for `as_int64()` and `as_float64()`. These are Value layer methods, making them a 0202-b concern (Phase 1b), not a 0202-d concern (Phase 3 VM). However, the cross-type comparison path (§6.12) that would exercise these methods is Phase 3 — during Phase 1-2, `as_float64()` is intercepted by the panic hazard in `Value::compare()`.

The gap: 0202-b (Phase 1b) implements the Value constructors, extractors, coercion, and casting. The `as_int64()` extension for BIGINT and `as_float64()` extension for DECIMAL are Value methods that belong in Phase 1b scope. The mission AC does not include them.

Specifically per RFC §6.13:
- `as_int64()` for BIGINT: `BigInt::try_from(self).ok()` — returns `None` for out-of-range values
- `as_float64()` for DECIMAL: `mantissa as f64 / 10f64.powi(scale as i32)` — precision loss for |mantissa| > 2^53

These are small additions but necessary for a complete Value layer.

**Required fix:** Add to Acceptance Criteria:

> - [ ] `as_int64()` updated for BIGINT Extension per RFC §6.13: `BigInt::try_from(&bi).ok()` — returns `None` for BIGINT values exceeding i64 range
> - [ ] `as_float64()` updated for DECIMAL Extension per RFC §6.13: `mantissa as f64 / 10f64.powi(scale as i32)` — note: precision loss for |mantissa| > 2^53

---

### C2 · LOW: Display and as_string overlap creates ambiguous test coverage

**Location:** AC-11 (Display), AC-10 (as_string)

**Observation:** The round 1 review added AC-10 for `as_string()` and AC-11 for Display. Both use the same extractors and similar logic:

```
as_string:
  BIGINT: as_bigint().map(|bi| bi.to_string())
  DECIMAL: as_decimal().and_then(|d| decimal_to_string(&d).ok())

Display:
  BIGINT: as_bigint().to_string()
  DECIMAL: decimal_to_string()
```

For BIGINT, `as_string()` and `Display` produce identical output. For DECIMAL, `as_string()` uses `.ok()` on `decimal_to_string()` while Display uses the function directly. The difference: `as_string()` returns `Option<String>`, `Display` returns `Result` (via `write!`).

The AC-9a (cast_to_type BIGINT→INTEGER) uses `i64::try_from(&BigInt)` which implies `as_int64()` is needed. This is consistent with C1 above.

**No action required.** The overlap is acceptable and the distinction (Option vs Result) is correct. Documented for completeness.

---

### C3 · LOW: `Value::null()` pattern for BIGINT/DECIMAL not mentioned

**Location:** Mission scope, RFC §6.5

**Observation:** RFC-0202-A §6.5 specifies NULL handling for BIGINT/DECIMAL Extension values. The mission implements constructors and extractors but does not mention `Value::Null(DataType::Bigint)` and `Value::Null(DataType::Decimal)`.

These follow the existing NULL pattern and don't require special constructors — `Value::Null(DataType::Bigint)` is the typed NULL for BIGINT columns. The mission is correct to not add special constructors, but a note in Mission Notes would prevent confusion.

**Recommended fix:** Add to Mission Notes:

> **NULL values:** `Value::Null(DataType::Bigint)` and `Value::Null(DataType::Decimal)` follow existing typed NULL patterns. No special constructors needed — `Value::Null(dt)` where `dt` is the column's DataType. The extractors `as_bigint()` and `as_decimal()` return `None` for NULL values.

---

## Summary Table (Round 2)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | MODERATE | as_int64/as_float64 Extension cases belong in 0202-b scope | Add explicit AC for both conversion methods |
| C2 | LOW | Display/as_string overlap (informational) | No action required |
| C3 | LOW | Typed NULL pattern not documented | Add note to Mission Notes |

---

## Recommendation

Mission 0202-b is **ready to start** after resolving **C1** (add as_int64/as_float64 to AC). The round 1 fixes are all correct and complete. C1 is a small but necessary addition to ensure the Value layer is complete — these methods are part of the Value interface that 0202-d's VM will rely on.

C2 and C3 are informational or low-effort additions that don't block implementation.
