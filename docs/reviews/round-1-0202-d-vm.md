# Adversarial Review: Mission 0202-d-bigint-decimal-vm

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-d-bigint-decimal-vm.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 1

---

## Executive Summary

Mission 0202-d covers Phase 3 of RFC-0202-A: expression VM operation dispatch and formula-based gas metering for BIGINT and DECIMAL arithmetic operations. The mission correctly identifies the VM integration points. After adversarial review, **five issues are found: one HIGH severity (specification mismatch), two MODERATE, and two LOW**. The HIGH issue is a specification conflict between the mission AC and the RFC.

---

## Verification: RFC-0202-A vs Mission Coverage

| RFC § | Requirement | Mission AC | Status |
|---|---|---|---|
| §7 BIGINT ops | ADD, SUB, MUL, DIV, MOD, CMP, SHL, SHR, BITLEN | AC-1 (BIGINT dispatch) | ⚠️ Missing BITLEN, SQRT mismatch |
| §7 DECIMAL ops | ADD, SUB, MUL, DIV, SQRT, CMP | AC-2 (DECIMAL dispatch) | ⚠️ Missing SQRT |
| §7a Aggregate ops | SUM, AVG, COUNT, MIN, MAX per type | Not in AC | ❌ Missing |
| §8 Gas formulas | Formula-based per operand sizes | AC-3 | ⚠️ Incomplete specification |
| §8 Per-query limit | 50,000 default, configurable | AC-5 | ✅ |
| §8 Per-op caps | MAX_BIGINT_OP_COST=15,000, MAX_DECIMAL_OP_COST=5,000 | AC-6 | ✅ |
| §8 Cost estimates | Optimizer plan cost modeling | AC-7 | ⚠️ Vague |
| §7a Streaming agg | Gas checked per-row | AC-8 | ⚠️ Formula not specified |
| §8 Pre-flight bounds | Minimal gas (10) before full operation | Not in AC | ❌ Missing |

---

## NEW ISSUES

### C1 · HIGH: BIGINT SQRT is not in RFC §7 but is in mission AC

**Location:** AC-1 (BIGINT operation dispatch), RFC §7 (Arithmetic Operations)

**Specification conflict:** AC-1 lists BIGINT operation dispatch including "SQRT". However, RFC-0202-A §7 (Arithmetic Operations table) explicitly shows:

```
| SQRT | N/A | `decimal_sqrt(a: &Decimal)` |
```

BIGINT has **no SQRT operation** in the RFC. The SQRT row shows "N/A" for BIGINT and `decimal_sqrt` for DECIMAL. Adding BIGINT SQRT dispatch to the VM would be implementing something not specified in the RFC.

**Required fix:** Remove SQRT from the BIGINT operation list in AC-1. If BIGINT SQRT is desired, it must be added to RFC-0202-A §7 first (via a separate RFC amendment or new RFC-0110 revision) — it cannot be added via the mission alone.

---

### C2 · MODERATE: Aggregate operations are missing from the acceptance criteria

**Location:** AC-1/AC-2 (operation dispatch), RFC §7a (Aggregate Operations)

**Problem:** RFC-0202-A §7a specifies aggregate operations for BIGINT and DECIMAL:

**BIGINT aggregates:**
- `COUNT(col)` → INTEGER, never overflows
- `SUM(col)` → BIGINT, returns `BigIntError::OutOfRange` when sum exceeds ±(2^4096 − 1)
- `MIN/MAX(col)` → BIGINT, never overflows
- `AVG(col)` → DECIMAL, returns `Error::NotSupported('AVG on BIGINT requires RFC-0202-B')` until RFC-0202-B

**DECIMAL aggregates:**
- `COUNT(col)` → INTEGER, never overflows
- `SUM(col)` → DECIMAL, returns `DecimalError::Overflow` if exceeds ±(10^36 − 1)
- `MIN/MAX(col)` → DECIMAL, never overflows
- `AVG(col)` → DECIMAL, `DecimalError::Overflow` if sum overflows, result scale = `min(36, input_scale + 6)`

The mission AC does not mention aggregate operations at all. These must be implemented in the VM as part of Phase 3.

**Required fix:** Add to Acceptance Criteria:

> - [ ] BIGINT aggregate dispatch: COUNT, SUM (with `BigIntError::OutOfRange` on overflow), MIN, MAX, AVG (returns `Error::NotSupported` until RFC-0202-B)
> - [ ] DECIMAL aggregate dispatch: COUNT, SUM (with `DecimalError::Overflow` on overflow), MIN, MAX, AVG (result scale = `min(36, input_scale + 6)`)
> - [ ] Streaming aggregation gas: per-row gas checked against query budget (per RFC §7a formulas: SUM = 10 + limbs for BIGINT, 10 + 2 × scale for DECIMAL)

---

### C3 · MODERATE: Gas metering formula specification is incomplete

**Location:** AC-3 (gas metering), AC-6 (per-op caps), RFC §8

**Problem:** AC-3 says "Gas metering wired: compute gas per RFC-0110/RFC-0111 formulas using operand sizes (limb count for BIGINT, scales for DECIMAL)". This does not specify the actual formulas. The RFC §8 specifies:

**BIGINT Gas (RFC-0110):**
| Operation | Formula |
|-----------|---------|
| ADD/SUB | 10 + limbs |
| MUL | 50 + 2 × 64 × 64 |
| DIV/MOD | 50 + 3 × 64 × 64 |
| CMP | 5 + limbs |
| SHL/SHR | 10 + limbs |

**DECIMAL Gas (RFC-0111):**
| Operation | Formula |
|-----------|---------|
| ADD/SUB | 10 + 2 × |scale_a - scale_b| |
| MUL | 20 + 3 × scale_a × scale_b |
| DIV | 50 + 3 × scale_a × scale_b |
| SQRT | 100 + 5 × scale |

The AC does not mention these specific formulas. The implementer must extract them from the RFC.

**Required fix:** Expand AC-3:

> - [ ] Gas metering wired per RFC §8 formulas:
>   - BIGINT: ADD/SUB = 10 + limbs; MUL = 50 + 2 × limbs × limbs; DIV/MOD = 50 + 3 × limbs × limbs; CMP = 5 + limbs; SHL/SHR = 10 + limbs; BITLEN = O(limbs) — use limb count from BigIntEncoding header
>   - DECIMAL: ADD/SUB = 10 + 2 × |scale_a - scale_b|; MUL = 20 + 3 × scale_a × scale_b; DIV = 50 + 3 × scale_a × scale_b; SQRT = 100 + 5 × scale; CMP = use decimal_cmp
>   - Per-operation caps: `MAX_BIGINT_OP_COST` (15,000) and `MAX_DECIMAL_OP_COST` (5,000) from determin crate
>   - Pre-flight bounds check: charge 10 gas to verify operation within valid bounds before committing full operation gas

---

### C4 · MODERATE: Optimizer cost estimates AC is vague

**Location:** AC-7, RFC §8

**Problem:** AC-7 says "Cost estimates added for optimizer (plan cost modeling)". This provides no guidance on:
- What cost model to use (per-row cost? per-operation cost?)
- How BIGINT/DECIMAL costs compare to existing INTEGER/FLOAT costs
- Whether the optimizer needs size-adaptive cost estimates (limb count/scale-dependent)

**Recommended fix:** Expand AC-7:

> - [ ] Optimizer cost estimates for BIGINT/DECIMAL operations:
>   - Use per-operation gas formulas as the cost unit
>   - BIGINT: cost scales with limb count (1–64 limbs)
>   - DECIMAL: cost scales with scale (0–36)
>   - Provide estimated costs for query planning (e.g., index scan vs. table scan decisions involving BIGINT/DECIMAL columns)

---

### C5 · LOW: Pre-flight bounds checks are missing

**Location:** Not in AC, RFC §8

**Observation:** RFC-0202-A §8 specifies:

> "Operations with bounded parameters (e.g., SHL, SHR with shift count) MUST perform a pre-flight bounds check before committing full gas. The pre-flight check charges a minimal fixed gas (10) to verify the operation is within valid bounds. If the check fails, the operation returns an error and only the pre-flight gas is consumed."

This is not in the mission AC. For SHL/SHR operations on BIGINT, a malicious or buggy shift count (e.g., 8192 on a 64-limb BigInt) could consume excessive gas without pre-flight checking.

**Recommended fix:** Add to Acceptance Criteria or Mission Notes:

> **Pre-flight bounds checks:** SHL and SHR operations must verify shift count is within valid bounds (0 ≤ shift < 8 × num_limbs for SHL; 0 ≤ shift < 8 × num_limbs for SHR) before committing full operation gas. Pre-flight check consumes 10 gas. If bounds check fails, return error without executing the full operation.

---

### C6 · LOW: `decimal_div` third parameter is ignored

**Location:** AC-2 (DECIMAL DIV), RFC §7

**Observation:** RFC-0202-A §7 notes for DIV:

> "The `_target_scale` parameter is completely ignored by the implementation (underscore prefix). The actual target scale is computed internally as `min(36, max(a.scale, b.scale) + 6)`. The VM must pass `0` as a placeholder value."

This is a minor implementation detail that implementers must know. The mission AC does not mention it.

**Recommended fix:** Add to DECIMAL DIV in AC-2:

> - DECIMAL DIV: call `decimal_div(a, b, 0)` — the third parameter is ignored; pass 0 as placeholder

---

## Summary Table

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | HIGH | BIGINT SQRT in AC but not in RFC §7 | Remove SQRT from BIGINT ops AC |
| C2 | MODERATE | Aggregate operations missing from AC | Add COUNT/SUM/MIN/MAX/AVG dispatch for BIGINT/DECIMAL |
| C3 | MODERATE | Gas formulas not explicit in AC | Expand AC-3 with RFC §8 formula details |
| C4 | MODERATE | Optimizer cost estimates vague | Expand AC-7 with cost model guidance |
| C5 | LOW | Pre-flight bounds checks missing | Add pre-flight check for SHL/SHR |
| C6 | LOW | decimal_div placeholder param not mentioned | Add note about passing 0 as third arg |

---

## Inter-Mission Dependencies

- Mission 0202-d depends on mission 0202-c (Phase 2 persistence) for serialization/deserialization infrastructure — limb count extraction from BigIntEncoding header and scale extraction from 24-byte DECIMAL encoding are needed for gas formulas.
- Mission 0202-d produces the VM operation dispatch that mission 0202-e (Phase 4 integration testing) will exercise with RFC-0110/RFC-0111 test vectors.
- AVG on BIGINT is blocked by RFC-0202-B (returns `Error::NotSupported` until RFC-0202-B implements internal BIGINT→DECIMAL conversion) — this is correctly noted in the mission dependency chain.

---

## Recommendation

Mission 0202-d is **not ready to start** without resolving **C1** (remove BIGINT SQRT — it is not in the RFC). C2 (add aggregate operations) is also blocking because aggregates are specified in RFC §7a and are part of the VM's job.

After C1 and C2 are addressed, the mission is implementable. C3-C6 are improvements that make the acceptance criteria unambiguous but don't block implementation.

The mission scope is correct (VM dispatch and gas integration) but incomplete — the aggregate operations are a significant missing component that must be added.
