# Round 4 Adversarial Review: Mission 0202-c (Persistence and BTree Indexing)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-c-bigint-decimal-persistence.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 4

---

## Status of Prior Issues

Round 3 found two issues: C1 (521-byte discrepancy — persisted as flag in AC-8, not resolved), C2 (Reference missing §4a — **FIXED**). The current mission file has both AC-8's discrepancy flag and §4a in Reference.

---

## ACCEPTED ISSUES

### C1 · LOW: 521 vs 513 byte discrepancy — RFC internal inconsistency blocks implementation

**Severity:** LOW (but blocking)
**Section:** AC-8, RFC-0202-A §6.11, §Storage Overhead

**Problem:** AC-8 correctly identifies the inconsistency: the format notation `[limb_count_with_sign: u8][limb0: BE]...[limbN: BE][zero_pad: 8 × (64 − N)]` mathematically resolves to **513 bytes max** (1 + 8(N+1) + 8(63−N) = 513). But RFC §6.11 and §Storage Overhead both state **521 bytes max**.

The mission's AC-8 says "flag any discrepancy to RFC maintainers" — but this is an implementation blocker. The RFC cannot be ambiguous on byte count for a BTree key encoding.

The AC also directs implementers to "verify against RFC-0110" — but RFC-0110 does not define lexicographic encoding. The authoritative source is RFC-0202-A §6.11, not RFC-0110.

**Math verification:** For N limbs (limb0 through limbN, N+1 total limbs):
- Sign prefix: 1 byte
- Actual limbs: 8(N+1) bytes
- Zero padding: 8(63−N) bytes (fills to 64 total limbs in fixed-width array)
- Total: 1 + 8N + 8 + 504 − 8N = **513 bytes** (constant for any N 0–63)

Maximum is 513 bytes, not 521. The 521-byte claim in RFC §Storage Overhead refers to the **serialized persistence format** (tag 13 + BigIntEncoding = 1 + 520 = 521 bytes), not the lexicographic key format.

**Required fix:**
1. RFC-0202-A §6.11: Correct "521 bytes max" to "513 bytes max" (format description is correct)
2. AC-8: Remove the 521-byte claim; the format is 513 bytes. Change to: "Verify encoded key length: 1 byte sign-prefix + 8(N+1) actual limbs + 8(63−N) zero-padding = **513 bytes** for any N (0–63). Format description is authoritative; flag any deviation from RFC-0202-A §6.11."

---

### C2 · LOW: Reference section still missing §4a (round 3 issue — NOT fixed)

**Severity:** LOW
**Section:** Reference

**Status:** Round 3 identified this and the mission was NOT updated. The Reference section still does not list §4a (NUMERIC_SPEC_VERSION wire format), which AC-7 explicitly implements.

**Required fix:** Add to Reference:
```
- RFC-0202-A §4a (NUMERIC_SPEC_VERSION wire format and upgrade trigger)
```

---

### C3 · LOW: Deserialize arm ordering not captured as explicit AC

**Severity:** LOW
**Section:** AC-5, Notes

**Problem:** AC-5 implements the wire tag 13 handler for `deserialize_value`, but does not state that tag 13/14 handlers must appear **before** the generic Extension handler (tag 11) in the match chain. The requirement is only in the Notes section, not in any AC checkbox. The Notes are correct but the ordering constraint is not an acceptance criterion.

**Required fix:** Add to AC-5: "Wire tag 13 handler added to `deserialize_value` — must appear before the generic Extension handler (tag 11) in the match chain."

---

## QUESTIONS

**Q1:** Should AC-8 direct implementers to "flag to RFC maintainers" or should the mission author have already resolved this before publishing? The mission author recognized the discrepancy — should the fix have been applied to the RFC before the mission was created?

**Q2:** Is the 521-byte claim in §Storage Overhead referring to the serialized BIGINT format (tag 13 + BigIntEncoding) rather than the lexicographic key? If so, §Storage Overhead is correct for the persistence format but AC-8 is wrong to cite it for the lexicographic format.

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| C1 | LOW (blocking) | 521 vs 513 discrepancy — RFC inconsistency | Fix RFC-0202-A §6.11 to 513 bytes; update AC-8 to reference format description |
| C2 | LOW | Reference missing §4a | Add §4a to Reference |
| C3 | LOW | Deserialize arm ordering not explicit in AC | Add ordering requirement to AC-5 |

---

## Verdict

**Not ready to start pending C1 resolution.** The 521-byte discrepancy is an RFC authoring error — the format description is mathematically 513 bytes. RFC-0202-A §6.11 needs a maintenance update before this mission can proceed unambiguously. C2 and C3 are simple documentation fixes that should be applied in the same revision.