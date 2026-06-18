# Round 4 Adversarial Review: Mission 0202-b (Schema and Value Layer)

**Reviewer:** @agent
**Date:** 2026-04-11
**Mission:** `missions/open/0202-b-bigint-decimal-schema-value.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 4

---

## Status of Prior Issues

Round 3 found C1 (Ord implementation missing — **FIXED**, Ord AC item added at lines 50-54), C2 (Reference section missing §6.3 and §6.13 — **FIXED**, both now listed), and C3 (as_int64/as_float64 scope — resolved as note in AC redirecting to 0202-b). The current mission file reflects these fixes.

---

## ACCEPTED ISSUES

### A1 · HIGH: Reference section missing §6.10 (Index Type Selection)

**Severity:** HIGH
**Section:** Reference

**Problem:** Reference section lists §6.3, §6.4–§6.9, §6.11, §6.13 but omits **§6.10 (Index Type Selection)**. RFC-0202-A §6.10 specifies `auto_select_index_type()` mapping BIGINT/DECIMAL → BTree. AC-8 in mission 0202-c implements this. The Reference in 0202-b should include it.

**Required fix:** Add to Reference:
```
- RFC-0202-A §6.10 (Index Type Selection — auto_select_index_type for BIGINT/DECIMAL → BTree)
```

---

### A2 · HIGH: Reference section missing §6.12 (Cross-Type Numeric Comparison)

**Severity:** HIGH
**Section:** Reference

**Problem:** Mission Notes (line 75) explicitly references RFC-0202-A §6.12 as the cross-type comparison hazard. The Reference section does not list §6.12 despite the hazard being directly relevant to Phase 1-2 implementation risk.

**Required fix:** Add to Reference:
```
- RFC-0202-A §6.12 (Cross-Type Numeric Comparison — is_numeric() update triggers as_float64 panic hazard during Phase 1-2)
```

---

### A3 · HIGH: DECIMAL→INTEGER coercion path not explicit in AC-8

**Severity:** HIGH
**Section:** AC-8 (`coerce_to_type()` / `into_coerce_to_type()`)

**Problem:** AC-8 blocks BIGINT→DECIMAL and BIGINT/DECIMAL→FLOAT, but does not specify what happens for DECIMAL→INTEGER. RFC-0202-A §6.7 says DECIMAL→INTEGER goes via BIGINT (blocked at DECIMAL→BIGINT by RFC-0202-B). Mission Notes (line 73) acknowledge this returns `Error::NotSupported` but the AC itself does not state this.

**Required fix:** Add to AC-8:
```
- DECIMAL→INTEGER: blocked via DECIMAL→BIGINT (RFC-0202-B scope); returns `Error::NotSupported("DECIMAL → INTEGER requires RFC-0202-B")` when scale > 0 or value out of i64 range
```

---

### A4 · MODERATE: AC-1/AC-2 not independently verifiable

**Severity:** MODERATE
**Section:** AC-1, AC-2

**Problem:** AC-1 and AC-2 are bundled in a single checkbox ("`SchemaColumn.decimal_scale: Option<u8>` field and `SchemaBuilder::set_last_decimal_scale()` builder method added"). Partial implementation (field without builder method, or vice versa) would satisfy the combined AC.

**Required fix:** Split into two independent AC items:
```
- [ ] `SchemaColumn.decimal_scale: Option<u8>` field added
- [ ] `SchemaBuilder::set_last_decimal_scale()` builder method added with correct consuming-builder signature
```

---

### A5 · MODERATE: as_int64() expression uses imprecise notation

**Severity:** MODERATE
**Section:** AC (`as_int64()` extractor)

**Problem:** AC says `BigInt::try_from(&bi).ok()`. RFC-0202-A §6.13 uses `i64::try_from(bi).ok()`. The `BigInt::try_from(&bi)` form is not valid syntax — `try_from` is a trait method called as `i64::try_from(&bi)`. The AC could confuse an implementer.

**Required fix:** Change to `i64::try_from(&bi).ok()` to match RFC §6.13 notation precisely.

---

### A6 · MODERATE: RFC-0110 §8 DIGM/MOD gas formula inconsistency

**Severity:** MODERATE
**Section:** Mission Notes (gas references)

**Problem:** RFC-0110 §8 states DIGM formula as `50 + 3 × limbs_a × limbs_b`. For 64 limbs: `50 + 12,288 = 12,338`. But the Worst-Case Proof table shows **12,362** under MAX column for DIV/MOD. The text says "50 + 3×4096 = 12,362" — but `3×4096 = 12,288`, not 12,362. There is a 74-gas discrepancy in RFC-0110.

This is a pre-existing RFC bug, not a mission error. However, the mission references RFC-0110 §8 for gas values. If the RFC has an arithmetic error, implementation could be inconsistent.

**Required fix:** Flag RFC-0110 §8 for correction. 0202-d should not implement 12,362 (the table value) when the formula gives 12,338 — the formula should win as the normative definition.

---

### A7 · LOW: Reference §6.11 description omits lexicographic encoding

**Severity:** LOW
**Section:** Reference

**Problem:** §6.11 entry says "Ord for Value — BIGINT/DECIMAL lexicographic ordering for BTree indexes" — but RFC-0202-A §6.11 also specifies lexicographic key encoding for BTree indexes (marked blocking for production). The Reference description only captures the Ord fix, not the encoding.

**Required fix:**
```
- RFC-0202-A §6.11 (Ord for Value — BIGINT/DECIMAL numeric ordering; lexicographic key encoding for BTree indexes — blocking for production)
```

---

### A8 · LOW: No aggregate gas AC items

**Severity:** LOW
**Section:** Mission AC (absent)

**Problem:** RFC-0202-A §7a specifies aggregate gas formulas (COUNT, SUM, MIN/MAX, AVG for both BIGINT and DECIMAL). The mission has no AC items for aggregate gas — not even a statement that aggregate operations are out of scope for 0202-b.

**Required fix:** Either add aggregate gas AC items, or add explicit note: "Aggregate operations (COUNT, SUM, MIN, MAX, AVG) are deferred to mission 0202-d. No aggregate gas ACs in this mission."

---

## QUESTIONS

**Q1:** Is the DECIMAL→INTEGER path intentionally AC-silent (Notes-only) because it's blocked by RFC-0202-B? If so, the AC should still explicitly state the blocking behavior, not leave it to Notes.

**Q2:** Does the mission need an explicit "no cross-type comparison tests during Phase 1-2" AC? The hazard is documented in Notes but not AC-verified.

---

## RECOMMENDATIONS

| ID | Severity | Issue | Required Action |
|----|----------|-------|-----------------|
| A1 | HIGH | Reference missing §6.10 | Add §6.10 to Reference |
| A2 | HIGH | Reference missing §6.12 | Add §6.12 to Reference |
| A3 | HIGH | DECIMAL→INTEGER coercion not in AC | Add explicit AC item for DECIMAL→INTEGER blocked path |
| A4 | MODERATE | AC-1/AC-2 not independent | Split into two separate AC checkboxes |
| A5 | MODERATE | as_int64() imprecise notation | Fix to `i64::try_from(&bi).ok()` |
| A6 | MODERATE | RFC-0110 §8 gas discrepancy | Flag RFC-0110 for correction; 0202-d should use formula (12,338), not table (12,362) |
| A7 | LOW | §6.11 description incomplete | Mention lexicographic key encoding in Reference |
| A8 | LOW | No aggregate gas AC items | Clarify scope or add AC items |

---

## Verdict

**Not ready to start.** Three HIGH issues (A1, A2, A3) require fixes before implementation. A3 (DECIMAL→INTEGER coercion) is particularly important — it's the same pattern that caused cross-mission scope errors in round 2. A1 and A2 are straightforward Reference additions. A4–A8 are lower priority but should be resolved before production deployment.