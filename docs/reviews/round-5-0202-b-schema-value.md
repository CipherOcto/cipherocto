# Round 5 Adversarial Review: Mission 0202-b (Schema and Value Layer)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-b-bigint-decimal-schema-value.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 5

---

## Status of Prior Issues (Round 4)

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| A1 | HIGH | Reference missing §6.10 | **FIXED** (commit 0fcb164) |
| A2 | HIGH | Reference missing §6.12 | **FIXED** (commit 0fcb164) |
| A3 | HIGH | DECIMAL→INTEGER coercion not in AC | **FIXED** (commit 0fcb164) |
| A4 | MODERATE | AC-1/AC-2 not independently verifiable | **FIXED** (split in commit 0fcb164) |
| A5 | MODERATE | as_int64() imprecise notation | **FIXED** (commit 0fcb164) |
| A6 | MODERATE | RFC-0110 §8 gas discrepancy | **STILL OPEN** — pre-existing RFC bug |
| A7 | LOW | §6.11 description omits lexicographic encoding | **FIXED** (commit 0fcb164) |
| A8 | LOW | No aggregate gas AC items | **STILL OPEN** — scope implicit but not explicit |

---

## ACCEPTED ISSUES

### A9 · MODERATE: AC-11 wildcard arm returning `Ordering::Greater` for invalid compare result is unjustified

**Severity:** MODERATE
**Section:** AC-11 (`compare_same_type()` — BIGINT/DECIMAL comparison)

**Problem:** AC-11 specifies that for both BIGINT and DECIMAL, invalid compare results (values not -1/0/1) should fall through to a wildcard arm that returns `Ordering::Greater`. This systematically biases total ordering upward. If `BigInt::compare()` can return unexpected values (which the `debug_assert!(false, ...)` acknowledges as invalid), returning `Greater` for those cases means invalid comparisons always compare as "larger" — which could cause subtle correctness issues in sorted outputs.

The wildcard arm should either:
1. Return `Ordering::Less` to be a neutral fallback (neither direction bias)
2. Include a comment explaining why `Greater` is the correct bias for BigInt/DECIMAL

**Required fix:** Add justification or change to `Ordering::Less`:
```
n => { debug_assert!(false, "invalid compare result: {}", n); Ordering::Less }
```

---

### A10 · MODERATE: AC-10 `as_float64()` precision threshold not explicit in AC text

**Severity:** MODERATE
**Section:** AC-10 (`as_float64()` for DECIMAL Extension)

**Problem:** AC-10 says "precision loss for |mantissa| > 2^53 is expected" — but the threshold `|mantissa| > 2^53` is only mentioned in the Note, not as part of the AC specification itself. An implementer reading the AC checkbox might miss this detail and write incorrect conversion code.

**Required fix:** Inline the precision threshold into the AC item:
```
- [ ] `as_float64()` updated for DECIMAL Extension per RFC §6.13: `mantissa as f64 / 10f64.powi(scale as i32)` — precision loss occurs when |mantissa| > 2^53 (f64 mantissa width); BIGINT→f64 not provided (values may exceed f64 range)
```

---

### A11 · LOW: No explicit aggregate operations scope note

**Severity:** LOW
**Section:** Mission AC (absent), Reference

**Problem:** A8 (round 4) noted no explicit scope note for aggregate operations. The round 4 fix did not add such a note. A reader must infer from the Reference section (which points to 0202-d for aggregate operations) that aggregates are out of scope.

**Required fix:** Add to the AC block header or Reference:
```
- Aggregate operations (COUNT, SUM, MIN, MAX, AVG) — out of scope; deferred to mission 0202-d
```

---

### A12 · LOW: Mission title missing RFC context

**Severity:** LOW
**Section:** Mission title (line 1)

**Problem:** Title reads "Phase 1b — SchemaColumn and Value Layer" with no indication this is about BIGINT/DECIMAL core types (RFC-0202-A scope). Browsing missions by title alone does not identify this as the RFC-0202-A schema/value mission.

**Required fix:** Change title to: "RFC-0202-A Phase 1b — SchemaColumn and Value Layer (BIGINT/DECIMAL)"

---

## QUESTIONS

**Q1:** A6 (RFC-0110 §8 gas discrepancy: formula gives 12,338, table shows 12,362) remains open as a pre-existing RFC bug. Should 0202-d implement the formula value (12,338) as the normative gas, or the table value (12,362)? The mission cannot resolve this — it needs an RFC erratum.

**Q2:** `stoolap_parse_decimal` AC (AC-7) specifies the parser but has no corresponding test AC in 0202-e. Is parser verification intentionally deferred to integration testing, or should 0202-e include parser tests?

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| A9 | MODERATE | Wildcard arm `Ordering::Greater` unjustified bias | Justify or change to `Ordering::Less` |
| A10 | MODERATE | Precision threshold not inline in AC-10 | Inline `|mantissa| > 2^53` into AC text |
| A11 | LOW | No explicit aggregate scope note | Add "out of scope — deferred to 0202-d" note |
| A12 | LOW | Title missing RFC context | Prepend "RFC-0202-A" to title |

---

## Verdict

**Conditionally ready to start.** All round-4 HIGH issues are resolved. A9 and A10 are MODERATE issues requiring fixes before implementation. A11 and A12 are LOW. A6 (RFC gas discrepancy) remains open but is correctly classified as a pre-existing RFC bug, not a mission defect.