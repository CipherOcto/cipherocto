# Adversarial Review: Mission 0202-a-bigint-decimal-typesystem (Round 3)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-a-bigint-decimal-typesystem.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 3

---

## Executive Summary

Round 2 review identified 2 issues (C1 MODERATE, C2 LOW). Round 2 fixes were applied. This Round 3 review verifies the fixes and finds **two issues: one MODERATE (as_int64/as_float64 belong in 0202-b, not 0202-a) and one LOW (Reference section missing §6.4–§6.9)**.

---

## Status of Round 2 Issues

| ID | Severity | Issue | Fix Applied | Assessment |
|---|---|---|---|---|
| C1 | MODERATE | as_int64/as_float64 Extension cases missing from Phase 1 scope | ✅ Added to AC-10, AC-11 | ❌ Wrong location — these are Value methods, not Type methods |
| C2 | LOW | from_str_versioned() unit tests missing from AC-9 | ✅ Added to AC-9 | ✅ Correct |

---

## NEW ISSUES (Round 3)

### C1 · MODERATE: `as_int64()` and `as_float64()` belong in 0202-b, not 0202-a

**Location:** AC-10 and AC-11, RFC §6.13, Key Files table

**Problem:** Round 2 review added `as_int64()` and `as_float64()` to 0202-a's AC, citing RFC §6.13 as justification. However, RFC §6.13 explicitly places these in `src/core/value.rs`, not `src/core/types.rs`:

```
// In as_int64(): ... in src/core/value.rs
// In as_float64(): ... in src/core/value.rs
```

The Key Files table (§13) confirms:
> `src/core/value.rs`: Add `Value::bigint()`, `Value::decimal()`, extractors, `from_typed()`, `coerce_to_type()`, `cast_to_type()`, `Display`, `as_string()`, `as_int64()`, `as_float64()`, `compare_same_type()`

Phase 1 is a type system mission (`types.rs`). Phase 1b (0202-b) is the Value layer mission (`value.rs`). These methods are Value accessor methods, not Type system methods. They were correctly identified as 0202-b scope in round 2 review of 0202-b, but round 2 review of 0202-a incorrectly added them to 0202-a's AC.

The result: as_int64/as_float64 appear in BOTH 0202-a AND 0202-b ACs, creating confusion about which mission owns the implementation.

**Required fix:** Remove AC-10 and AC-11 from 0202-a. These belong in 0202-b (Phase 1b), which already has them correctly specified.

---

### C2 · LOW: Reference section is missing sections covered in AC

**Location:** Reference section, RFC §6.4–§6.9

**Problem:** The Reference section lists only §1, §6.1, §6.2, §6.3, and §4a. However, the AC covers additional RFC sections:
- §6.4 (as_string) — covered in 0202-b, but as_string is also needed for Display
- §6.5 (NULL handling) — covered in 0202-b Mission Notes
- §6.6 (compare_same_type) — covered in 0202-b AC-13
- §6.7 (coercion hierarchy) — covered in 0202-b AC-8, AC-9
- §6.8 (from_typed) — covered in 0202-b AC-7
- §6.8a (stoolap_parse_decimal) — covered in 0202-b AC-6
- §6.9 (SchemaColumn extension) — covered in 0202-b AC-1, AC-2
- §6.13 (as_int64/as_float64) — covered in 0202-b

While 0202-a is Phase 1 (type system) and 0202-b covers the Value layer, the Reference section should accurately reflect which RFC sections are relevant to the combined Phase 1 + Phase 1b scope.

**Recommended fix:** Update Reference section:

```
## Reference

- RFC-0202-A §1 (DataType discriminants)
- RFC-0202-A §4a (NUMERIC_SPEC_VERSION migration gate)
- RFC-0202-A §6.1 (FromStr update)
- RFC-0202-A §6.2 (Display update)
- RFC-0202-A §6.3 (is_numeric, is_orderable, from_u8)
- RFC-0202-A §6.4–§6.9 (Value layer extensions — implemented in mission 0202-b)
- RFC-0202-A §6.13 (as_int64/as_float64 — implemented in mission 0202-b)
```

---

## Summary Table (Round 3)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | MODERATE | as_int64/as_float64 belong in 0202-b, not 0202-a | Remove AC-10 and AC-11 from 0202-a |
| C2 | LOW | Reference section missing §6.4–§6.9, §6.13 | Update Reference section |

---

## Recommendation

Mission 0202-a is **ready to start** after resolving **C1** (remove as_int64/as_float64 from AC — these are 0202-b scope). The round 2 C2 fix (from_str_versioned tests) is correct. C2 is a documentation improvement that doesn't block implementation.

C1 is a cross-mission coordination issue introduced by round 2 review — the fix was placed in the wrong mission.
