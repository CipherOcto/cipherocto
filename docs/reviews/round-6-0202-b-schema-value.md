# Round 6 Adversarial Review: Mission 0202-b (Schema and Value Layer)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-b-bigint-decimal-schema-value.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 6

---

## Status of Prior Issues (Round 5)

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| A9 | MODERATE | Wildcard arm `Ordering::Greater` bias in `compare_same_type()` | **NOT FIXED** — AC-11 not implemented |
| A10 | MODERATE | Precision threshold not inline in AC-10 | **NOT FIXED** — AC-10 not implemented |
| A11 | LOW | No explicit aggregate scope note | **NOT FIXED** |
| A12 | LOW | Title missing RFC context | **NOT FIXED** |

---

## ACCEPTED ISSUES

### A13 · HIGH: All 16 AC items entirely unimplemented — mission non-startable

**Severity:** HIGH
**Section:** All acceptance criteria

**Problem:** Implementation codebase contains **zero** BIGINT/DECIMAL support:
- `DataType` enum ends at `Blob = 10` — no Bigint/Decimal entries
- `SchemaColumn` struct has no `decimal_scale` field
- `Value` enum has no `bigint()` constructor, `decimal()` constructor, `as_bigint()` extractor, or `as_decimal()` extractor
- No `stoolap_parse_decimal` function exists
- No `compare_same_type` BIGINT/DECIMAL handling

Since no code exists, A9 and A10 cannot be verified — both were predicated on AC-11 and AC-10 existing in implemented code.

**Required action:** Implementation must commence. Dependency 0202-a must add `DataType::Bigint = 13` and `DataType::Decimal = 14` first.

---

### A14 · MODERATE: A9 (wildcard arm bias) still unresolved — carried from Round 5

**Severity:** MODERATE
**Status:** Carried from Round 5 (A9)

**Problem:** AC-11 specifies that for both BIGINT and DECIMAL, invalid compare results fall through to a wildcard arm that returns `Ordering::Greater`. This systematically biases total ordering upward.

**Required fix (repeat):** Add justification or change to `Ordering::Less`:
```
n => { debug_assert!(false, "invalid compare result: {}", n); Ordering::Less }
```

---

### A15 · MODERATE: A10 (precision threshold) still unresolved — carried from Round 5

**Severity:** MODERATE
**Status:** Carried from Round 5 (A10)

**Problem:** AC-10 text does not inline the precision threshold `|mantissa| > 2^53`. The Note mentions it but the AC checkbox itself does not.

**Required fix (repeat):** Update AC-10 text to:
```
- [ ] `as_float64()` updated for DECIMAL Extension per RFC §6.13: `mantissa as f64 / 10f64.powi(scale as i32)` — precision loss occurs when |mantissa| > 2^53 (f64 mantissa width); BIGINT→f64 not provided (values may exceed f64 range)
```

---

### A16 · LOW: A11 (aggregate scope) still unresolved — carried from Round 5

**Severity:** LOW

**Problem:** No explicit aggregate operations scope note added.

**Required fix (repeat):** Add to the AC block header or Reference:
```
- Aggregate operations (COUNT, SUM, MIN, MAX, AVG) — out of scope; deferred to mission 0202-d
```

---

### A17 · LOW: A12 (title RFC context) still unresolved — carried from Round 5

**Severity:** LOW

**Problem:** Title still reads "Phase 1b — SchemaColumn and Value Layer" with no RFC-0202-A indication.

**Required fix (repeat):** Change title to: "RFC-0202-A Phase 1b — SchemaColumn and Value Layer (BIGINT/DECIMAL)"

---

### A18 · LOW: AC-13 (as_int64 for BIGINT) references non-existent `DataType::Bigint`

**Severity:** LOW

**Problem:** AC-3 says BIGINT uses tag 13. But `DataType::Bigint` does not exist in the current enum. 0202-a must add it first.

**Required fix:** Ensure 0202-a completes before 0202-b implementation.

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| A13 | HIGH | No AC items implemented — mission non-startable | Implementation must begin |
| A14 | MODERATE | Wildcard arm bias (A9, carry) | Fix in code during implementation |
| A15 | MODERATE | Precision threshold not inline (A10, carry) | Fix AC text + implementation |
| A16 | LOW | Aggregate scope note missing (A11, carry) | Add scope note to mission |
| A17 | LOW | Title missing RFC context (A12, carry) | Prepend RFC-0202-A to title |
| A18 | LOW | Tag 13/14 reference premature | Ensure 0202-a adds Bigint/Decimal to DataType first |

---

## Verdict

**Not ready to start.** All four issues from round-5 (A9, A10, A11, A12) remain unresolved and cannot be verified because implementation has not begun. A13 is a new HIGH issue arising from the complete absence of implementation. A16-A17 are text fixes that can be applied immediately to the mission document regardless of implementation status.

**Before round-7:**
1. A16, A17 can be fixed in mission document immediately
2. Implementation depends on 0202-a completing first
3. A14, A15 will be verified during implementation