# Adversarial Review: Mission 0202-d-bigint-decimal-vm (Round 3)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-d-bigint-decimal-vm.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 3

---

## Executive Summary

Round 1 and round 2 reviews identified 9 issues total. Both rounds' fixes were applied to the mission. This Round 3 review verifies fixes and finds **three issues: one MODERATE (Reference lists SQRT under BigInt), one MODERATE (aggregate MIN/MAX gas missing from AC), and one LOW (streaming aggregation COUNT gas not specified)**.

---

## Status of Prior Issues

| Round | Issue | Status |
|-------|-------|--------|
| R1-C1 | BIGINT SQRT in AC but not in RFC §7 | ✅ Fixed — removed |
| R1-C2 | Aggregate operations missing | ✅ Fixed — COUNT, SUM, MIN, MAX, AVG added |
| R1-C3 | Gas formulas incomplete | ✅ Fixed — exact formulas added |
| R1-C4 | Optimizer cost estimates vague | ✅ Fixed — size-adaptive guidance added |
| R1-C5 | Pre-flight bounds checks missing | ✅ Fixed — SHL/SHR pre-flight added |
| R1-C6 | decimal_div placeholder param not noted | ✅ Fixed — pass-0 note added |
| R2-C3-R2 | Division by zero not specified | ✅ Fixed — zero check added |
| R2-C4-R2 | Aggregate error mapping | ✅ Fixed — OutOfGas mapping added |
| R2-C5-R2 | BITLEN gas formula | ✅ Fixed — conservative estimate noted |

All prior fixes are correctly applied.

---

## NEW ISSUES (Round 3)

### C1 · MODERATE: Reference section lists SQRT under BigInt operations

**Location:** Reference section

**Problem:** The Reference section says:
> RFC-0110 §Operations (BigInt ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN, SQRT)

SQRT is listed under BigInt operations, but RFC §7 explicitly shows:
```
| SQRT | N/A | `decimal_sqrt(a: &Decimal)` |
```

SQRT is N/A for BIGINT. It should only appear under DECIMAL.

**Required fix:** Correct the Reference section:
```
- RFC-0110 §Operations (BigInt ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN)
- RFC-0111 §Operations (Decimal ADD, SUB, MUL, DIV, SQRT, CMP)
```

---

### C2 · MODERATE: Aggregate MIN/MAX gas is missing from the AC

**Location:** Streaming aggregation gas AC

**Problem:** RFC §7a specifies aggregate gas per row:
| Aggregate | BIGINT Gas | DECIMAL Gas |
|----------|-----------|-------------|
| COUNT | 5 | 5 |
| SUM | 10 + limbs | 10 + 2 × scale |
| MIN/MAX | 5 + limbs | 5 + 2 × scale |
| AVG | 15 + 2 × limbs | 15 + 3 × scale |

The mission AC currently specifies:
- BIGINT SUM: 10 + limbs per row
- DECIMAL SUM: 10 + 2 × scale per row
- BIGINT AVG: 15 + 2 × limbs per row
- DECIMAL AVG: 15 + 3 × scale per row

But MIN/MAX gas is missing from the AC. While MIN/MAX are simpler operations than SUM/AVG and have lower gas, they still consume gas per row.

**Required fix:** Update the streaming aggregation gas AC:

> - [ ] Streaming aggregation gas checked per-row (SUM, AVG) per RFC §7a:
>   - BIGINT COUNT: 5 gas per row
>   - BIGINT SUM: 10 + limbs per row
>   - BIGINT MIN/MAX: 5 + limbs per row
>   - BIGINT AVG: 15 + 2 × limbs per row
>   - DECIMAL COUNT: 5 gas per row
>   - DECIMAL SUM: 10 + 2 × scale per row
>   - DECIMAL MIN/MAX: 5 + 2 × scale per row
>   - DECIMAL AVG: 15 + 3 × scale per row (input column scale, not result scale)

---

### C3 · LOW: Reference section should acknowledge BITLEN gas is a placeholder

**Location:** Reference section, AC (BITLEN gas)

**Problem:** AC specifies "BITLEN = 10 + limbs (conservative estimate — verify against RFC-0110 reference before production)". The Reference section lists RFC-0110 §Operations but does not note that BITLEN gas is not yet in RFC-0110's gas tables.

**Recommended fix:** Update Reference section note:

```
- RFC-0110 §Operations (BigInt ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN — gas formula for BITLEN not yet in RFC-0110 §8; use 10 + limbs as conservative estimate pending RFC-0110 amendment)
- RFC-0111 §Operations (Decimal ADD, SUB, MUL, DIV, SQRT, CMP)
```

---

## Summary Table (Round 3)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | MODERATE | Reference lists SQRT under BigInt | Remove SQRT from BigInt Reference entry |
| C2 | MODERATE | Aggregate MIN/MAX gas missing from AC | Add MIN/MAX gas (5 + limbs / 5 + 2 × scale) |
| C3 | LOW | BITLEN gas placeholder not noted in Reference | Add note about BITLEN gas pending RFC-0110 |

---

## Recommendation

Mission 0202-d is **ready to start** after resolving **C1** (remove SQRT from BigInt Reference) and **C2** (add MIN/MAX aggregate gas). C3 is a documentation improvement.

All round 1 and round 2 fixes are correctly applied. The mission scope is comprehensive and the AC items are detailed.
