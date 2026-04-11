# Round 6 Adversarial Review: Mission 0202-c (Persistence and BTree Indexing)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-c-bigint-decimal-persistence.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 6

---

## Status of Prior Issues (Round 5)

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| C4 | LOW | AC-8 format: `64−N` zero-padding vs 513-byte claim inconsistency | **UNRESOLVED** — AC-8 text unchanged, still says `8 × (64 − N)` |
| C5 | LOW | AC-3 assertion self-contradicts (fires for tags that shouldn't reach generic arm) | **UNRESOLVED** — AC-3 text unchanged |
| C6 | LOW | RFC §6.11 still says "521 bytes max" | **UNRESOLVED** — RFC error, mission cannot fix |

---

## ACCEPTED ISSUES

### C7 · CRITICAL: Mission is entirely blocked — `DataType::Bigint`/`Decimal` do not exist

**Severity:** CRITICAL
**Section:** All AC items
**Owner:** Mission author

**Problem:** `DataType::Bigint = 13` and `DataType::Decimal = 14` do not exist in the `DataType` enum (`src/core/types.rs` lines 27–68). The enum ends at `Blob = 10`. This means:
- AC-1 and AC-2 (serialize_value arms for wire tags 13/14) cannot be implemented
- AC-3 and AC-4 (deserialize_value handlers for wire tags 13/14) cannot be implemented
- AC-6 (`auto_select_index_type` for `DataType::Bigint | DataType::Decimal`) cannot be implemented

The mission declares dependencies on missions 0202-a and 0202-b, both marked `open`. **This mission cannot proceed until those dependencies are resolved.**

**Required fix:** Mark mission as blocked-on-dependencies. Do not attempt implementation until 0202-a adds `DataType::Bigint`/`Decimal`.

---

### C8 · LOW: AC-8 format notation `64−N` still inconsistent with 513-byte claim

**Severity:** LOW
**Section:** AC-8
**Status:** UNRESOLVED (carried from Round 5)

**Problem:** AC-8 format still specifies `zero_pad: 8 × (64 − N)`. With `64 − N`, total = 1 + 8(N+1) + 8(64 − N) = 521 bytes, not the stated 513 bytes.

**Required fix:** Change `zero_pad: 8 × (64 − N)` to `zero_pad: 8 × (63 − N)` to match the stated 513-byte result. Note: RFC-0202-A §6.11 (513 bytes constant) was already corrected in Round 12.

---

### C9 · LOW: AC-3 assertion description still internally contradictory

**Severity:** LOW
**Section:** AC-3
**Status:** UNRESOLVED (carried from Round 5)

**Problem:** AC-3 describes an assertion that fires "if tag byte is 13 or 14 and the code reaches the generic arm." With correct arm ordering (tag 13/14 before tag 11), tags 13/14 **never** reach the generic arm. The assertion is described as firing in a state that should not occur with correct code.

**Required fix (repeat from Round 5):** Clarify as defense-in-depth:
> "Fires if tag byte is 13 OR 14 and the code reaches this arm — indicating an arm-ordering bug where tag 13/14 did not precede the generic Extension arm. This is defense-in-depth; with correct arm ordering this assertion never fires."

---

### C10 · LOW: C6 (RFC §6.11 "521 bytes max") remains unresolved — RFC authoring error

**Severity:** LOW
**Section:** RFC-0202-A §6.11

**Status:** UNRESOLVED — mission cannot fix the RFC. However, RFC-0202-A §6.11 was partially corrected in Round 12 (the BIGINT lexicographic format note now says "513 bytes constant" and distinguishes it from "521 bytes max" serialized persistence format). The RFC text itself still has "521 bytes max" in the format description.

**Required fix:** Flag this to RFC maintainer. This is outside mission scope.

---

### C11 · LOW: AC-3 is not independently verifiable — pure defense-in-depth assertion

**Severity:** LOW
**Section:** AC-3

**Problem:** The debug assertion in AC-3 can never fire with correct arm ordering. It is purely defensive against future reordering bugs. There is no test that can demonstrate this assertion works — it can only be verified by intentionally misordering the arms.

**Required fix:** Either:
- Add a compile-time ordering guarantee (e.g., `matches!` in the tag 11 arm that causes compile error if tags 13/14 are not matched first), OR
- Add a comment explicitly stating this assertion is unverifiable and relies on code review

---

## QUESTIONS

**Q1:** AC-3 is labeled a debug assertion but is a required AC checkbox. Debug assertions disappear in release builds. Is AC-3 meant to be a `debug_assert!` (compile-time-only) or a real runtime check always active?

**Q2:** AC-8 specifies test vectors for BIGINT lexicographic ordering: `-2^64 < -1 < 0 < 1 < 2^64`. For `-2^64` (which exceeds signed 64-bit range), how is this represented in the BigInt encoding?

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| C7 | CRITICAL | DataType Bigint/Decimal missing — blocked on dependencies | Mark as blocked, do not attempt implementation until 0202-a/b complete |
| C8 | LOW | AC-8 format notation `64−N` inconsistent with 513-byte result | Change to `8 × (63 − N)` to match 513 bytes |
| C9 | LOW | AC-3 assertion self-contradicts | Apply Round 5 clarification |
| C10 | LOW | RFC §6.11 "521 bytes max" (RFC error) | Flag to RFC maintainer |
| C11 | LOW | AC-3 not independently verifiable | Add compile-time check or explicit comment |

---

## Verdict

**Not ready to start** — The mission is blocked on unresolved dependencies (0202-a, 0202-b). The foundational types (`DataType::Bigint`, `DataType::Decimal`) do not exist.

C8 and C9 remain unresolved from Round 5. C10 requires RFC maintainer action. C7 is a new CRITICAL issue recognizing the dependency blockade.

**Recommendation:** Close this mission as blocked-on-dependencies. Reopen when:
1. `DataType::Bigint` and `DataType::Decimal` exist (0202-a complete)
2. Missions 0202-a, 0202-b are resolved
3. C8 and C9 fixes are applied to mission text