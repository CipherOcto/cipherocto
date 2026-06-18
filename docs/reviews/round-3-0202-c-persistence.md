# Adversarial Review: Mission 0202-c-bigint-decimal-persistence (Round 3)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-c-bigint-decimal-persistence.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 3

---

## Executive Summary

Round 1 and round 2 reviews identified 8 issues total. Both rounds' fixes were applied to the mission (all 5 round 1 issues, all 3 round 2 issues). This Round 3 review verifies fixes and finds **two issues: one MODERATE (BIGINT lexicographic zero_pad formula typo) and one LOW (Reference section missing §4a)**.

---

## Status of Prior Issues

| Round | Issue | Status |
|-------|-------|--------|
| R1-C1 | Lexicographic verification missing | ✅ Fixed |
| R1-C2 | NUMERIC_SPEC_VERSION wiring underspecified | ✅ Fixed |
| R1-C3 | Debug assertion placement ambiguous | ✅ Fixed |
| R1-C4 | REINDEX documentation vague | ✅ Fixed |
| R1-C5 | serialize arm ordering not explicit | ✅ Fixed (Notes section) |
| R2-C2-R2 | deserialize arm ordering unspecified | ✅ Fixed (Notes section) |
| R2-C3-R2 | BIGINT deserialize variable-length not specified | ✅ Fixed |
| R2-C4-R2 | Lexicographic test vectors unspecified | ✅ Fixed |

All prior fixes are correct.

---

## NEW ISSUES (Round 3)

### C1 · MODERATE: BIGINT lexicographic format has inconsistent zero_pad description

**Location:** AC-8 (BIGINT lexicographic encoding)

**Problem:** AC-8 describes the BIGINT lexicographic format as:
```
Format: `[limb_count_with_sign: u8][limb0: BE][limb1: BE]...[limbN: BE][zero_pad: 8 × (64 − N)]` — 521 bytes max
```

But the description text says:
```
- Verify encoded key length is 521 bytes (1 byte sign-prefix + 64 × 8 bytes padded limbs)
```

The "64 × 8 bytes" in the verification note confirms the correct formula (8 bytes per limb for 64 limbs = 512 bytes of padding). However, within the same AC-8, the inline format description `[limb_count_with_sign: u8][limb0: BE][limb1: BE]...[limbN: BE][zero_pad: 8 × (64 − N)]` is internally inconsistent:

If `limb0: BE` means 8 bytes per limb, then for 64 limbs the zero padding should be `8 × (64 − N)` bytes. But the variable `N` represents the actual number of limbs (not the number of zero-padding limbs), so the zero padding in bytes is `8 × (64 − N)`. The formula is dimensionally correct but the notation is confusing.

More critically: the format description in the AC inline notation uses `zero_pad: 8 × (64 − N)` but the **Mission Notes section** (which was added in round 1) says:

> **serialize arm ordering:** Wire tag 13 (BIGINT) and 14 (DECIMAL) arms MUST appear **before** the generic Extension arm (tag 11)...

This Notes section doesn't describe the lexicographic format — it was added for arm ordering. So the inconsistency is within AC-8 itself.

**Verification:** The RFC §6.11 uses the notation `[limb0: 8]...[limbN: 8]` to indicate 8 bytes per limb, and specifies "512 bytes total for padded limb array" and "521 bytes max" for the full key. The mission's AC is consistent with the RFC's intent but uses a slightly different notation.

**Recommended fix:** Clarify the zero_pad notation in AC-8:

> - Format: `[limb_count_with_sign: u8][limb0: u64_le][limb1: u64_le]...[limbN: u64_le][zero_pad: u64_le × (64 − N)]`
>   - Each limb is 8 bytes (u64, big-endian representation in the key)
>   - Zero padding fills to 64 limbs: `8 × (64 − N)` bytes of `0x00`
>   - Total: 1 + 8×N + 8×(64−N) = 1 + 512 = **513 bytes** for N limbs? No — wait.

Actually, let me verify: For N=1 (1 limb): 1 + 8 + 496 = 505 bytes. For N=64 (64 limbs): 1 + 512 + 0 = 513 bytes. The RFC says "521 bytes max" which matches N=64: 1 + 64×8 = 513? No, 1 + 512 = 513, not 521.

Let me re-read RFC §6.11 carefully:

> "Format: `[limb_count_with_sign: u8][limb0: BE][limb1: BE]...[limbN: BE][zero_pad: 8 × (64 − N)]` — **521 bytes max**"

For N=64: 1 + 8×64 + 8×(64−64) = 1 + 512 + 0 = 513 bytes. But RFC says 521 max. Where does 521 come from?

Ah, the RFC says: "1 byte sign-prefix + 64 × 8 bytes padded limbs = 521 bytes max". But limb0 through limbN are the ACTUAL limbs (1 to 64), and then zero_pad fills to 64 limbs. So:
- Actual limbs: N × 8 bytes
- Zero padding: (64 − N) × 8 bytes
- Total limb array: N×8 + (64−N)×8 = 64×8 = 512 bytes
- Plus 1 byte sign prefix = 513 bytes

But RFC says 521 bytes. Let me re-examine...

The RFC §6.11 says: "8 + (64 × 8) = 520 bytes" for the BigIntEncoding header. Wait, 8 (header) + 512 (limbs) = 520 bytes. Plus 1 byte sign-prefix = 521 bytes.

Hmm, but the lexicographic format replaces the 8-byte BigIntEncoding header with a 1-byte sign/limb-count prefix. So the lexicographic key is: 1 byte (sign/limb-count) + 512 bytes (64 limbs × 8 bytes) = 513 bytes, not 521.

But the RFC §6.11 says "521 bytes max". Let me look at this again. The RFC says "length-prefix with sign in byte 0, 64-limb fixed-width padding". And "521 bytes max" — but if the format is 1 + 512 = 513 bytes, where does 521 come from?

Actually, the RFC says in the format description: "521 bytes max" but also says the format is "length-prefix with sign in byte 0, 64-limb fixed-width padding". If we have 64 limbs × 8 bytes = 512 bytes + 1 byte prefix = 513 bytes.

Unless... the 8-byte BigIntEncoding header (version + sign + reserved + num_limbs + reserved) is INCLUDED in the lexicographic key? Let me re-read:

> "Format: `[limb_count_with_sign: u8][limb0: BE][limb1: BE]...[limbN: BE][zero_pad: 8 × (64 − N)]`"

This is a 1-byte prefix + up to 512 bytes of limb data = 513 bytes max. But RFC says 521.

I think the RFC §6.11 has a typo saying "521 bytes max" when it should be "513 bytes max". OR, the format includes the BigIntEncoding header's 8 bytes plus the 512-byte padded limb array plus 1 byte = 521 bytes.

Looking at the RFC §6.11 more carefully:
> "BIGINT lexicographic encoding: BIGINT values have variable length (1–64 limbs), requiring length-prefix encoding for BTree comparison. Format: `[limb_count_with_sign: u8][limb0: BE][limb1: BE]...[limbN: BE][zero_pad: 8 × (64 − N)]` — limbs in big-endian, padded to 64 limbs (512 bytes) total, plus 1 byte sign-prefix = **521 bytes max**."

So the 8-byte BigIntEncoding header IS included in the lexicographic encoding? That would be: 1 (sign/limb-count) + 8 (BigIntEncoding header fields other than limbs?) + 512 (limbs)?

Actually, the BigIntEncoding header is: [version:1][sign:1][reserved:2][num_limbs:1][reserved:3][limb0:8]...

For lexicographic, we're replacing [version:1][sign:1][reserved:2][num_limbs:1][reserved:3] with 1 byte (limb_count_with_sign), and keeping the limb data. So: 1 + 8×N + 8×(64−N) = 513 bytes.

The RFC's "521 bytes max" is inconsistent with the format description. The mission AC correctly says "521 bytes max" which matches the RFC's stated number, even though the RFC's own format description suggests 513 bytes.

I'll flag this as a clarification needed: the RFC has an internal inconsistency (521 vs 513). The mission AC follows the RFC's stated "521 bytes max" but the format notation suggests 513. The implementer should verify against the RFC or RFC-0110.

**Required fix:** Add a clarification note or use the RFC's exact language:

> - Verify encoded key length: 1 byte sign-prefix + 64 × 8 bytes padded limbs = **521 bytes max** (per RFC §6.11 — the implementer should verify this number against RFC-0110 if there is discrepancy with the format description)
>
> OR correct the mission AC to say "513 bytes" if the 8-byte header is NOT included.

---

### C2 · LOW: Reference section missing §4a

**Location:** Reference section

**Problem:** AC-7 implements NUMERIC_SPEC_VERSION header wiring per RFC §4a, but the Reference section does not list §4a.

**Recommended fix:** Update Reference section:

```
## Reference

- RFC-0202-A §4a (NUMERIC_SPEC_VERSION wire format)
- RFC-0202-A §5 (Persistence Wire Format)
- RFC-0202-A §6.10 (BTree index type selection)
- RFC-0202-A §6.11 (Lexicographic key encoding)
- RFC-0202-A §Storage Overhead (521 bytes max for BIGINT serialized)
```

---

## Summary Table (Round 3)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | MODERATE | BIGINT lexicographic 521 vs 513 byte discrepancy (RFC inconsistency) | Add clarification note or verify against RFC-0110 |
| C2 | LOW | Reference section missing §4a | Update Reference section |

---

## Recommendation

Mission 0202-c is **ready to start** after resolving **C1** (clarify the 521-byte max discrepancy — this is a RFC inconsistency that the implementer should flag). C2 is a simple documentation fix.

All round 1 and round 2 fixes are correctly applied and verified.
