# Adversarial Review: Mission 0202-e-bigint-decimal-integration-testing (Round 3)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-e-bigint-decimal-integration-testing.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 3

---

## Executive Summary

Round 1 and round 2 reviews identified 9 issues total. Both rounds' fixes were applied to the mission. This Round 3 review verifies fixes and finds **two issues: one MODERATE (aggregate operation tests missing from AC) and one LOW (BTree mixed-sign ordering test vectors incomplete)**.

---

## Status of Prior Issues

| Round | Issue | Status |
|-------|-------|--------|
| R1-C1 | Cross-type comparison panic hazard | ✅ Fixed — execution timing note added |
| R1-C2 | Test vector coverage unspecified | ✅ Fixed — Merkle root verification added |
| R1-C3 | Gas benchmarking underspecified | ✅ Fixed — specific limb counts and scales added |
| R1-C4 | DECIMAL sqrt vectors not referenced | ✅ Fixed — explicit SQRT vectors added |
| R1-C5 | BTree ordering not verified | ✅ Fixed — ordering verification added |
| R2-C2-R2 | Wire format test vectors not referenced | ✅ Fixed — byte-level vectors added |
| R2-C3-R2 | as_int64/as_float64 round-trip not tested | ✅ Fixed — conversion tests added |
| R2-C4-R2 | Division by zero test missing | ✅ Fixed — explicit div-by-zero tests added |
| R2-C5-R2 | Canonical zero test underspecified | ✅ Fixed — wire format verification added |

All prior fixes are correctly applied.

---

## NEW ISSUES (Round 3)

### C1 · MODERATE: Aggregate operation tests are not explicitly in the AC

**Location:** AC (aggregate tests), RFC §7a

**Problem:** AC-1 and AC-2 test arithmetic operations via RFC-0110/RFC-0111 test vectors, but aggregate operations (COUNT, SUM, AVG, MIN, MAX) are not explicitly tested in any AC item. RFC §7a specifies aggregate behavior including overflow semantics, result types, and scale computation for AVG.

The mission correctly tests division by zero (AC-12) and type conversions (AC-13), but aggregate operations — which have distinct semantics from element-wise arithmetic — are not covered.

**Required fix:** Add explicit aggregate operation tests to AC:

> - [ ] Aggregate operation tests for BIGINT:
>   - `COUNT(BIGINT col)` on NULL-only column → `0` (COUNT never returns NULL for empty sets)
>   - `SUM(BIGINT col)` on NULL-only column → NULL
>   - `MIN/MAX(BIGINT col)` on NULL-only column → NULL
>   - `SUM` overflow: `SUM` of values exceeding ±(2^4096 − 1) → `BigIntError::OutOfRange`
>   - `AVG(BIGINT col)` → `Error::NotSupported("AVG on BIGINT requires RFC-0202-B")`
> - [ ] Aggregate operation tests for DECIMAL:
>   - `COUNT(DECIMAL col)` on NULL-only column → `0`
>   - `SUM(DECIMAL col)` on NULL-only column → NULL
>   - `MIN/MAX(DECIMAL col)` on NULL-only column → NULL
>   - `SUM` overflow: `SUM` of values exceeding ±(10^36 − 1) → `DecimalError::Overflow`
>   - `AVG(DECIMAL '1.000000')` → result scale ≥ 6 (input_scale + 6 capped at 36)
> - [ ] Aggregate operation tests for mixed NULL/data columns:
>   - Verify NULLs are excluded from SUM/AVG/MIN/MAX but counted by COUNT
>   - Verify NULL sorts as lowest in MIN/MAX

---

### C2 · LOW: BTree lexicographic ordering test vectors don't include all RFC §6.11 mixed-sign cases

**Location:** AC-10 (BTree range scan), RFC §6.11

**Problem:** AC-10 verifies `BIGINT '-100' < BIGINT '0' < BIGINT '100'` and `DECIMAL '-12.3' < DECIMAL '0' < DECIMAL '12.3'`. These are single-sign comparisons (all negative < zero < all positive).

RFC §6.11 specifies mixed-sign ordering:
> "Within negative values: smaller magnitude (fewer limbs) is more negative. Within positive values: larger magnitude (more limbs) is larger."

The mission's test vectors only verify cross-sign ordering (negative < zero < positive). They don't verify within-sign ordering for values with different limb counts.

**Required fix:** Expand AC-10 lexicographic ordering verification:

> - [ ] BTree index range scan tests with lexicographic ordering verification:
>   - Cross-sign: `BIGINT '-100' < BIGINT '0' < BIGINT '100'` ✅
>   - Cross-sign: `DECIMAL '-12.3' < DECIMAL '0' < DECIMAL '12.3'` ✅
>   - Within-negative ordering (per RFC §6.11: more limbs = more negative): `BIGINT '-2^64' < BIGINT '-1'` (2 limbs vs 1 limb, both negative)
>   - Within-positive ordering (per RFC §6.11: more limbs = larger): `BIGINT '2^64' > BIGINT '1'` (2 limbs vs 1 limb, both positive)
>   - Zero vs positive: `BIGINT '0' < BIGINT '1'` — byte comparison confirms zero's all-zero limb array sorts before non-zero limbs
>   - DECIMAL within-negative: verify different negative mantissas sort correctly after sign-flip transformation

---

## Summary Table (Round 3)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | MODERATE | Aggregate operation tests not in AC | Add explicit aggregate tests (COUNT, SUM, MIN, MAX, AVG) |
| C2 | LOW | BTree ordering test vectors incomplete | Add within-sign ordering tests (multi-limb BIGINT, mixed-mantissa DECIMAL) |

---

## Recommendation

Mission 0202-e is **ready to start** after resolving **C1** (add aggregate operation tests). C2 is a completeness improvement for the lexicographic ordering verification.

All round 1 and round 2 fixes are correctly applied. The mission scope is comprehensive — C1 adds the missing aggregate test coverage that rounds 1 and 2 reviews identified as implicit but not explicit in the AC.
