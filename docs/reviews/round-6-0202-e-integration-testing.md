# Round 6 Adversarial Review: Mission 0202-e (Integration Testing and Verification)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-e-bigint-decimal-integration-testing.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 6

---

## Status of Prior Issues (Round 5)

| ID | Issue | Status |
|----|-------|--------|
| C1 | "Verify determin crate behavior" has no deliverable (not a checkbox) | **NOT FIXED** |
| C2 | Merkle verification procedure unspecified (no expected root) | **PARTIALLY FIXED** — roots exist in determin crate but not referenced in mission |
| C3 | Phase 3 panic hazard unenforceable without 0202-d coordination | **PARTIALLY FIXED** — dependency noted in 0202-d but cross-reference missing in 0202-e |
| C4 | Serialization API unspecified | **NOT FIXED** |

All four Round 5 issues remain open or only partially addressed.

---

## ACCEPTED ISSUES

### C5 · CRITICAL: Mission has circular/unmet dependency chain — all ACs currently unverifiable

**Severity:** CRITICAL
**Section:** Dependencies, all ACs

**Dependency chain:** `0202-e → 0202-d → 0202-c → 0202-b → 0202-a`

- **0202-a (open):** `DataType::Bigint = 13` and `DataType::Decimal = 14` are **NOT yet added** to `src/core/types.rs`
- **0202-b (open):** `Value::bigint()`, `Value::decimal()`, `as_bigint()`, `as_decimal()` do **NOT exist**
- **0202-c (open):** Wire tags 13/14 are **not implemented** in `serialize_value`/`deserialize_value`
- **0202-d (open):** BigInt/DECIMAL operation dispatch is **not yet implemented** in `vm.rs`

**Consequence:** Every single AC in mission 0202-e is **currently impossible to execute**. The mission should be marked `Blocked` rather than `Open`.

**Required fix:** Add to Status section:
```
**Blocked by:** Missions 0202-a, 0202-b, 0202-c, 0202-d (all must complete before any AC can be executed)
```

---

### C6 · HIGH: AC-5 (canonical zero) still conflicts with RFC-0110 §10.2 after 2 rounds

**Severity:** HIGH
**Section:** AC-5, line 30–33

**Problem:** AC-5 requires that `BigInt::from_str("-0")` and `BigInt::from_str("0")` produce byte-identical serialization. It adds a note acknowledging the conflict but does not resolve it. This is the **third consecutive round** this issue appears. RFC-0110 §10.2 says "reject (TRAP)" for `-0` — the AC as written expects both parses to succeed, which violates the RFC.

**Required fix:** Convert to explicit two-part AC:
```
- [ ] Determine `BigInt::from_str("-0")` behavior in determin crate: returns Error or canonical bytes
  - If Error: AC-5 canonical zero verification uses only `BigInt::from_str("0")`
  - If canonical bytes: verify both "-0" and "0" produce identical wire bytes `[13]01000000010000000000000000000000`
- [ ] Execute canonical zero verification per above determination
```

---

### C7 · MODERATE: AC-1 and AC-2 still lack expected Merkle root values

**Severity:** MODERATE
**Section:** AC-1, AC-2

**Problem:** The determin crate **does contain** these values (`/home/mmacedoeu/_w/ai/cipherocto/determin/src/probe.rs`):
- BIGINT reference root: `c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0`
- DECIMAL reference root: `496bc8038e3fd38462f4308bf03088b3f872d000256a45ddb53d4932efff0c1c`

But the mission **does not reference these values**, nor does it specify the hash function (SHA-256) or what the test vector outputs look like.

**Required fix:** Add to AC-1:
```
- Compute Merkle root of all 56 test vector outputs using SHA-256
- Expected root: `c447fa82db0763435c1a18268843300c2ed811e21fcb400b18c75e579ddac7c0`
```

Add to AC-2:
```
- Compute Merkle root of all 57 test vector outputs using SHA-256
- Expected root: `496bc8038e3fd38462f4308bf03088b3f872d000256a45ddb53d4932efff0c1c`
```

---

### C8 · LOW: AC-3 (parser tests) is unverifiable — no BIGINT/DECIMAL literal syntax exists

**Severity:** LOW
**Section:** AC-3, AC-4

**Problem:** AC-3 requires "SQL parser tests for `BIGINT '...'` and `DECIMAL '...'` literals." Currently `BIGINT` parses to `DataType::Integer` and `DECIMAL` parses to `DataType::Float` — there is no `BIGINT '123'` literal syntax.

**Required fix:** Split AC-3 into:
```
- [ ] SQL parser tests for `BIGINT '123'` literal expressions (requires 0202-a type system + 0202-b Value::bigint constructor)
- [ ] SQL parser tests for `DECIMAL(p,s)` and `NUMERIC(p,s)` DDL column types
```

---

### C9 · LOW: Cross-type comparison panic hazard cross-reference still not in 0202-e

**Severity:** LOW
**Section:** AC-4, Dependencies

**Problem:** AC-4 says "execute only after Phase 3 (mission 0202-d) is complete." 0202-d does implement safe cross-type comparison dispatch. However, 0202-e does not have an explicit cross-reference to this requirement.

**Required fix:** Add to AC-4 or Dependencies:
```
- [ ] Cross-type comparison tests (BIGINT vs Integer, DECIMAL vs Float, BIGINT vs DECIMAL)
  **Prerequisite:** Phase 3 (0202-d) MUST implement safe cross-type comparison dispatch that avoids the as_float64().unwrap() panic described in 0202-d Notes.
```

---

### C10 · LOW: Serialization API still unnamed after two rounds

**Severity:** LOW
**Section:** AC-7, AC-8

**Problem:** AC-7 says "BIGINT '1' serializes to `[13]01000000010000000100000000000000`" but does not name the specific API. In the determin crate, functions are `BigInt::serialize()` / `BigInt::deserialize()` and `decimal_to_bytes()` / `decimal_from_bytes()`.

**Required fix:** Name the specific API:
```
- [ ] BIGINT: use `BigInt::serialize()` → `[13][BigIntEncoding]`; `BigInt::deserialize()` for round-trip
- [ ] DECIMAL: use `decimal_to_bytes()` → `[14][24-byte encoding]`; `decimal_from_bytes()` for round-trip
```

---

### C11 · INFORMATIONAL: Division by zero error mapping unspecified

**Severity:** INFORMATIONAL
**Section:** AC-12, AC-13

**Problem:** AC-12 and AC-13 specify that division by zero "returns Error" but do not name the specific error variant. The determin crate returns `BigIntError::DivisionByZero` and `DecimalError::DivisionByZero`. 0202-d maps these to `Error::invalid_argument("division by zero")`.

**Required fix:** Clarify:
```
- `BIGINT '1' / BIGINT '0'` → `Error::InvalidArgument("division by zero")`
- `DECIMAL '1.0' / DECIMAL '0.0'` → `Error::InvalidArgument("division by zero")`
```

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| C5 | CRITICAL | Circular dependency — all ACs unverifiable | Mark as blocked-on-dependencies |
| C6 | HIGH | AC-5 canonical zero conflicts with RFC after 2 rounds | Convert to explicit two-part AC |
| C7 | MODERATE | Merkle roots not referenced in mission | Add expected root hashes from determin crate |
| C8 | LOW | Parser literal tests unverifiable | Split AC-3 into type system + DDL tests |
| C9 | LOW | Panic cross-reference not in 0202-e | Add explicit cross-reference to 0202-d Notes |
| C10 | LOW | Serialization API unnamed after 2 rounds | Name specific API functions |
| C11 | INFO | Division by zero error mapping unspecified | Name error variants in AC |

---

## Verdict

**Not ready to start.** This mission is blocked by all four prerequisite phases (0202-a through 0202-d), none of which are complete. All acceptance criteria are currently impossible to execute. The mission should be reclassified from `Open` to `Blocked` until the dependency chain is resolved.

The four issues carried from Round 5 (C1, C2, C3, C4) were either not fixed or only partially fixed. C5 (CRITICAL circular dependency) and C6 (HIGH canonical zero conflict, unresolved for 2 rounds) are the most urgent.

**Before round-7:**
1. Mark mission as blocked on 0202-a through 0202-d
2. Fix C6 (canonical zero — this has persisted 3 rounds)
3. Add expected Merkle root values from determin crate