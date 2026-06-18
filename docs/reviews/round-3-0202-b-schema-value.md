# Adversarial Review: Mission 0202-b-bigint-decimal-schema-value (Round 3)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-b-bigint-decimal-schema-value.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 3

---

## Executive Summary

Round 2 review identified 3 issues (C1 MODERATE, C2/C3 LOW). Round 2 fixes were applied. This Round 3 review verifies the fixes and finds **three issues: one MODERATE (as_int64/as_float64 are correctly placed but the AC item numbering is inconsistent with 0202-a's removal), one LOW (Reference section incomplete), and one informational (partial redundancy with cast_to_type)**.

---

## Status of Round 2 Issues

| ID | Severity | Issue | Fix Applied | Assessment |
|---|---|---|---|---|
| C1 | MODERATE | as_int64/as_float64 Extension cases belong in 0202-b | ✅ Added to AC | ✅ Correct — these are value.rs methods |
| C2 | LOW | Display/as_string overlap | Informational | N/A |
| C3 | LOW | Typed NULL pattern not documented | ✅ Added to Mission Notes | ✅ Correct |

All round 2 fixes are correct.

---

## NEW ISSUES (Round 3)

### C1 · MODERATE: `as_int64()` AC is redundant with `cast_to_type()` BIGINT→INTEGER

**Location:** AC (as_int64), AC-9a (cast_to_type BIGINT→INTEGER)

**Problem:** AC-9a specifies that `cast_to_type()` for BIGINT→INTEGER uses `i64::try_from(&BigInt)`. This is semantically identical to what `as_int64()` does for BIGINT: `BigInt::try_from(&bi).ok()`. The round 2 review added `as_int64()` as a separate AC item, but its behavior overlaps with the BIGINT→INTEGER cast path.

The distinction: `as_int64()` is a query method that extracts an i64 from a BIGINT Value without changing the Value type. `cast_to_type()` is an explicit type conversion operation that returns a new Value (possibly of a different type). However, for BIGINT, the extraction (as_int64 → Some/None) and the cast (cast_to_type → Value::Integer or error) have the same i64 semantics.

The RFC §6.13 specifies `as_int64()` as a separate method with the same i64::try_from logic. So it IS needed — but the redundancy should be noted.

**No action required.** The RFC explicitly specifies both as separate methods. The redundancy is intentional. Documented for completeness.

---

### C2 · LOW: Reference section missing §6.13 and §6.3

**Location:** Reference section, RFC §6.3, §6.13

**Problem:** The Reference section lists §2, §6.4–§6.9 but is missing:
- §6.3 (Display implementation — BIGINT/DECIMAL Display is per RFC §6.3)
- §6.13 (as_int64/as_float64 — added in round 2)

**Recommended fix:** Update Reference section:

```
## Reference

- RFC-0202-A §2 (Value constructors/extractors — BigIntEncoding wire format)
- RFC-0202-A §6.3 (Display update — Value::Display for BIGINT/DECIMAL)
- RFC-0202-A §6.4 (as_string update)
- RFC-0202-A §6.5 (NULL handling)
- RFC-0202-A §6.6 (compare_same_type — includes wildcard arm requirement)
- RFC-0202-A §6.7 (type coercion hierarchy — INTEGER→DECIMAL shortcut, DECIMAL→INTEGER via BIGINT)
- RFC-0202-A §6.8 (from_typed update — Result semantics)
- RFC-0202-A §6.8a (stoolap_parse_decimal — standalone parser function)
- RFC-0202-A §6.9 (SchemaColumn extension — decimal_scale: Option<u8>)
- RFC-0202-A §6.13 (as_int64/as_float64 Extension methods)
```

---

### C3 · LOW: Missing `Ord` implementation update from AC

**Location:** Not in AC, RFC §6.11

**Observation:** RFC-0202-A §6.11 specifies an `Ord for Value` implementation update for BIGINT and DECIMAL:

> "The existing `Ord for Value` implementation compares Extension types via raw byte comparison... Fix: Dispatch BIGINT/DECIMAL Extension types to numeric comparison"

The mission AC covers `compare_same_type()` (§6.6) but does not mention the `Ord` implementation update (§6.11). The `Ord` implementation is distinct from `compare_same_type()` — `Ord` cannot return errors, so it falls back to byte comparison for unknown extension types. The numeric dispatch for BIGINT/DECIMAL in `Ord` uses the same `as_bigint()`/`as_decimal()` extractors and `BigInt::compare`/`decimal_cmp` as `compare_same_type()`, but without the error return.

**Recommended fix:** Add to Acceptance Criteria:

> - [ ] `Ord for Value` updated for BIGINT/DECIMAL per RFC §6.11:
>   - BIGINT: deserialize both values, use `BigInt::compare()` for ordering
>   - DECIMAL: deserialize both values, use `decimal_cmp()` for ordering
>   - If deserialization fails (corrupt data), fall back to byte comparison with debug assertion
>   - **Note:** `Ord` cannot return errors — unlike `compare_same_type()`, it must provide a total ordering. Corrupt BIGINT/DECIMAL data falls back to byte comparison.

---

## Summary Table (Round 3)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | MODERATE | as_int64 redundant with cast_to_type BIGINT→INTEGER | No action — RFC §6.13 specifies both intentionally |
| C2 | LOW | Reference section missing §6.3 and §6.13 | Update Reference section |
| C3 | LOW | Ord implementation missing from AC | Add Ord for Value AC for BIGINT/DECIMAL |

---

## Recommendation

Mission 0202-b is **ready to start** after resolving **C2** (update Reference section) and **C3** (add Ord implementation to AC). The round 2 fixes are all correct. C1 is informational — the redundancy between as_int64() and cast_to_type() BIGINT→INTEGER is intentional per RFC §6.13.

C3 is a missing specification item — the `Ord` implementation for BIGINT/DECIMAL is required for BTree index operations to work correctly with lexicographic encoding.
