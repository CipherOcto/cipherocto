# Adversarial Review: Mission 0202-a-bigint-decimal-typesystem

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-a-bigint-decimal-typesystem.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 1

---

## Executive Summary

Mission 0202-a covers Phase 1 of RFC-0202-A: adding BIGINT and DECIMAL to Stoolap's type system. The acceptance criteria are a correct reading of the RFC Phase 1 checklist. After adversarial review, **four issues are found: one HIGH severity blocking gap, and three MODERATE/LOW observations**. None of the issues prevent starting Phase 1 implementation — all are fixable within the existing scope.

---

## Verification: RFC-0202-A vs Mission Coverage

| RFC-0202-A § | Requirement | Mission AC | Status |
|---|---|---|---|
| §1 DataType discriminants | `Bigint = 13`, `Decimal = 14` | AC-1 | ✅ |
| §6.1 FromStr | `BIGINT`, `DECIMAL`, `NUMERIC` keywords | AC-2 | ✅ |
| §4a from_str_versioned | Migration gate function | AC-3 | ✅ |
| §6.2 Display | `BIGINT`, `DECIMAL` display | AC-4 | ✅ |
| §6.3 is_numeric | Include Bigint \| Decimal | AC-5 | ✅ |
| §6.3 is_orderable | Include Bigint \| Decimal | AC-6 | ✅ |
| §6.3 from_u8 | Entries for 13, 14 | AC-7 | ✅ |
| §4a NUMERIC_SPEC_VERSION | Constant = 2 | AC-8 | ✅ |

All RFC requirements are covered. No missed items.

---

## NEW ISSUES

### C1 · HIGH: Adding BigInt/Decimal to `is_numeric()` creates latent `as_float64()` panic before Phase 3

**Location:** AC-5 (is_numeric update)

**Problem:** RFC-0202-A §6.12 explicitly warns:

> "The existing `Value::compare()` cross-type numeric path uses `as_float64().unwrap()` which **panics** for Extension-based numeric types (BIGINT, DECIMAL, DFP, Quant). Adding BIGINT/DECIMAL to `is_numeric()` triggers this panic for any cross-type comparison like `WHERE bigint_col > 42`."

Phase 1 (this mission) adds BigInt and Decimal to `is_numeric()`. Phase 3 (mission 0202-d) implements the safe cross-type comparison dispatch that avoids the panic. **Between these phases, any query that performs cross-type comparison involving BIGINT or DECIMAL (e.g., `WHERE bigint_col > 42` or `SELECT bigint_col = float_col`) will panic.**

The current codebase already has this panic for DFP/DQA (both already in `is_numeric()`). Adding BigInt/Decimal compounds the problem without the fix.

**Required fix:** Add an acceptance criterion or a note in the Mission Notes section:

> **Latent panic warning:** Adding BigInt/Decimal to `is_numeric()` enables the `is_numeric()` branch in `Value::compare()` for these types before Phase 3 implements safe cross-type comparison. During Phase 1-2, any cross-type comparison of BigInt/Decimal with other numeric types (e.g., `WHERE bigint_col > 42`) will panic via `as_float64().unwrap()`. This is a pre-existing issue for DFP/DQA. Phase 3 (mission 0202-d) resolves it by adding type-specific comparison dispatch before `as_float64()` is reached. **No test that exercises cross-type comparison with BigInt/Decimal types should be written or run until Phase 3 is complete.**

---

### C2 · MODERATE: `from_str_versioned()` is specified in persistence layer context but mission scope is types.rs

**Location:** AC-3, Location field

**Problem:** RFC-0202-A §4a specifies `from_str_versioned()` as a DDL-replay and schema-loading function — it reads the `NUMERIC_SPEC_VERSION` from the WAL/snapshot header and routes keywords accordingly. The function lives in the persistence/schema loading path, not in `types.rs`.

The mission lists `src/core/types.rs` and `src/storage/mvcc/persistence.rs` as locations. The `from_str_versioned()` function should be implemented in `persistence.rs` (or a dedicated schema module), with the `NUMERIC_SPEC_VERSION` constant imported into `types.rs` if needed by both.

Additionally, the `FromStr` for `DataType` (AC-2) is correctly in `types.rs`. But AC-3 conflates two concerns: the constant definition (types.rs or persistence.rs) and the version-gated dispatch function (persistence.rs).

**Required fix:** Split AC-3 into two distinct items:
- AC-3a: Add `NUMERIC_SPEC_VERSION: u32 = 2` constant (RFC §4a specifies it in persistence.rs; types.rs can import it)
- AC-3b: Add `fn from_str_versioned(s: &str, spec_version: u32) -> Result<DataType, Error>` in `src/storage/mvcc/` (not types.rs) — document that it is called during WAL replay before the version header is upgraded

Or: clarify the Location field to specify `persistence.rs` for AC-3 and add a note that the function is persistence-layer infrastructure, not a types.rs concern.

---

### C3 · LOW: Phase 1 acceptance criteria omit unit tests for the new type system additions

**Location:** Acceptance Criteria (all)

**Problem:** AC-1 through AC-8 specify implementation items but do not mention tests. The existing `types.rs` has comprehensive unit tests (lines 510–522: `test_datatype_is_numeric`, `test_datatype_is_orderable`; lines 536–547: `test_datatype_u8_conversion`). After adding Bigint and Decimal:

- `test_datatype_is_numeric` must be updated to assert `Bigint.is_numeric()` and `Decimal.is_numeric()` return true
- `test_datatype_is_orderable` must be updated to assert both return true
- `test_datatype_u8_conversion` must be updated to cover `from_u8(13)` and `from_u8(14)`
- New FromStr tests must cover `"BIGINT"`, `"DECIMAL"`, `"NUMERIC(10,2)"`, `"NUMERIC"` (all returning the new types)
- `Display` tests must cover `Bigint.display()` → "BIGINT" and `Decimal.display()` → "DECIMAL"

**Required fix:** Add to Acceptance Criteria:

> - [ ] Unit tests updated in `src/core/types.rs`:
>   - `test_datatype_is_numeric`: add `Bigint` and `Decimal` assertions
>   - `test_datatype_is_orderable`: add `Bigint` and `Decimal` assertions
>   - `test_datatype_u8_conversion`: add `from_u8(13)` and `from_u8(14)` cases
>   - `test_datatype_display`: add `"BIGINT".parse() → Bigint` and `"DECIMAL".parse() → Decimal`
>   - `test_datatype_from_str`: add `"DECIMAL(10,2)`.parse()` and `"NUMERIC".parse()` → Decimal

This ensures Phase 1 implementation is test-verified at the typesystem level.

---

### C4 · LOW: `to_uppercase()` Unicode behavior is a latent pre-existing bug, not introduced by Phase 1

**Location:** AC-2 (FromStr update), RFC-0202-A §6.1

**Observation:** RFC-0202-A specifies `.to_uppercase()` for case-insensitive keyword matching. In Rust, `to_uppercase()` handles Unicode and can change string length (e.g., German `ß` → `"SS"`, Greek sigma final `ς` → `Σ`). For pure-ASCII SQL keywords this has no practical effect, but it is technically incorrect Unicode handling.

This is a pre-existing issue in the codebase (all existing `FromStr` uses `to_uppercase()`), not introduced by Phase 1. However, if a future Unicode-aware keyword extension is added, this pattern would need to change.

**No action required for Phase 1.** This is a pre-existing design choice in Stoolap. Documented here as a known limitation.

---

## Observations (no action required)

### O1: Decimal Display limitation not mentioned in mission

RFC-0202-A §6.2 notes:

> `DataType::Decimal.display()` always outputs `"DECIMAL"` — precision and scale are not included. For `SHOW CREATE TABLE` or schema dump/restore, use `SchemaColumn.decimal_scale` directly.

This is a known limitation that implementers should be aware of. The mission's AC-4 (Display update) implements the Display trait but does not mention this caveat. Consider adding a note in Mission Notes.

### O2: Discriminant numbering in codebase diverges from RFC comment

The RFC comment in `types.rs` says:
```
// Note: 10 = Blob (RFC-0201), 8 = DeterministicFloat (RFC-0104), 9 = Quant (RFC-0105)
// 11 = unused DataType discriminant
// 12+ available
```

But `DataType::Blob = 10` in the enum (line 67), so discriminant 10 is already used. The RFC-0201 comment appears stale — the actual discriminants are 0-10 in use. New discriminants 13 (Bigint) and 14 (Decimal) are correct per RFC.

**No action required in this mission.** The mission implements the RFC correctly. The stale comment in `types.rs` is a separate cleanup task.

### O3: `from_str_versioned()` handles `spec_version < 2` uniformly

The RFC specifies:
```rust
if spec_version < 2 {
    // Legacy behavior
} else {
    // New behavior
}
```

This means spec_version = 0, 1, or any value < 2 falls through to legacy. The default for new databases is 2. The mission correctly implements this. No issue.

### O4: Mission lists WAL header integration as a dependency but Phase 1 only needs the constant

Mission says:
> Dependencies: Mission 0110-wal-numeric-spec-version (open) — WAL header integration

This is correct — `from_str_versioned()` needs `NUMERIC_SPEC_VERSION` to exist (which is added in AC-8), but it does not need the WAL header read/write wiring yet. That wiring is in mission 0110-wal-numeric-spec-version. The dependency chain is correct.

### O5: `as_bigint()` and `as_decimal()` extractors belong to mission 0202-b, not 0202-a

Mission 0202-a correctly omits `as_bigint()` and `as_decimal()` from its AC — those are Value layer concerns specified in RFC-0202-A §2 (Value Type Extension) and implemented in mission 0202-b. The scope boundary between 0202-a and 0202-b is correct.

---

## Summary Table

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | HIGH | Adding BigInt/Decimal to is_numeric() creates latent as_float64() panic before Phase 3 | Add latent panic warning to mission notes |
| C2 | MODERATE | from_str_versioned() is persistence-layer, not types.rs | Split AC-3 into AC-3a/AC-3b; specify correct module location |
| C3 | LOW | Acceptance criteria omit unit tests for new type system additions | Add test coverage to AC |
| C4 | LOW | to_uppercase() Unicode behavior (pre-existing, not Phase 1) | No action required |

---

## Recommendation

Mission 0202-a is ready to start after resolving **C1** (add panic warning to notes) and **C2** (clarify AC-3 location). **C3** (add tests to AC) is a recommended improvement but not blocking — the types.rs test suite structure makes it obvious what tests need updating.

The mission is well-scoped and correctly derives from RFC-0202-A Phase 1 checklist. The issues found are engineering discipline issues (missing test coverage, ambiguous scope boundary) rather than specification errors.
