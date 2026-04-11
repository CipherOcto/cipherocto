# Round 5 Adversarial Review: Mission 0202-d (Expression VM Support)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-d-bigint-decimal-vm.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 5

---

## Status of Prior Issues (Round 4)

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| A1 | MODERATE | BITLEN "conservative estimate" vs Reference "normative" | **FIXED** (commit dceb19c) |
| A2 | LOW | Blocking dependency on 0202-c undocumented | **FIXED** (commit 0fcb164) |
| A3 | LOW | Stale round-3 review document | **NOT FIXED** |
| Q1 | — | NEG/ABS/BITCOUNT missing from AC | **NOT RESOLVED** |
| Q2 | — | SHL/SHR MAX_LIMBS evaluation point | **RESOLVED** (mission's conservative choice is correct) |

---

## ACCEPTED ISSUES

### D1 · LOW: A3 (stale round-3 review) still unfixed

**Severity:** LOW
**Section:** `docs/reviews/round-3-0202-d-vm.md`

**Problem:** Round 4 required adding a resolution note to `docs/reviews/round-3-0202-d-vm.md`: "C1, C2, C3 resolved by mission update in commit dceb19c (2026-04-11)." This was not done in commits dceb19c or 0fcb164.

**Required fix:** Add to top of `docs/reviews/round-3-0202-d-vm.md`:
> **Resolution:** Issues C1 (SQRT under BigInt), C2 (MIN/MAX gas missing), C3 (BITLEN placeholder) were resolved in mission update commit dceb19c (2026-04-11). See [round-4 review](round-4-0202-d-vm.md) for details.

---

### D2 · LOW: NEG/ABS status unresolved — not VM-dispatchable or missing from AC?

**Severity:** LOW
**Section:** AC (BIGINT operation dispatch), Reference

**Problem:** Round 4 Q1 asked whether NEG and ABS are valid VM-dispatchable BIGINT operations per RFC-0110 §7. The Reference says "BigInt ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN" — NEG and ABS are absent from both the AC opcode list and the Reference. RFC-0110 §7 defines these operations but the mission does not clarify whether they are VM-dispatchable.

**Required fix:** Add to Reference section:
> - RFC-0110 §7 (BigInt operations — NEG, ABS defined but NOT VM-dispatchable; excluded from this mission)

Or add to AC if they are in scope.

---

### D3 · LOW: DECIMAL AVG gas ambiguity — SUM gas billed separately or combined?

**Severity:** LOW
**Section:** AC (DECIMAL AVG streaming aggregation gas)

**Problem:** AVG on DECIMAL computes a sum internally then divides. RFC §7a specifies AVG gas as `15 + 3 × scale` but does not clarify whether the SUM gas (`10 + 2 × scale`) is billed separately before the AVG gas, or whether `15 + 3 × scale` is a combined figure that supersedes the SUM gas.

**Required fix:** Add clarifying note to DECIMAL AVG AC:
> "AVG gas (15 + 3 × scale) includes sum computation; do not bill SUM gas separately. If sum exceeds ±(10^36 − 1), return DecimalError::Overflow before computing average."

---

## QUESTIONS

**Q1:** MIN/MAX on DECIMAL with scale=0: gas = `5 + 2 × 0 = 5`. Confirmed correct per RFC §7a.

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| D1 | LOW | A3 (stale round-3 review) unfixed | Add resolution note to round-3 review doc |
| D2 | LOW | NEG/ABS not in AC, not in Reference | Add NEG/ABS excluded-from-VM note to Reference |
| D3 | LOW | AVG DECIMAL SUM gas ambiguity | Add clarifying note about combined vs separate billing |

---

## Verdict

**Ready to start** after D1, D2, D3 are resolved. All are documentation/scope fixes. The core AC, gas formulas, dependency declaration, and BITLEN normative reference are correct.