# Round 4 Adversarial Review: Mission 0202-e (Integration Testing and Verification)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-e-bigint-decimal-integration-testing.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 4

---

## Status of Prior Issues

All round 3 fixes are correctly applied. Round 3 added aggregate operation tests (COUNT/SUM/MIN/MAX/AVG), within-sign BTree ordering tests, and proper cross-type comparison execution timing (after Phase 3).

---

## ACCEPTED ISSUES

### C1 · LOW: AC-5 canonical zero verification assumes from_str("-0") succeeds; RFC-0110 §10.2 says reject

**Severity:** LOW
**Section:** AC-5 (Canonical zero verification)

**Problem:** AC-5 says `BigInt::from_str("-0")` and `BigInt::from_str("0")` must produce byte-identical serialization. This assumes `from_str("-0")` returns without error and applies canonicalization internally.

RFC-0110 §10.2 states: "An implementation MUST reject (TRAP) any non-canonical input." Per §6.10 `is_zero` definition, negative zero (`{limbs: [0], sign: true}`) is **non-canonical** (`sign ≠ 0` means `is_zero = false`). RFC-0110 §10.2 says to reject, not to canonicalize.

If the determin crate `from_str("-0")` returns an error (as RFC-0110 §10.2 literally requires), AC-5's test cannot run as specified. The mission AC does not account for the possibility that `from_str("-0")` is rejected rather than canonicalized.

**Required fix:** Add clarifying note to AC-5:
```
- Note: RFC-0110 §10.2 requires rejecting non-canonical inputs. If `BigInt::from_str("-0")` returns `Error` rather than canonical bytes, update this AC to expect error for "-0" input and verify canonical zero only from "0" input. Confirm determin crate behavior before writing tests.
```

---

### C2 · LOW: AC-10 DECIMAL within-negative ordering test vectors unspecified

**Severity:** LOW
**Section:** AC-10 (BTree index range scan), RFC §6.11

**Problem:** AC-10 provides explicit BIGINT within-sign ordering test vectors:
- `BIGINT '-2^64' < BIGINT '-1'` (2 limbs vs 1 limb, both negative)
- `BIGINT '2^64' > BIGINT '1'` (2 limbs vs 1 limb, both positive)

For DECIMAL within-negative ordering, the AC only says: "verify different negative mantissas sort correctly after sign-flip transformation" — no concrete test values.

RFC-0202-A §6.11 specifies the sign-flip transformation (XOR byte 0 with 0x80), but without explicit test values, the DECIMAL within-negative ordering test is incomplete.

**Required fix:** Add explicit DECIMAL within-negative test vectors:
```
- `DECIMAL '-2' < DECIMAL '-1'` (both negative; -2 sorts below -1 after sign-flip encoding)
- `DECIMAL '-100' < DECIMAL '-1'` (3-digit vs 1-digit mantissa, both negative)
- Verify sign-flip: `DECIMAL '-1'` (mantissa = -1) → encoded byte0 = 0x80 XOR 0x7F = 0xFF → sorts among negatives
```

---

### C3 · LOW: AC-4 "per RFC §8" reference ambiguous — may confuse with RFC-0110 §8 arithmetic formulas

**Severity:** LOW
**Section:** AC-4 (Benchmark serialization/deserialization gas costs)

**Problem:** AC-4 says "per RFC §8" and references "~100" and "~20" gas estimates. These match RFC-0202-A §8 (serialization/conversion gas), NOT RFC-0110 §8 (arithmetic gas formulas: ADD/SUB 10+limbs, MUL 50+2×limbs², etc.).

An implementer who reads "per RFC §8" and reaches for RFC-0110 §8 will find arithmetic formulas, not serialization estimates. The wrong section could lead to implementing the wrong benchmark.

**Required fix:** Change "per RFC §8" to "per RFC-0202-A §8 (serialization/conversion gas estimates)."

---

## QUESTIONS

**Q1:** Does `BigInt::from_str("-0")` in the determin crate return error or canonical bytes? RFC-0110 §10.2 says reject; RFC-0202-A §6 note says "prevented from entering the system at construction time." These are consistent (reject) but the test assumption in AC-5 may be wrong.

**Q2:** Who benchmarks the arithmetic operation gas formulas from RFC-0110 §8 (ADD: 10+limbs, MUL: 50+2×limbs², etc.)? AC-4 only covers serialization/conversion gas. 0202-d should implement arithmetic gas, but is there a separate benchmarking AC for it?

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| C1 | LOW | AC-5 assumes from_str("-0") canonicalizes; RFC says reject | Add note confirming determin crate behavior; update AC if rejection is expected |
| C2 | LOW | DECIMAL within-negative test vectors unspecified | Add explicit DECIMAL test vectors for within-negative ordering |
| C3 | LOW | "per RFC §8" ambiguous | Change to "per RFC-0202-A §8" to avoid confusion with RFC-0110 §8 |

---

## Verdict

**Ready to start** after C1 and C2 are addressed. C1 requires confirming the determin crate's `from_str` behavior for "-0". C2 is a completeness fix (add missing test vectors). C3 is a documentation clarification. No MODERATE or CRITICAL issues found.