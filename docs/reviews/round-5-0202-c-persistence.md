# Round 5 Adversarial Review: Mission 0202-c (Persistence and BTree Indexing)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-c-bigint-decimal-persistence.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 5

---

## Status of Prior Issues (Round 4)

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| C1 | LOW | 521 vs 513 byte discrepancy (RFC inconsistency) | **PARTIALLY ADDRESSED** — mission AC-8 correctly identifies 513; RFC §6.11 still wrong |
| C2 | LOW | Reference missing §4a | **FIXED** (commit 0fcb164) |
| C3 | LOW | Deserialize arm ordering not explicit in AC | **FIXED** (commit 0fcb164) |

---

## ACCEPTED ISSUES

### C4 · LOW: AC-8 format notation uses `64 − N` zero-padding but claims 513 bytes — inconsistent

**Severity:** LOW
**Section:** AC-8 (BIGINT lexicographic encoding)

**Problem:** The format notation `[limb_count_with_sign: u8][limb0: BE][limb1: BE]...[limbN: BE][zero_pad: 8 × (64 − N)]` uses `64 − N` for zero-padding. The AC also states the byte count formula as "8(N+1) actual limbs + 8(63−N) zero-padding = **513 bytes**". These are inconsistent — if zero-padding is `8 × (64 − N)`, total = 1 + 8(N+1) + 8(64 − N) = 521 bytes, not 513.

The correct formula to yield 513 bytes constant is `8 × (63 − N)` zero-padding.

**Required fix:** Change format notation to `zero_pad: 8 × (63 − N)` to match the stated 513-byte result, OR update the byte count formula to reflect `64 − N` yielding 521 bytes. The former (63 − N → 513 bytes) is the correct fix since the AC already states 513.

---

### C5 · LOW: AC-3 debug assertion description is internally contradictory

**Severity:** LOW
**Section:** AC-3

**Problem:** AC-3 describes an assertion that fires "if tag byte is 13 or 14 and the code reaches the generic arm." With correct arm ordering (tag 13/14 before tag 11), tag 13/14 NEVER reach the generic arm. The note acknowledges this ("defensive check against future arm-reordering bugs") but the description still describes a situation that shouldn't occur with correct code.

Additionally, the assertion description doesn't clarify it fires for EITHER tag 13 OR 14 (not both simultaneously).

**Required fix:** Clarify:
> "Debug assertion added: place inside the generic Extension arm (tag 11 branch). Fires if tag byte is 13 OR 14 and the code reaches this arm — indicating an arm-ordering bug where tag 13/14 did not precede the generic Extension arm. This is defense-in-depth; with correct arm ordering (see Mission Notes) this assertion never fires."

---

### C6 · LOW: RFC-0202-A §6.11 still states "521 bytes max" — mission AC is correct but RFC is wrong

**Severity:** LOW (but blocking for RFC)
**Section:** RFC-0202-A §6.11, Mission AC-8 (as reference)

**Status:** Mission AC-8 correctly identifies the format as 513 bytes and explains that 521 bytes refers to the serialized persistence format. However, the RFC's own §6.11 text still says "521 bytes max" directly in the format description, not as a cross-reference.

**Required fix for RFC-0202-A §6.11:** Change "521 bytes max" to "513 bytes max (constant for any N, 0–63)" and note that 521 bytes is the serialized persistence format maximum (see §Storage Overhead).

---

## QUESTIONS

**Q1:** AC-6 (wire tag 14 handler for `deserialize_value`) does not include arm ordering requirement, though the Notes section states it must appear before tag 11. Is the lack of explicit ordering in AC-6 intentional (self-evident) or an oversight?

**Q2:** Should serialize arm ordering for wire tags 13/14 be an explicit AC checkbox (like deserialize arm ordering in AC-5), or is the Notes section sufficient given AC-3's debug assertion defense?

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| C4 | LOW | AC-8 format: 64−N zero-padding vs 513-byte claim inconsistency | Change format notation to `8 × (63 − N)` to match 513-byte result |
| C5 | LOW | AC-3 assertion self-contradicts (fires for tags that shouldn't reach generic arm) | Clarify as defense-in-depth; fires for EITHER tag 13 OR 14 |
| C6 | LOW | RFC §6.11 still says "521 bytes max" | Fix RFC-0202-A §6.11 to "513 bytes max" with cross-ref to §Storage Overhead for 521 |

---

## Verdict

**Not ready to start** — C4 and C6 must be resolved. C4 is a mission documentation fix (format notation). C6 is an RFC authoring error that blocks unambiguous implementation; the mission is not responsible for fixing the RFC but must note the conflict. C2 and C3 confirmed fixed. C5 is a documentation clarity fix.