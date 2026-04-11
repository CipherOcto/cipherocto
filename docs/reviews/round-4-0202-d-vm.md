# Round 4 Adversarial Review: Mission 0202-d (Expression VM Support)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-d-bigint-decimal-vm.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 4

---

## Status of Prior Issues

Round 3 found three issues: C1 (SQRT under BigInt — **FIXED**, Reference updated to say "SQRT is N/A for BIGINT per RFC §7"), C2 (MIN/MAX gas missing — **FIXED**, streaming aggregation gas bullets now include MIN/MAX for both BIGINT and DECIMAL), C3 (BITLEN placeholder note — **FIXED**, Reference updated to say "RFC-0110 §8 specifies 10 + limbs (v2.14)"). All round 3 prescriptions are correctly present in the current mission file.

---

## ACCEPTED ISSUES

### A1 · MODERATE: AC BITLEN gas note still says "conservative estimate" despite Reference claiming normative

**Severity:** MODERATE
**Section:** AC (BITLEN gas), Reference

**Problem:** Reference section (line 77) says: "RFC-0110 §8 specifies `10 + limbs` (v2.14). No amendment needed." The AC item for BITLEN gas (line 34) still reads: "BITLEN = 10 + limbs **(conservative estimate — verify against RFC-0110 reference before production)**."

These are contradictory. If RFC-0110 v2.14 is normative ("no amendment needed"), the "conservative estimate" and "verify before production" language is stale and should be removed.

**Required fix:** Remove "conservative estimate" qualifier from BITLEN gas AC item. Change to: "BITLEN = 10 + limbs (per RFC-0110 §8 v2.14 — confirmed normative)."

---

### A2 · LOW: Blocking dependency on 0202-c not documented

**Severity:** LOW
**Section:** Dependencies

**Problem:** Mission 0202-d depends on 0202-c (open). The dependency listing does not clarify whether 0202-d should block on 0202-c completion or proceed concurrently. AC items in 0202-d reference wire tags 13/14 and `serialize_value`/`deserialize_value` — if these interfaces are not finalized, 0202-d implementation could face rework.

**Required fix:** Add blocking note to Dependencies:
```
- Mission: 0202-c-bigint-decimal-persistence (open) — **blocking**; wire tags 13/14 and serialize/deserialize must be finalized before 0202-d implementation begins
```

---

### A3 · LOW: Stale R3 review document

**Severity:** LOW
**Section:** docs/reviews/round-3-0202-d-vm.md

**Problem:** The round-3 review at `docs/reviews/round-3-0202-d-vm.md` lists C1, C2, C3 as unresolved. The mission was subsequently updated (post-R3) to fix all three. The review document now describes a state that no longer exists, which may cause future reviewers to reopen resolved issues.

**Required fix:** Add resolution note at top of round-3 review: "C1, C2, C3 resolved by mission update in commit dceb19c (2026-04-11)."

---

## QUESTIONS

**Q1:** Are NEG, ABS, or BITCOUNT valid VM-dispatchable BIGINT operations per RFC-0110 §7? The mission AC lists 9 opcodes (ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN). RFC-0110 §7's operation table may include NEG, ABS, BITCOUNT — if so, these are missing from the AC.

**Q2:** SHL/SHR pre-flight check: `0 ≤ shift < 8 × num_limbs`. RFC-0110 says operations must reject if `limbs > MAX_LIMBS`. For SHR producing zero: is the check on the normalized result (1 limb, always passes) or the intermediate limb count? The mission rejects `shift = 8 × num_limbs` (produces zero, 1 normalized limb). RFC-0110 text does not disambiguate.

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| A1 | MODERATE | BITLEN "conservative estimate" contradicts Reference "normative" claim | Remove stale qualifier from AC BITLEN item |
| A2 | LOW | Blocking dependency on 0202-c not documented | Add blocking qualifier to Dependencies |
| A3 | LOW | Round-3 review doc is stale | Add resolution note to round-3 review |
| Q1 | — | NEG/ABS/BITCOUNT may be missing from AC | Verify RFC-0110 §7 operation table; add if applicable |
| Q2 | — | SHL/SHR MAX_LIMBS evaluation point ambiguous | Clarify in mission or RFC-0110 |

---

## Verdict

**Ready to start** after A1 is resolved. A1 is a straightforward qualifier removal. A2 and A3 are documentation improvements. Q1 and Q2 require RFC clarification before they become ACCEPT issues.