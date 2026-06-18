# Adversarial Review: Mission 0202-c-bigint-decimal-persistence (Round 2)

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-c-bigint-decimal-persistence.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 2

---

## Executive Summary

Round 1 review identified 5 issues (C1 HIGH, C2/C3 MODERATE, C4/C5 LOW). The review was committed but **the mission file was NOT updated with fixes** — all 5 issues remain unresolved. This Round 2 review:

1. Confirms all round 1 issues remain open
2. Finds **three new issues: one HIGH (deserialize arm ordering), one MODERATE (wire tag 13/14 deserialize needs variable-length BIGINT handling), one LOW (lexicographic test vector specification)**
3. Provides complete fix language for all items

---

## Status of Round 1 Issues (ALL UNRESOLVED)

**Mission file `missions/open/0202-c-bigint-decimal-persistence.md` was NOT updated after round 1 review.** All 5 issues from round 1 remain open.

| ID | Severity | Issue | Status |
|---|---|---|---|
| C1 | HIGH | Lexicographic encoding verification not in AC | ❌ Still missing |
| C2 | MODERATE | NUMERIC_SPEC_VERSION wiring underspecified | ❌ Still underspecified |
| C3 | MODERATE | Debug assertion placement ambiguous | ❌ Still ambiguous |
| C4 | LOW | REINDEX documentation vague | ❌ Still vague |
| C5 | LOW | serialize_value arm ordering not explicit | ❌ Still missing |

---

## NEW ISSUES (Round 2)

### C2-R2 · HIGH: Wire tag ordering in `deserialize_value` is unspecified

**Location:** AC-1 to AC-4, RFC §5

**Problem:** Round 1 C5 (serialize arm ordering) was identified but the mission doesn't mention that `deserialize_value` also requires proper arm ordering. The RFC §5 deserialize implementation shows wire tags 13 and 14 handled as dedicated cases. If the generic Extension handler (tag 11) appears before the 13/14 cases, it would incorrectly consume BIGINT/DECIMAL wire bytes as a generic Extension.

The `deserialize_value` function reads a wire tag byte first, then dispatches. The structure must be:

```rust
match tag {
    1 => /* Boolean */,
    2 => /* Integer */,
    // ...
    13 => /* BIGINT — must appear before generic Extension (tag 11) */,
    14 => /* DECIMAL — must appear before generic Extension (tag 11) */,
    11 => /* Generic Extension — fallback only */,
    // ...
}
```

If tag 11 appears before 13/14, a BIGINT value (tag 13) would be misread as a generic Extension.

**Required fix:** Add to Mission Notes:

> **deserialize arm ordering:** Wire tag 13 (BIGINT) and 14 (DECIMAL) handlers MUST appear **before** the generic Extension (tag 11) handler in `deserialize_value`. The generic Extension handler would otherwise consume BIGINT/DECIMAL wire bytes as malformed data. This is the same ordering principle as `serialize_value` (round 1 C5).

---

### C3-R2 · MODERATE: BIGINT deserialize must read variable-length header correctly

**Location:** AC-3 (deserialize wire tag 13), RFC §5

**Problem:** RFC §5 specifies the BIGINT deserialize handler:

```rust
13 => {
    // BIGINT: variable-length — must read header to determine exact byte count
    if rest.len() < 8 {
        return Err(Error::internal("truncated bigint header"));
    }
    let num_limbs = rest[4] as usize;
    let total = 8 + num_limbs * 8;
    if rest.len() < total {
        return Err(Error::internal("truncated bigint data"));
    }
    let big_int = BigInt::deserialize(&rest[..total])
        .map_err(|e| Error::internal(format!("bigint deserialization: {:?}", e)))?;
    Ok(Value::bigint(big_int))
}
```

The mission AC-3 says "Wire tag 13 handler added to `deserialize_value` reconstructing BigInt from BigIntEncoding" but doesn't specify the variable-length handling requirements:
- Must check minimum 8 bytes for header
- Must read `num_limbs` from byte offset 4
- Must compute total size and bounds-check before passing to `deserialize()`

If an implementer passes the entire `rest` slice to `BigInt::deserialize()`, the deserializer would read garbage beyond the BIGINT data (from whatever follows in the buffer), causing corruption or wrong values.

**Required fix:** Expand AC-3:

> - [ ] Wire tag 13 handler in `deserialize_value`:
>   - Read `num_limbs` from byte offset 4 of the BigIntEncoding header
>   - Compute total size = 8 + num_limbs * 8 bytes
>   - Bounds-check: return `Error::internal("truncated bigint data")` if `rest.len() < total`
>   - Slice `&rest[..total]` before passing to `BigInt::deserialize()` — caller must advance buffer by `total` bytes
>   - Do NOT pass entire `rest` slice to deserialize — data beyond the BIGINT payload would be misread

---

### C4-R2 · LOW: Lexicographic encoding test vectors should be specified

**Location:** AC-8/AC-9 (lexicographic encoding), RFC §6.11

**Observation:** Round 1 C1 identified that lexicographic encoding verification is missing. The fix should include specific test vectors from RFC §6.11:

For BIGINT lexicographic ordering (per RFC §6.11 examples):
- `BIGINT '-2^64'` → `[7E][7F...FF][zero_pad×62]` (most negative)
- `BIGINT '-1'` → `[7F][7F...FF][zero_pad×63]` (negative, 1 limb)
- `BIGINT '0'` → `[81][00...00][zero_pad×63]` (zero)
- `BIGINT '1'` → `[81][00...01][zero_pad×63]` (positive, 1 limb)
- `BIGINT '2^64'` → `[82][80...0001][zero_pad×62]` (positive, 2 limbs)

For DECIMAL lexicographic sign-flip (per RFC §6.11):
- Zero mantissa encodes as `0x80...00` (sign-bit set, magnitude zero) — sorts between negatives and positives
- Sign-flip: XOR byte 0 of mantissa with `0x80`

**Recommended fix:** Add to the lexicographic verification AC (C1 fix):

> - [ ] BIGINT lexicographic verification test vectors:
>   - Verify: `-2^64 < -1 < 0 < 1 < 2^64` in encoded key space
>   - Verify: encoded key length is 521 bytes (1 byte sign-prefix + 64 × 8 bytes padded limbs)
> - [ ] DECIMAL lexicographic verification test vectors:
>   - Verify: `DECIMAL '-12.3'` encodes with sign-bit flipped in byte 0
>   - Verify: zero mantissa encodes as `0x80...00` and sorts between negatives and positives
>   - Verify: scale byte appended as BE u8 at byte 23

---

## Summary Table (Round 2)

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 (R1) | HIGH | Lexicographic encoding verification not in AC | ✅ Still open — apply round 1 fix |
| C2 (R1) | MODERATE | NUMERIC_SPEC_VERSION wiring underspecified | ✅ Still open — apply round 1 fix |
| C3 (R1) | MODERATE | Debug assertion placement ambiguous | ✅ Still open — apply round 1 fix |
| C4 (R1) | LOW | REINDEX documentation vague | ✅ Still open — apply round 1 fix |
| C5 (R1) | LOW | serialize arm ordering not explicit | ✅ Still open — apply round 1 fix |
| C2-R2 | HIGH | deserialize arm ordering unspecified | Add deserialization ordering requirement to Mission Notes |
| C3-R2 | MODERATE | BIGINT deserialize variable-length not specified | Expand AC-3 with bounds-check and slice requirements |
| C4-R2 | LOW | Lexicographic test vectors unspecified | Add specific test vectors from RFC §6.11 |

---

## Priority: Apply Round 1 Fixes First

The mission file must be updated with round 1 fixes before round 2 items can be meaningfully assessed. Round 1 issues C1 (HIGH) and C2 (MODERATE) are the most critical blocking items — lexicographic verification is blocking for production per the RFC.

---

## Recommendation

Mission 0202-c is **not ready to start** — all round 1 issues remain unfixed and two new issues were found. The mission file must be updated with:
1. Round 1 fixes (C1-C5)
2. Round 2 fixes (C2-R2: deserialize ordering, C3-R2: variable-length BIGINT deserialize)

Priority order: Apply C1 (HIGH) + C2 (R1 MODERATE) first, then C2-R2 (HIGH) for deserialize, then the remaining items.
