# Round 6 Adversarial Review: Mission 0202-a (Type System Integration)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-a-bigint-decimal-typesystem.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 6

---

## Status of Prior Issues (Round 5)

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| B1 | CRITICAL | DECIMAL wire format: 25 bytes instead of 24 in RFC-0202-A §9 test vectors | **RESOLVED** — Fixed in RFC commit `9c178ae` (Round 12) |
| B2 | CRITICAL | BIGINT '2^64' wire format: 17 bytes instead of 24 in RFC-0202-A §9 test vectors | **RESOLVED** — Fixed in RFC commit `9c178ae` (Round 12) |
| B3 | LOW | §6.7 not explicit in Reference (Notes cites §6.7 but Reference only lists §6.4–§6.9 as a group) | **NOT FIXED** |

**Note on B1/B2:** Both CRITICAL RFC wire format errors have been corrected in the RFC itself (commit `9c178ae`, 2026-04-11). The DECIMAL test vectors now show correct 48 hex chars (24 bytes) and BIGINT '2^64' shows correct 48 hex chars (24 bytes).

---

## ACCEPTED ISSUES

### C1 · MEDIUM: Mission Reference still missing explicit §6.7 despite round-5 fix request

**Severity:** MEDIUM
**Section:** Reference (lines 56–66)
**Owner:** Mission author

**Problem:** The round-5 review requested that `RFC-0202-A §6.7 (coercion hierarchy)` be added explicitly to the Reference list. The current Reference still only lists `RFC-0202-A §6.4–§6.9 (Value layer extensions)` as a group. While the group technically encompasses §6.7, the specific citation in the coercion context warrants explicit mention.

**Required fix:** Add `RFC-0202-A §6.7 (coercion hierarchy)` explicitly to the Reference list, separate from the §6.4–§6.9 group.

---

### C2 · HIGH: Mission implementation entirely absent — zero AC items completed

**Severity:** HIGH
**Section:** All acceptance criteria
**Owner:** Mission author

**Problem:** The mission acceptance criteria describe implementation work in `src/core/types.rs` and `src/storage/mvcc/persistence.rs`. Current codebase shows:

**`src/core/types.rs` — NOT MODIFIED:**
- `DataType::Bigint = 13` and `DataType::Decimal = 14` are **absent** (enum ends at `Blob = 10`)
- `FromStr` still maps `BIGINT` → `DataType::Integer` and `DECIMAL`/`NUMERIC` → `DataType::Float`
- `is_numeric()` does NOT include `Bigint` or `Decimal`
- `from_u8()` has no entries for discriminants 13 or 14

**Evidence:** The types.rs enum ends with `Blob = 10`. The `FromStr` implementation maps `BIGINT` to `Integer` and `DECIMAL`/`NUMERIC` to `Float`. The `is_numeric()` only includes `Integer | Float | DeterministicFloat | Quant`.

**Required fix:** Implement all 10 acceptance criteria items. This is a complete implementation task.

---

## ACCEPTANCE CRITERIA REVIEW

| AC | Description | Verifiable? | Complete? | Notes |
|----|-------------|-------------|-----------|-------|
| 1 | `DataType::Bigint = 13` and `DataType::Decimal = 14` in types.rs | YES | NO | Not implemented |
| 2 | `FromStr` updated for BIGINT/DECIMAL/NUMERIC keywords | YES | NO | Still maps to Integer/Float |
| 3 | `NUMERIC_SPEC_VERSION: u32 = 2` in persistence.rs | YES | NO | Not implemented |
| 4 | `from_str_versioned()` in persistence.rs | YES | NO | Not implemented |
| 5 | `Display` updated for BIGINT/DECIMAL | YES | NO | Not implemented |
| 6 | `is_numeric()` includes Bigint \| Decimal | YES | NO | Not implemented |
| 7 | `is_orderable()` includes Bigint \| Decimal | YES | NO | Not implemented |
| 8 | `from_u8()` entries for 13 and 14 | YES | NO | Not implemented |
| 9 | Unit tests in types.rs | YES | NO | Not implemented |
| 10 | Unit tests for `from_str_versioned()` in persistence layer | YES | NO | Not implemented |

**AC Item 2 note:** `FromStr` should NOT be version-gated per RFC §1 — it always resolves to new types. Version-gating is only in `from_str_versioned()` for WAL replay. The AC does not clarify this distinction.

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action | Owner |
|----|----------|-------|----------------|-------|
| C1 | MEDIUM | §6.7 not explicit in Reference | Add `RFC-0202-A §6.7 (coercion hierarchy)` explicitly to Reference list | Mission author |
| C2 | HIGH | Mission implementation entirely absent | Implement all AC items in types.rs and persistence.rs | Mission author |

---

## Verdict

**Not ready to start.** The CRITICAL RFC errors (B1/B2) from round-5 have been resolved in the RFC. B3 (LOW, §6.7 not explicit) remains unresolved and should be fixed. C2 reveals the mission has not been started — all 10 AC items need implementation. The dependency "0110-wal-numeric-spec-version (open)" should be tracked.

**Action required:**
- Mission author should fix C1 (add §6.7 explicit reference)
- Mission author should begin implementation of all AC items per the RFC specification