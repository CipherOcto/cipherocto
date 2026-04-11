# Round 4 Adversarial Review: Mission 0202-a (Type System Integration)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-a-bigint-decimal-typesystem.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 4

---

## Status of Prior Issues

Round 3 found two issues: C1 (as_int64/as_float64 scope — **FIXED**, note in AC redirects to 0202-b) and C2 (Reference section missing §6.3 and §6.13 — **FIXED**). Both prescriptions are correctly present in the current mission file.

---

## ACCEPTED ISSUES

### A1 · CRITICAL: AC-9 wire format test vector — DECIMAL '123.45' wrong byte count

**Severity:** CRITICAL
**Section:** AC-9 (unit tests), wire format test vector table

**Problem:** The DECIMAL '123.45' wire format test vector shows:
```
DECIMAL '123.45' | `{mantissa: 12345, scale: 2}` | `[14]000000000000000000000000000003039000000000000000002`
```
The hex payload after `[14]` is **40 hex characters = 20 bytes**. But DECIMAL is a fixed 24-byte format (16-byte mantissa + 7 reserved bytes + 1 scale byte). The vector is missing 4 bytes.

**Byte count verification:** `[14]` (1) + `000000000000000000000000000003039000000000000000002` (40 = 20 bytes) = **21 bytes total**. Required: **24 bytes**.

**Required fix:** Correct the hex string to 24 payload bytes:
- Mantissa (16 bytes, big-endian i128): `0000000000003039` (12345)
- Reserved (7 bytes): `00000000000000`
- Scale (1 byte): `02`
- Full payload: `0000000000003039000000000000000000000000000002`

---

### A2 · CRITICAL: AC-9 wire format test vector — BIGINT '2^64' header/data mismatch

**Severity:** CRITICAL
**Section:** AC-9 (unit tests), wire format test vector table

**Problem:** The BIGINT '2^64' test vector:
```
BIGINT '2^64' | BigInt(2^64) | `[13]0100000002000000000100000000000000`
```
Header bytes 0–7: `01 00 00 00 02 00 00 00`
- Byte 4 = `02` → **num_limbs = 2** (two limbs present)

Bytes 8–15 (16 bytes for 2 limbs): `00_00_00_00_01_00_00_00` = **only 1 limb's worth of data**. A 2-limb BigInt requires 8 header + 16 data = **24 bytes total**. The vector provides only 16 data bytes.

For 2^64 = `0x1_0000_0000_0000_0000` in little-endian u64 limbs: limbs = `[0x0000_0000_0000_0001, 0x0000_0000_0000_0000]`. The vector's limb[0] = `0x0000_0000_0100_0000` (≠ 1).

**Required fix:** Either (a) show full 24-byte payload with 2 limbs, or (b) change header to `01` (1 limb) if intending BigInt(1).

---

### A3 · MEDIUM: RFC-0111 missing from Reference section

**Severity:** MEDIUM
**Section:** Reference

**Problem:** AC-3 explicitly references RFC-0111 §Canonical Byte Format for DECIMAL wire format, yet RFC-0111 (Numeric/Math: Deterministic DECIMAL) does not appear in the Reference section. RFC-0110 is listed; RFC-0111 is absent.

**Required fix:** Add to Reference:
```
- RFC-0111 (Numeric/Math): Deterministic DECIMAL — DECIMAL wire format reference
```

---

### A4 · MEDIUM: Dependencies section references non-existent mission

**Severity:** MEDIUM
**Section:** Dependencies

**Problem:**
```
- Mission: 0110-wal-numeric-spec-version (open) — WAL header integration
```
No such mission exists. The WAL header integration is normative in RFC-0110 §4a (accepted). There is no open mission with this ID.

**Required fix:** Either remove or rephrase: "RFC-0110 §4a (WAL header integration — specification complete, implementation pending)."

---

### A5 · LOW: AC-9 Notes reference §6.7 not explicitly in Reference section

**Severity:** LOW
**Section:** AC-9 Notes, Reference

**Problem:** AC-9 Notes state "§6.7 (coercion hierarchy) — covered in 0202-b AC-8, AC-9" but §6.7 is not explicitly listed in the Reference (only "§6.4–§6.9" as a group).

**Required fix:** Add §6.7 explicitly to Reference, or remove the specific §6.7 claim from AC-9 Notes.

---

## QUESTIONS

**Q1:** Does the DECIMAL '1' and DECIMAL '3' test vectors in the same AC table also have wrong byte counts (20 bytes instead of 24)? Only '123.45' was verified in detail.

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| A1 | CRITICAL | DECIMAL '123.45' 20 bytes instead of 24 | Correct hex string to 24-byte payload with big-endian mantissa |
| A2 | CRITICAL | BIGINT '2^64' 1-limb data for 2-limb header | Fix to show correct 2-limb encoding or change to 1 limb |
| A3 | MEDIUM | RFC-0111 absent from Reference | Add RFC-0111 to Reference section |
| A4 | MEDIUM | Non-existent mission in Dependencies | Remove or correct to point to RFC-0110 §4a |
| A5 | LOW | §6.7 in Notes but not explicit in Reference | Add §6.7 explicitly or remove from Notes |

---

## Verdict

**Not ready to start.** Two CRITICAL wire format test vector errors (A1, A2) will cause implementation failures if copied verbatim. A3 and A4 are straightforward fixes. The round-3 fixes (C1, C2) are correctly present — the mission is close to ready but the test vector errors are blockers.