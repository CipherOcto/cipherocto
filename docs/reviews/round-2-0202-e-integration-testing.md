# Adversarial Review: Mission 0202-e-bigint-decimal-integration-testing (Round 2)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-e-bigint-decimal-integration-testing.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 2

---

## Executive Summary

Round 1 review identified 5 issues (C1 HIGH, C2/C3 MODERATE, C4/C5 LOW). The review was committed but **the mission file was NOT updated with fixes** — all 5 round 1 issues remain unresolved. This Round 2 review:

1. Confirms all round 1 issues remain open
2. Finds **two new issues: one MODERATE (wire format test vectors not referenced) and one LOW (as_int64/as_float64 round-trip not tested)**
3. Provides complete fix language for all items

---

## Status of Round 1 Issues (ALL UNRESOLVED)

**Mission file `missions/open/0202-e-bigint-decimal-integration-testing.md` was NOT updated after round 1 review.** All 5 issues from round 1 remain open.

| ID | Severity | Issue | Status |
|---|---|---|---|
| C1 | HIGH | Cross-type comparison tests panic before Phase 3 | ❌ Still unresolved — no execution timing constraint |
| C2 | MODERATE | Test vector coverage unspecified (Merkle root verification) | ❌ Still underspecified |
| C3 | MODERATE | Gas benchmarking underspecified | ❌ Still underspecified |
| C4 | LOW | DECIMAL sqrt test vectors not referenced | ❌ Still missing |
| C5 | LOW | BTree ordering not verified in range scan tests | ❌ Still missing |

---

## NEW ISSUES (Round 2)

### C2-R2 · MODERATE: Wire format test vectors from RFC §9 are not referenced in AC

**Location:** AC-7, AC-8 (serialization round-trip), RFC §9 (Wire Format Test Vectors)

**Problem:** RFC-0202-A §9 includes a "Wire Format Test Vectors" table that specifies exact wire bytes for BIGINT and DECIMAL persistence serialization. AC-7 and AC-8 say "round-trip tests: BIGINT → serialize → deserialize → same value" but don't reference the wire format test vectors from the RFC.

The RFC wire format test vectors include:
- BIGINT '1': `[13]01000000010000000100000000000000`
- BIGINT '-1': `[13]01FF0000010000000100000000000000`
- BIGINT '0': `[13]01000000010000000000000000000000`
- BIGINT '2^64': `[13]010000000200000000000000000000000100000000000000`
- DECIMAL '123.45': `[14]00000000000000000000000000003039000000000000000002`
- DECIMAL '1': `[14]00000000000000000000000000000001000000000000000000`
- DECIMAL '0': `[14]00000000000000000000000000000000000000000000000000`
- DECIMAL '-12.3': `[14]FFFFFFFFFFFFFFFFFFFFFFFFFFFFFF85000000000000000001`

These test both the wire tag (13/14) and the canonical byte encoding. Without testing these specific bytes, a bug in the serialization format (e.g., wrong limb order, wrong sign encoding) would not be caught.

**Required fix:** Expand AC-7 and AC-8:

> - [ ] BIGINT serialization round-trip tests: verify against RFC §9 wire format test vectors:
>   - BIGINT '1' serializes to `[13]01000000010000000100000000000000`
>   - BIGINT '-1' serializes to `[13]01FF0000010000000100000000000000`
>   - BIGINT '0' serializes to `[13]01000000010000000000000000000000`
>   - BIGINT '2^64' serializes to `[13]010000000200000000000000000000000100000000000000`
> - [ ] DECIMAL serialization round-trip tests: verify against RFC §9 wire format test vectors:
>   - DECIMAL '123.45' serializes to `[14]00000000000000000000000000003039000000000000000002`
>   - DECIMAL '1' serializes to `[14]00000000000000000000000000000001000000000000000000`
>   - DECIMAL '0' serializes to `[14]00000000000000000000000000000000000000000000000000`
>   - DECIMAL '-12.3' serializes to `[14]FFFFFFFFFFFFFFFFFFFFFFFFFFFFFF85000000000000000001`

---

### C3-R2 · LOW: `as_int64()` and `as_float64()` round-trip not tested

**Location:** Not in AC, RFC §6.13

**Observation:** RFC-0202-A §6.13 adds Extension cases for `as_int64()` (BIGINT) and `as_float64()` (DECIMAL). These methods are partial conversions:
- BIGINT within i64 range → `Some(i64)`, out-of-range → `None`
- DECIMAL → `mantissa as f64 / 10f64.powi(scale as i32)` (precision loss possible)

Mission 0202-b (Phase 1b) should implement these per round 2 review. Mission 0202-e should test them.

**Recommended fix:** Add to AC:

> - [ ] `as_int64()` round-trip for BIGINT:
>   - `BIGINT '42'.as_int64()` → `Some(42)`
>   - `BIGINT '99999999999999999999'.as_int64()` → `None` (out of i64 range)
> - [ ] `as_float64()` precision loss test for DECIMAL:
>   - `DECIMAL '12345678901234567890.0'.as_float64()` → should produce an f64 value (precision loss acceptable per RFC §6.13)
>   - `DECIMAL '0.1'.as_float64()` → should produce `10.0` (exact representable)

---

### C4-R2 · LOW: Division by zero test is missing

**Location:** Not in AC, RFC §7, §Security Considerations

**Observation:** RFC §Security Considerations (and RFC-0202-A §12) specifies "Division by zero MUST return error". AC-1 and AC-2 (arithmetic tests from AC-1/AC-2 in mission 0202-e) don't explicitly include division by zero tests.

**Recommended fix:** Add to AC-1 or create new AC:

> - [ ] Division by zero tests:
>   - `BIGINT '1' / BIGINT '0'` → Error
>   - `DECIMAL '1.0' / DECIMAL '0.0'` → Error
>   - Verify error is returned, not panic or incorrect value

---

### C5-R2 · LOW: Canonical zero test should use wire format verification

**Location:** AC-5, RFC §9

**Problem:** AC-5 says "Verify `BigInt::from_str("-0")` produces canonical zero — compare zero encoding with `BigInt::from_str("0")`". The mission correctly identifies this but the verification method is underspecified. The RFC §9 wire test vector shows:

```
BIGINT '0': [13]01000000010000000000000000000000
```

If `-0` and `0` both serialize to this exact same byte sequence, the test passes. If they differ, the test vector in the RFC would need updating (and the determin crate's canonicalization behavior would need fixing).

**Recommended fix:** Expand AC-5:

> - [ ] Canonical zero verification: `BigInt::from_str("-0")` and `BigInt::from_str("0")` must produce byte-identical serialization:
>   - Serialize both to wire format
>   - Assert wire bytes are identical: `[13]01000000010000000000000000000000`
>   - If bytes differ, update RFC §9 test vectors to reflect actual canonical form and file issue against determin crate

---

## Summary Table (Round 2)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 (R1) | HIGH | Cross-type comparison tests panic before Phase 3 | ✅ Still open — add execution timing constraint |
| C2 (R1) | MODERATE | Test vector coverage unspecified | ✅ Still open — add Merkle root verification |
| C3 (R1) | MODERATE | Gas benchmarking underspecified | ✅ Still open — expand methodology |
| C4 (R1) | LOW | DECIMAL sqrt vectors not referenced | ✅ Still open — add explicit SQRT vectors |
| C5 (R1) | LOW | BTree ordering not verified | ✅ Still open — add ordering tests |
| C2-R2 | MODERATE | Wire format test vectors not referenced | Add RFC §9 byte-level verification to AC-7/AC-8 |
| C3-R2 | LOW | as_int64/as_float64 round-trip not tested | Add conversion round-trip tests |
| C4-R2 | LOW | Division by zero test missing | Add explicit div-by-zero error test |
| C5-R2 | LOW | Canonical zero test underspecified | Specify wire format byte comparison |

---

## Priority

The mission file must be updated with round 1 fixes first. C1 (HIGH — cross-type comparison execution timing) is the most critical safety item. C2-R2 (wire format test vectors) is the most important correctness item — without byte-level verification, serialization bugs would go undetected.

---

## Recommendation

Mission 0202-e is **not ready to start** — all round 1 issues remain unfixed and four new issues were found. The mission file must be updated with round 1 fixes (C1-C5) plus round 2 fixes (C2-R2: wire format test vectors, C3-R2: as_int64/as_float64 tests, C4-R2: div-by-zero, C5-R2: canonical zero wire verification).

The testing scope for 0202-e is larger than initially specified — wire format test vectors (C2-R2) and round-trip conversion tests (C3-R2) add significant coverage that was implicit in the RFC but not explicit in the original mission.
