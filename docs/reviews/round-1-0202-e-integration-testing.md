# Adversarial Review: Mission 0202-e-bigint-decimal-integration-testing

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-e-bigint-decimal-integration-testing.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 1

---

## Executive Summary

Mission 0202-e covers Phase 4 of RFC-0202-A: end-to-end integration testing and verification for BIGINT/DECIMAL. The mission correctly identifies the testing surface. After adversarial review, **four issues are found: one HIGH severity (cross-type comparison hazard), two MODERATE, and one LOW**. The HIGH issue must be addressed to prevent tests from exercising known-panic code paths.

---

## Verification: RFC-0202-A vs Mission Coverage

| RFC § | Requirement | Mission AC | Status |
|---|---|---|---|
| §9 Test vectors | RFC-0110/RFC-0111 test vectors | AC-1, AC-2 | ⚠️ Which vectors? |
| §9 SQL parser | BIGINT/DECIMAL literals | AC-3, AC-4 | ✅ |
| §9 Canonical zero | BigInt::from_str("-0") == BigInt::from_str("0") | AC-5 | ✅ |
| §9 Cross-type cmp | BIGINT vs Integer, DECIMAL vs Float, etc. | AC-6 | ❌ PANIC HAZARD |
| §9 Persistence | Round-trip serialize/deserialize | AC-7, AC-8 | ✅ |
| §8 Gas benchmarking | Benchmark actual vs formula, 2× divergence threshold | AC-9 | ⚠️ Missing 2× threshold |
| §6.11 BTree scan | Range scans on indexed BIGINT/DECIMAL | AC-10 | ✅ |
| §6.5 NULL handling | NULL in expressions, IS NULL, ORDER BY | AC-11 | ✅ |

---

## NEW ISSUES

### C1 · HIGH: Cross-type comparison tests will panic before Phase 3

**Location:** AC-6 (cross-type comparison tests), RFC §6.12, mission 0202-b Notes

**Problem:** RFC-0202-A §6.12 explicitly warns:

> "The existing `Value::compare()` cross-type numeric path uses `as_float64().unwrap()` which **panics** for Extension-based numeric types (BIGINT, DECIMAL, DFP, Quant). Adding BIGINT/DECIMAL to `is_numeric()` triggers this panic for any cross-type comparison like `WHERE bigint_col > 42`."

Mission 0202-b (Phase 1b) adds BigInt/Decimal to `is_numeric()` and implements `compare_same_type()`. Phase 3 (mission 0202-d) implements the safe cross-type comparison dispatch that avoids the panic. **Phase 1-2 cross-type comparisons involving BigInt/Decimal panic via `as_float64().unwrap()`.**

AC-6 includes: "Cross-type comparison tests: BIGINT vs Integer, DECIMAL vs Float, BIGINT vs DECIMAL". These exact tests will **panic** during Phase 1-2 (before mission 0202-d implements the fix).

**Required fix:** Either:
1. Remove AC-6 from Phase 4 and re-add it after Phase 3 (0202-d) is complete, OR
2. Add an explicit note to AC-6 that these tests must only be **executed** after Phase 3 (mission 0202-d) is complete, even if written during Phase 4

> - [ ] Cross-type comparison tests: **execute only after Phase 3 (mission 0202-d) is complete** — these tests will panic during Phase 1-2 via `as_float64().unwrap()`. Phase 3 implements the safe cross-type comparison dispatch that avoids the panic.

---

### C2 · MODERATE: RFC-0110/RFC-0111 test vector coverage is unspecified

**Location:** AC-1 (BIGINT test vectors), AC-2 (DECIMAL test vectors), RFC §9

**Problem:** AC-1 says "Integration tests with RFC-0110 test vectors: bigint arithmetic, overflow, SHL/SHR, bitlen, cmp". AC-2 says "Integration tests with RFC-0111 test vectors: decimal arithmetic, sqrt, overflow, canonicalization". Neither specifies:
- Which specific test vectors from RFC-0110/RFC-0111 (there are 56 BIGINT and 57 DECIMAL entries with Merkle roots)
- Whether the Merkle root must be verified
- What the pass/fail criterion is

RFC-0202-A §9 references test vectors with Merkle roots. If the integration tests don't verify the Merkle roots, they aren't fully validating correctness against the reference spec.

**Recommended fix:** Expand AC-1 and AC-2:

> - [ ] Integration tests with RFC-0110 test vectors (56 entries with Merkle root):
>   - Execute all 56 test vectors for BIGINT (arithmetic, overflow, SHL, SHR, bitlen, cmp)
>   - Verify Merkle root of test vector outputs matches RFC-0110 §Test Vectors Merkle root
>   - Document Merkle verification result (pass/fail with root hash)
> - [ ] Integration tests with RFC-0111 test vectors (57 entries with Merkle root):
>   - Execute all 57 test vectors for DECIMAL (arithmetic, sqrt, overflow, canonicalization)
>   - Verify Merkle root of test vector outputs matches RFC-0111 §Test Vectors Merkle root
>   - Document Merkle verification result (pass/fail with root hash)

---

### C3 · MODERATE: Gas benchmarking acceptance criterion is underspecified

**Location:** AC-9, RFC §8

**Problem:** AC-9 says "Benchmark serialization/deserialization gas costs... If divergence exceeds 2×, update formulas." The "2× divergence threshold" is mentioned in the AC text but the RFC §8 says only:

> "Benchmark serialization/deserialization gas costs... Confirm estimates do not diverge from real costs by more than 2× — if they do, update the formulas before production deployment."

The mission AC correctly includes the 2× threshold from the RFC. However, it doesn't specify:
- How to benchmark (microbenchmark framework? stoolap's own benchmarking?)
- What "representative payload sizes" means exactly (which limb counts? which scales?)
- What the expected gas formulas are for serialization/deserialization

RFC §8 provides estimates: BIGINT serialization ~100 gas, deserialization ~100 gas, DECIMAL serialization ~20 gas, deserialization ~20 gas. These should be the baseline for comparison.

**Recommended fix:** Expand AC-9:

> - [ ] Benchmark serialization/deserialization gas costs per RFC §8:
>   - BIGINT: measure `BigInt::serialize()` and `BigInt::deserialize()` gas across 1-limb, 16-limb, 32-limb, 64-limb payloads
>   - DECIMAL: measure `decimal_to_bytes()` and `decimal_from_bytes()` gas across scale 0, 12, 24, 36
>   - Compare measured values against RFC §8 estimates (serialize ~100, deserialize ~100 for BIGINT; ~20 each for DECIMAL)
>   - If measured/estimated ratio exceeds 2× in either direction, update the RFC §8 formulas before production deployment
>   - Document benchmark methodology and results

---

### C4 · LOW: DECIMAL sqrt test vectors from RFC are not referenced

**Location:** AC-2, RFC §9, RFC §7

**Observation:** RFC-0202-A §9 includes DECIMAL sqrt test vectors that are not mentioned in the mission AC:

```
| DECIMAL sqrt | `SELECT SQRT(DECIMAL '2.00')` | `Decimal { mantissa: 141, scale: 2 }` |
| DECIMAL sqrt scale | `SELECT SQRT(DECIMAL '0.000001')` | `Decimal { mantissa: 10, scale: 4 }` |
```

The mission AC-2 mentions "decimal arithmetic, sqrt, overflow, canonicalization" but doesn't specifically call out these test vectors. Given that DECIMAL SQRT has a specific result scale formula (`⌈(scale + 1) / 2⌉`), these test vectors are valuable for verifying the scale computation.

**Recommended fix:** Add to AC-2:

> - [ ] Integration tests with RFC-0111 test vectors: include explicit DECIMAL SQRT test vectors from RFC-0202-A §9:
>   - `SQRT(DECIMAL '2.00')` → `{mantissa: 141, scale: 2}` (scale = ⌈(2+1)/2⌉ = 2)
>   - `SQRT(DECIMAL '0.000001')` → `{mantissa: 10, scale: 4}` (scale = ⌈(6+1)/2⌉ = 4)
>   - Verify result scale computation matches `⌈(input_scale + 1) / 2⌉`

---

### C5 · LOW: BTree index range scan tests could include lexicographic ordering verification

**Location:** AC-10, RFC §6.11

**Observation:** AC-10 says "BTree index range scan tests: `WHERE bigint_col > BIGINT '1000'`, `WHERE dec_col < DECIMAL '99.99'`". These test execution of range scans but don't verify the lexicographic key encoding ordering.

RFC-0202-A §6.11 is specifically about lexicographic encoding for BTree indexes. The key property to verify is:
- Negative values sort before zero, which sorts before positive values
- Within negatives: more negative (larger magnitude) sorts first
- Within positives: larger magnitude sorts first

**Recommended fix:** Add to AC-10:

> - [ ] BTree index range scan tests: verify lexicographic key ordering:
>   - BIGINT: verify `BIGINT '-100' < BIGINT '0' < BIGINT '100'` in index scan results
>   - DECIMAL: verify `DECIMAL '-12.3' < DECIMAL '0' < DECIMAL '12.3'` in index scan results
>   - Verify range scan returns correctly ordered results (not just non-empty results)

---

## Summary Table

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | HIGH | Cross-type comparison tests panic before Phase 3 | Mark AC-6 as executable only after Phase 3 complete, or defer |
| C2 | MODERATE | Test vector coverage unspecified | Specify Merkle root verification for 56 BIGINT + 57 DECIMAL vectors |
| C3 | MODERATE | Gas benchmarking underspecified | Expand with specific limb counts, scales, and comparison methodology |
| C4 | LOW | DECIMAL sqrt vectors not referenced | Add explicit SQRT test vectors from RFC |
| C5 | LOW | BTree ordering not verified | Add ordering verification to range scan tests |

---

## Inter-Mission Dependency Note

Mission 0202-e (Phase 4) is the final gate before production deployment. It depends on:
- Mission 0202-c (Phase 2 persistence) for serialization round-trip tests (AC-7, AC-8)
- Mission 0202-d (Phase 3 VM) for gas benchmarking (AC-9) and arithmetic tests (AC-1, AC-2)

The dependency chain is: 0202-a → 0202-b → 0202-c → 0202-d → 0202-e

AC-6 (cross-type comparison) cannot be executed until 0202-d is complete. AC-1 and AC-2 (test vectors) can be written during Phase 4 but must wait for 0202-d's VM dispatch to execute.

---

## Recommendation

Mission 0202-e is **conditionally ready** after resolving **C1** (cross-type comparison panic hazard). The tests must not be **executed** until Phase 3 (mission 0202-d) is complete — they can be written during Phase 4 but running them before the safe cross-type comparison dispatch is implemented will cause panics.

C2 and C3 are significant scope clarifications that should be addressed to ensure the integration testing is comprehensive and meaningful (Merkle root verification, specific gas benchmarking methodology).

The mission is well-scoped as a final verification gate. The Phase 3→Phase 4 dependency is correctly noted in the Dependencies section.
