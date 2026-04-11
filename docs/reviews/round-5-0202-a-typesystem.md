# Round 5 Adversarial Review: Mission 0202-a (Type System Integration)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-a-bigint-decimal-typesystem.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 5

---

## Status of Prior Issues (Round 4)

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| A1 | CRITICAL | DECIMAL '123.45' wire format: 25 bytes instead of 24 | **NOT FIXED — RFC error, not mission error** |
| A2 | CRITICAL | BIGINT '2^64' wire format: 17 bytes instead of 24 | **NOT FIXED — RFC error, not mission error** |
| A3 | MEDIUM | RFC-0111 missing from Reference | **FIXED** (commit 0fcb164) |
| A4 | MEDIUM | Non-existent mission in Dependencies | **FIXED** (note added, commit 0fcb164) |
| A5 | LOW | §6.7 not explicit in Reference | **PARTIALLY FIXED** |

**Note on A1/A2:** These CRITICAL wire format errors are in RFC-0202-A §9 test vectors, not in the mission. The mission correctly cites RFC-0202-A as authoritative. These must be fixed in the RFC by the maintainer before implementation uses the test vectors.

---

## ACCEPTED ISSUES

### B1 · CRITICAL: RFC-0202-A §9 wire format test vectors — DECIMAL entries have 1 extra byte

**Severity:** CRITICAL
**Section:** RFC-0202-A §Test Vectors (lines 1299–1303)
**Owner:** RFC maintainer (@ciphercito)

**Problem:** All three DECIMAL wire format test vectors in the RFC show 50–51 hex characters (25 bytes) after the tag, but DECIMAL is fixed at 24 bytes (16-byte mantissa + 7 reserved + 1 scale):

| Vector | RFC hex chars | RFC bytes | Expected |
|--------|--------------|-----------|----------|
| DECIMAL '123.45' | 51 | 25.5 | 24 |
| DECIMAL '1' | 50 | 25 | 24 |
| DECIMAL '3' | 50 | 25 | 24 |

**Required fix:** Regenerate DECIMAL hex strings from determin crate encoder:
- DECIMAL '123.45': `[14]00000000000000000000000000003039000000000000000002` (48 hex chars = 24 bytes)
- DECIMAL '1': `[14]000000000000000000000000000000010000000000000000` (48 hex chars)
- DECIMAL '3': `[14]000000000000000000000000000000030000000000000000` (48 hex chars)

---

### B2 · CRITICAL: RFC-0202-A §9 wire format test vector — BIGINT '2^64' truncated at 17 bytes

**Severity:** CRITICAL
**Section:** RFC-0202-A §Test Vectors (line 1298)
**Owner:** RFC maintainer (@ciphercito)

**Problem:** BIGINT '2^64' test vector shows 34 hex characters (17 bytes) after tag. A 2-limb BigInt requires 24 bytes total (8-byte header + 16 bytes for 2 limbs). Additionally, the limb encoding is byte-reversed.

**Required fix:** Regenerate from determin crate:
```
[13]010000000200000001000000000000000000000000000000
```
(Hex: 48 chars = 24 bytes. Header: version=1, sign=0, reserved=0x0000, num_limbs=2, reserved=0x0000. Limb[0]: 0x0000_0000_0000_0001 LE = `0100000000000000`. Limb[1]: `0000000000000000`.)

---

### B3 · LOW: §6.7 not explicit in Reference despite specific citation in Notes

**Severity:** LOW
**Section:** Reference (line 61), Notes (line 69)

**Problem:** Notes reference "§6.7 (coercion hierarchy)" specifically but Reference only lists "§6.4–§6.9" as a group. The group reference technically covers §6.7 but the specific citation implies it warrants explicit mention.

**Required fix:** Add `RFC-0202-A §6.7 (coercion hierarchy)` explicitly to Reference list.

---

## QUESTIONS

**Q1:** Should wire format test vectors be duplicated in the mission (as a reference implementation checklist) to avoid implementers copying incorrect RFC values? Or should the mission rely entirely on the RFC as the canonical source?

**Q2:** The round-4 review noted that A1/A2 are "RFC errors discovered during mission review" — at what point does the mission author escalate RFC errors to the maintainer rather than noting them indefinitely?

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action | Owner |
|----|----------|-------|----------------|-------|
| B1 | CRITICAL | DECIMAL hex strings: 25 bytes instead of 24 | Fix RFC-0202-A §9 lines 1299–1301 | RFC maintainer |
| B2 | CRITICAL | BIGINT '2^64': 17 bytes instead of 24, wrong limb | Fix RFC-0202-A §9 line 1298 | RFC maintainer |
| B3 | LOW | §6.7 not explicit in Reference | Add §6.7 explicitly to Reference | Mission author |

---

## Verdict

**Not ready to start** — B1/B2 are CRITICAL but are RFC errors, not mission errors. The mission is correctly structured and cites the RFC as authoritative. B1/B2 must be fixed in the RFC by the maintainer before implementation uses the test vectors. B3 is a simple documentation fix that can be applied alongside the RFC fix.

**Action required:** RFC maintainer must regenerate wire format test vectors from the determin crate reference implementation for RFC-0202-A §9.