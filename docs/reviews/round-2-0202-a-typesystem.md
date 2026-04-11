# Adversarial Review: Mission 0202-a-bigint-decimal-typesystem (Round 2)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-a-bigint-decimal-typesystem.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 2

---

## Executive Summary

Round 1 review identified 4 issues (C1 HIGH, C2 MODERATE, C3 LOW, C4 informational). Round 1 fixes (latent panic warning, AC-3 split, unit tests) were applied and committed. This Round 2 review verifies the fixes are adequate and complete, and finds **two new issues: one MODERATE specification gap (as_int64/as_float64 extension) and one LOW (test completeness)**.

---

## Status of Round 1 Issues

| ID | Severity | Issue | Fix Applied | Assessment |
|---|---|---|---|---|
| C1 | HIGH | Adding BigInt/Decimal to is_numeric() creates latent as_float64() panic | ✅ Added to Mission Notes | ✅ Correct — note is present and accurate |
| C2 | MODERATE | from_str_versioned() is persistence-layer, not types.rs | ✅ Split AC-3 into 3a/3b | ✅ Correct — 3a (constant) and 3b (persistence layer) are separate |
| C3 | LOW | Acceptance criteria omit unit tests | ✅ Added AC-9 with specific tests | ✅ Correct — test coverage is explicit |
| C4 | — | Decimal Display limitation | 📝 Informational | N/A |

---

## NEW ISSUES (Round 2)

### C1 · MODERATE: `as_int64()` and `as_float64()` Extension cases missing from Phase 1 scope

**Location:** Mission scope, RFC §6.13

**Problem:** RFC-0202-A §6.13 specifies that BIGINT and DECIMAL Extension values should have special handling in `as_int64()` and `as_float64()`:

```rust
// In as_int64():
Value::Extension(data) if data.first() == Some(&(DataType::Bigint as u8)) => {
    self.as_bigint().and_then(|bi| i64::try_from(bi).ok())
}

// In as_float64():
Value::Extension(data) if data.first() == Some(&(DataType::Decimal as u8)) => {
    self.as_decimal().and_then(|d| {
        let mantissa = d.mantissa() as f64;
        let scale = d.scale();
        Some(mantissa / 10f64.powi(scale as i32))
    })
}
```

These are Phase 1 implementation items per RFC §6.13 ("Phase 1 scope") and are needed by the cross-type comparison path (§6.12). However:
- The cross-type comparison path (§6.12) is Phase 3 (0202-d) — the panic hazard means these methods are intercepted before being reached
- `as_int64()` for BIGINT (i64::try_from) and `as_float64()` for DECIMAL are partial conversions that lose precision
- The RFC explicitly notes "BIGINT→f64 conversion is NOT provided because BigInt values may exceed f64 precision"

The issue: these extension cases are specified in RFC §6.13 as Phase 1 scope but are NOT in the mission AC. Phase 1b (0202-b) adds the Value constructors/extractors but also does not include these conversion cases. The consequence: during Phase 1-2, `as_int64()` on a BIGINT value returns `None`, even for values within i64 range. This is a latent gap.

**Required fix:** Add to Acceptance Criteria or confirm deferred:

> - [ ] `as_int64()` updated for BIGINT Extension: `BigInt::try_from(self).ok()` (i64::try_from for values within i64 range); returns `None` for out-of-range BIGINT values
> - [ ] `as_float64()` updated for DECIMAL Extension: `mantissa as f64 / 10f64.powi(scale as i32)` (per RFC §6.13 — loss of precision for |mantissa| > 2^53)
>
> OR: explicitly defer to Phase 3 (0202-d) — note that these are Phase 1 scope per RFC §6.13 but are blocked by the cross-type comparison panic hazard, so no code currently reaches them during Phase 1-2.

---

### C2 · LOW: AC-9 unit test coverage is correct but omits `from_str_versioned()` test

**Location:** AC-9 (unit tests), RFC §6.1, §4a

**Observation:** AC-9 specifies unit tests for `is_numeric`, `is_orderable`, `u8_conversion`, `from_str`, and `display`. However, AC-3b (added in round 1) adds `from_str_versioned()` in the persistence layer — there is no test criterion for this function.

The `from_str_versioned()` function has two paths (spec_version < 2 and spec_version ≥ 2) and is a critical migration gate. Its behavior must be tested:
- spec_version 1: BIGINT → Integer, DECIMAL → Float
- spec_version 2+: BIGINT → Bigint, DECIMAL → Decimal

**Recommended fix:** Add to AC-9:

> - [ ] Unit tests for `from_str_versioned()` in persistence layer:
>   - `from_str_versioned("BIGINT", 1)` → `Ok(DataType::Integer)`
>   - `from_str_versioned("DECIMAL", 1)` → `Ok(DataType::Float)`
>   - `from_str_versioned("BIGINT", 2)` → `Ok(DataType::Bigint)`
>   - `from_str_versioned("DECIMAL", 2)` → `Ok(DataType::Decimal)`
>   - `from_str_versioned("DECIMAL(10,2)", 1)` → `Ok(DataType::Float)` (legacy parameterized form)
>   - `from_str_versioned("DECIMAL(10,2)", 2)` → `Ok(DataType::Decimal)`

---

### C3 · LOW: `test_datatype_from_str` specification is slightly ambiguous

**Location:** AC-9 (unit tests)

**Observation:** AC-9 says `test_datatype_from_str` should test `"DECIMAL(10,2)".parse()` and `"NUMERIC".parse()` → Decimal. However, this tests the `FromStr` impl, not `from_str_versioned`. The mission correctly splits these — `FromStr` always returns the new types (Bigint/Decimal), while `from_str_versioned` is version-gated.

The test description in AC-9 is fine but could be clearer: `"DECIMAL(10,2)".parse()` → `DataType::Decimal` tests the `FromStr` impl, which always returns `Decimal` for DECIMAL/NUMERIC keywords regardless of version. This is the correct behavior for parsing new DDL statements.

**No action required.** This is already correctly implemented in the AC. Documented here for completeness.

---

## Summary Table (Round 2)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | MODERATE | as_int64/as_float64 Extension cases missing from Phase 1 scope | Add explicit AC for as_int64 (BIGINT) and as_float64 (DECIMAL), or explicitly defer |
| C2 | LOW | from_str_versioned() unit tests missing from AC-9 | Add version-gated tests for from_str_versioned() |

---

## Recommendation

Mission 0202-a is **conditionally ready** after resolving **C1** (add as_int64/as_float64 extension AC or explicitly defer). The round 1 fixes are all correct and complete. C1 is a specification gap between RFC §6.13 and the mission scope — the cross-type comparison panic means these methods are currently unreachable during Phase 1-2, but they should still be documented as Phase 1 scope or explicitly deferred.

C2 is a straightforward test completeness item that can be addressed in PR review.
