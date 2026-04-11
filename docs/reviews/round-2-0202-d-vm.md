# Adversarial Review: Mission 0202-d-bigint-decimal-vm (Round 2)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-d-bigint-decimal-vm.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 2

---

## Executive Summary

Round 1 review identified 6 issues (C1 HIGH, C2/C3/C4 MODERATE, C5/C6 LOW). The review was committed but **the mission file was NOT updated with fixes** — all 6 round 1 issues remain unresolved. This Round 2 review:

1. Confirms all round 1 issues remain open
2. Finds **two new issues: one MODERATE (division by zero handling not specified) and one LOW (aggregate error mapping unclear)**
3. Provides complete fix language for all items

---

## Status of Round 1 Issues (ALL UNRESOLVED)

**Mission file `missions/open/0202-d-bigint-decimal-vm.md` was NOT updated after round 1 review.** All 6 issues from round 1 remain open.

| ID | Severity | Issue | Status |
|---|---|---|---|
| C1 | HIGH | BIGINT SQRT in AC but not in RFC §7 | ❌ Still present — SQRT still listed in AC |
| C2 | MODERATE | Aggregate operations missing from AC | ❌ Still missing |
| C3 | MODERATE | Gas metering formulas not explicit | ❌ Still vague |
| C4 | MODERATE | Optimizer cost estimates vague | ❌ Still vague |
| C5 | LOW | Pre-flight bounds checks missing | ❌ Still missing |
| C6 | LOW | decimal_div placeholder param not noted | ❌ Still missing |

---

## NEW ISSUES (Round 2)

### C3-R2 · MODERATE: Division by zero handling is unspecified for both BIGINT and DECIMAL

**Location:** AC-1 (DIV operation), AC-2 (DIV operation), RFC §7

**Problem:** RFC-0202-A §7 specifies `bigint_div(a: BigInt, b: BigInt)` and `decimal_div(a: &Decimal, b: &Decimal, _target_scale: u8)` but does not explicitly call out division-by-zero behavior in the VM dispatch section. However, RFC §Security Considerations (§12) explicitly states:

> "Division by zero MUST return error"

And RFC-0110/RFC-0111 specify that division by zero returns an error. The mission AC does not mention this critical error path. If the VM does not check for zero divisor before calling the division operation, the determin crate's error handling applies — but the VM should handle it explicitly with proper gas accounting (zero-divisor check should consume minimal gas).

**Required fix:** Add to AC-1 and AC-2:

> - [ ] Division by zero check: before executing DIV, verify divisor is non-zero. If divisor is zero, return `Error::invalid_argument("division by zero")` and consume pre-flight gas only (10 gas), not full operation gas.

---

### C4-R2 · LOW: Aggregate SUM/MIN/MAX overflow error mapping is underspecified

**Location:** AC-2 (aggregate operations — new), RFC §7a

**Problem:** RFC §7a specifies aggregate operations but the error mapping is not explicit in the mission context:
- BIGINT SUM: `BigIntError::OutOfRange` when sum exceeds ±(2^4096 − 1) — the mission must map this to a Stoolap `Error` variant
- DECIMAL SUM: `DecimalError::Overflow` when sum exceeds ±(10^36 − 1)

The mission needs to specify:
1. Which Stoolap `Error` variant wraps `BigIntError::OutOfRange` (likely `Error::OutOfGas` or a new numeric overflow error)
2. Whether partial sums are maintained across rows or recomputed from scratch
3. For streaming aggregation: when the overflow occurs mid-stream, is the error returned immediately or does it truncate/round?

**Recommended fix:** Add to Mission Notes:

> **Aggregate overflow handling:** When SUM overflows mid-stream (streaming aggregation), the error is returned immediately for the row that causes overflow. The accumulated sum up to that point is discarded — there is no partial result. This matches the behavior of `BigIntError::OutOfRange` and `DecimalError::Overflow` from the determin crate. The Stoolap `Error` variant for BIGINT SUM overflow should be `Error::OutOfGas` (numeric overflow maps to gas exhaustion semantics).

---

### C5-R2 · LOW: `BITLEN` operation gas is unspecified in AC

**Location:** AC-1 (BIGINT ops), RFC §8

**Observation:** AC-1 lists BITLEN as a BIGINT operation. The gas formula for BITLEN is not in RFC §8 gas tables. RFC-0110 specifies BITLEN as O(n) where n is limb count — the gas should be proportional to limb count. The formula is likely `10 + limbs` (similar to CMP) but this is not confirmed in RFC §8.

**Recommended fix:** Add to Mission Notes or confirm via RFC-0110:

> **BITLEN gas:** RFC-0110 does not specify a gas formula for BITLEN. Until RFC-0110 is amended with a BITLEN gas formula, use `10 + limbs` as a conservative estimate (same as CMP). Verify against RFC-0110 reference implementation before production deployment.

---

## Summary Table (Round 2)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 (R1) | HIGH | BIGINT SQRT in AC but not in RFC §7 | ✅ Still open — remove SQRT from BIGINT ops |
| C2 (R1) | MODERATE | Aggregate operations missing from AC | ✅ Still open — add COUNT/SUM/MIN/MAX/AVG |
| C3 (R1) | MODERATE | Gas metering formulas not explicit | ✅ Still open — expand AC-3 with formulas |
| C4 (R1) | MODERATE | Optimizer cost estimates vague | ✅ Still open — expand AC-7 |
| C5 (R1) | LOW | Pre-flight bounds checks missing | ✅ Still open — add SHL/SHR pre-flight |
| C6 (R1) | LOW | decimal_div placeholder param not noted | ✅ Still open — add pass-0 note |
| C3-R2 | MODERATE | Division by zero not specified | Add explicit zero-divisor check before DIV |
| C4-R2 | LOW | Aggregate error mapping underspecified | Add overflow error handling details |
| C5-R2 | LOW | BITLEN gas formula not in RFC §8 | Add conservative estimate note |

---

## Priority

The mission file must be updated with round 1 fixes first. C1 (HIGH — remove BIGINT SQRT) and C2 (MODERATE — add aggregate operations) are the most critical. C3 (division by zero) from round 2 should be addressed together with the gas formulas since they interact.

---

## Recommendation

Mission 0202-d is **not ready to start** — all round 1 issues remain unfixed and three new issues were found. The mission file must be updated with round 1 fixes (C1-C6) plus round 2 fixes (C3-R2: division by zero, C4-R2: aggregate error mapping, C5-R2: BITLEN gas).

C1 (HIGH) is the most critical: removing BIGINT SQRT from AC-1 must happen before any implementation begins.
