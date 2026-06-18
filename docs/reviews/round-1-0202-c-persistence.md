# Adversarial Review: Mission 0202-c-bigint-decimal-persistence

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-c-bigint-decimal-persistence.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 1

---

## Executive Summary

Mission 0202-c covers Phase 2 of RFC-0202-A: persistence wire format (wire tags 13/14), BTree index type selection, NUMERIC_SPEC_VERSION header wiring, and lexicographic key encoding. The mission is a correct identification of Phase 2 implementation items. After adversarial review, **four issues are found: one HIGH severity (blocking verification gap), two MODERATE, and one LOW**. The HIGH issue must be resolved before Phase 2 is considered complete.

---

## Verification: RFC-0202-A vs Mission Coverage

| RFC § | Requirement | Mission AC | Status |
|---|---|---|---|
| §5 Wire tags | serialize/deserialize arms for 13/14 | AC-1 to AC-4 | ✅ |
| §5 Debug assertion | Catch 13/14 reaching generic Extension | AC-5 | ⚠️ Ambiguous placement |
| §6.10 BTree index | auto_select_index_type → BTree | AC-6 | ✅ |
| §4a NUMERIC_SPEC_VERSION | WAL/snapshot header wiring | AC-7 | ⚠️ Dependency spec unclear |
| §6.11 BIGINT lexicographic | 521-byte fixed-width padded format | AC-8 | ⚠️ Missing verification criterion |
| §6.11 DECIMAL lexicographic | Sign-flip + scale byte format | AC-9 | ⚠️ Missing specification detail |
| §6.11 REINDEX | Rebuild existing indexes | AC-10 | ⚠️ Scope unclear |

---

## NEW ISSUES

### C1 · HIGH: Lexicographic encoding verification is not an acceptance criterion

**Location:** AC-8 and AC-9, RFC §6.11

**Problem:** RFC-0202-A §6.11 states explicitly:

> **"Required implementation item: Add a debug assertion (or static compile-time check) in `serialize_value` that verifies wire tag 13/14 values never reach the generic Extension branch. This is not optional — without it, a future contributor could silently reorder the match arms and cause a 5-byte-per-value storage overhead regression."**

But more critically, the RFC header for Phase 2 says:

> "Add persistence layer support for BIGINT and DECIMAL... **Production deployment is blocked until lexicographic encoding is verified.**"

The mission's AC-8 and AC-9 say "implemented" but do not include any verification criterion. The RFC's own text identifies this as a **blocking** item for production, yet the mission has no explicit test or verification step for the lexicographic encoding.

**Required fix:** Add to Acceptance Criteria:

> - [ ] **Lexicographic encoding verification** (blocking for production per RFC §6.11):
>   - BIGINT: verify ordering: negative < zero < positive; limb-by-limb big-endian comparison within same sign
>   - DECIMAL: verify sign-flip XOR encoding; verify zero mantissa sorts between negatives and positives
>   - Verify 64-limb fixed-width padding for BIGINT (521 bytes max)
>   - Verify scale byte appended as BE u8 for DECIMAL
>   - Document verification results (test output or assertion logs confirming correct ordering)

Without this, Phase 2 is implemented but unverified — blocking for production deployment per the RFC's own admission.

---

### C2 · MODERATE: NUMERIC_SPEC_VERSION header wiring AC is underspecified

**Location:** AC-7, RFC §4a

**Problem:** AC-7 says "`NUMERIC_SPEC_VERSION` wired to WAL/snapshot header read/write (see mission 0110-wal-numeric-spec-version)". This AC references a dependency mission but does not specify:
- Where exactly in the WAL header the version is read/written (offset 0, u32 little-endian per RFC §4a)
- How the version upgrade is triggered when a version-1 database executes DDL with new-type keywords
- The atomicity requirement: header upgrade and DDL commit must be in the same WAL transaction

The RFC §4a specifies the wire format in detail (u32 little-endian at offset 0), the upgrade trigger (DDL with BIGINT/DECIMAL keywords), and the atomicity requirement. The mission AC is too vague to guide implementation.

**Required fix:** Expand AC-7:

> - [ ] `NUMERIC_SPEC_VERSION` wired to WAL/snapshot header read/write per RFC §4a:
>   - Read version from bytes 0–3 of WAL segment header (u32 little-endian) on recovery
>   - Write version to same offset on WAL segment creation (default = 2 for new databases)
>   - Header upgrade to version 2 triggered when DDL uses BIGINT/DECIMAL keywords in a version-1 database
>   - Header upgrade and DDL commit occur in the same WAL transaction (atomic)
>   - If WAL segment is corrupt (checksum failure), skip entire segment — no partial replay

---

### C3 · MODERATE: Debug assertion placement is ambiguous

**Location:** AC-5, RFC §6.11

**Problem:** RFC-0202-A §6.11 requires a debug assertion in the generic Extension branch to catch wire tags 13/14 reaching it. However, the RFC's own serialize_value example (§5) shows wire tags 13/14 handled as **dedicated arms** before the generic Extension fallback:

```rust
if tag == DataType::Bigint as u8 {
    buf.push(13); buf.extend_from_slice(payload);
} else if tag == DataType::Decimal as u8 {
    buf.push(14); buf.extend_from_slice(payload);
} else {
    // Tag 11: generic extension...
}
```

With this structure, wire tags 13/14 **cannot** reach the generic branch by construction. The assertion would never fire in correct code. The RFC's phrasing "Required implementation item: Add a debug assertion" suggests the assertion is a defensive check, but it belongs in a different location or form.

**Recommended fix:** Clarify AC-5:

> - [ ] Debug assertion added to catch wire tags 13/14 reaching the generic Extension branch. **Note:** The assertion should be placed in the `serialize_value` match for `Value::Extension` — if the tag is 13 or 14 but the code path reaches the generic Extension arm (indicating a match-arm ordering bug), the assertion fires. Placement should be inside the generic Extension arm, after checking the tag bytes.

Alternatively, if the serialization structure handles 13/14 via dedicated arms before the generic arm (per RFC §5 example), the assertion may be redundant — verify during implementation and update the AC accordingly.

---

### C4 · LOW: REINDEX documentation AC is too vague

**Location:** AC-10, RFC §6.11

**Problem:** AC-10 says "REINDEX documentation added for existing BTree indexes on BIGINT/DECIMAL columns". This is underspecified:
- What documentation? A code comment? User-facing docs? A runbook item?
- Does this mean the REINDEX command must actually support online reindexing with the new encoding?
- Does this apply to existing databases (version-1) that have BIGINT/DECIMAL columns stored as Integer/Float?

**Recommended fix:** Expand AC-10:

> - [ ] REINDEX documentation added for BIGINT/DECIMAL BTree indexes:
>   - Document that existing BTree indexes on BIGINT/DECIMAL columns must be rebuilt after deploying lexicographic encoding
>   - For version-1 databases: existing columns stored as Integer/Float do not need reindexing (only new DDL-created columns use new types)
>   - Recommended migration path: `REINDEX INDEX idx_name` or `CREATE INDEX ... USING btree (col) WITH (encoding = 'lexicographic')` for online migration

---

### C5 · LOW: serialize_value wire tag ordering not explicit in AC

**Location:** AC-1 to AC-4, RFC §5

**Observation:** RFC-0202-A §5 implementation note says:

> "Implementation order: BIGINT/DECIMAL checks MUST appear **before** the generic Extension fallback (tag 11) in the `serialize_value` match chain. If the generic branch matches first, BIGINT/DECIMAL values would be serialized as generic extensions, losing the dedicated wire tags and 5-byte savings."

The mission does not mention this ordering requirement. If the implementer places the generic Extension arm before the BIGINT/DECIMAL arms, wire tags 13/14 would be stored as generic Extensions (tag 11 + sub-tag + length prefix), creating a storage overhead regression.

**Recommended fix:** Add to Mission Notes:

> **Implementation order:** Wire tag 13/14 arms for BIGINT/DECIMAL MUST appear before the generic Extension arm (tag 11) in `serialize_value`. If the generic arm is placed first, BIGINT/DECIMAL values fall through to it, losing the dedicated wire tag optimization (5 bytes per value). AC-5 (debug assertion) is the defense against this ordering bug.

---

## Summary Table

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | HIGH | Lexicographic encoding verified but no verification criterion in AC | Add explicit verification AC (ordering tests, format checks) |
| C2 | MODERATE | NUMERIC_SPEC_VERSION wiring underspecified | Expand AC-7 with wire format, atomicity, upgrade trigger details |
| C3 | MODERATE | Debug assertion placement ambiguous | Clarify where assertion belongs given RFC §5 arm ordering |
| C4 | LOW | REINDEX documentation vague | Specify what documentation, scope, migration path |
| C5 | LOW | serialize_value arm ordering not explicit | Add implementation order note to Mission Notes |

---

## Inter-Mission Dependencies

- Mission 0202-c depends on mission 0110-wal-numeric-spec-version for the WAL header infrastructure. AC-7 correctly notes this dependency.
- Mission 0202-d (Phase 3 VM) depends on Phase 2 persistence for serialization round-trip verification — gas metering formulas (§8) require correct serialization to compute limb counts and scales.
- Mission 0202-e (Phase 4 integration testing) depends on Phase 2 for round-trip serialization tests (AC-23, AC-24 in 0202-e).

---

## Recommendation

Mission 0202-c is **conditionally ready** after resolving **C1** (add verification criterion — HIGH, blocking for production) and **C2** (expand NUMERIC_SPEC_VERSION AC). C3-C5 are clarifications that prevent implementation ambiguity.

C1 is the critical blocking item: the RFC explicitly states production deployment is blocked until lexicographic encoding is verified. Without an explicit verification acceptance criterion, Phase 2 implementation cannot be considered complete.
