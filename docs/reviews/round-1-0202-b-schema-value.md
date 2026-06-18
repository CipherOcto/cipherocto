# Adversarial Review: Mission 0202-b-bigint-decimal-schema-value

**Reviewed by:** @agent (adversarial review)
**Date:** 2026-04-11
**Mission:** `missions/open/0202-b-bigint-decimal-schema-value.md`
**RFC:** RFC-0202-A (Storage) — BIGINT and DECIMAL Core Types
**Round:** 1

---

## Executive Summary

Mission 0202-b covers Phase 1b of RFC-0202-A: SchemaColumn extension (decimal_scale), Value constructors/extractors, from_typed with Result semantics, coercion, and comparison. The mission is a correct reading of the RFC §6.4–§6.9 acceptance criteria. After adversarial review, **five issues are found: two HIGH severity (blocking gaps and specification mismatches), two MODERATE, and one LOW**. Several acceptance criteria need splitting or clarification before implementation can proceed safely.

---

## Verification: RFC-0202-A vs Mission Coverage

| RFC § | Requirement | Mission AC | Status |
|---|---|---|---|
| §6.4 as_string | BIGINT/DECIMAL Extension → numeric string | Implicit (via Display) | ⚠️ Missing explicit AC |
| §6.4 Display | `Value::Display` for BIGINT/DECIMAL | AC-10 | ✅ |
| §6.5 NULL | Null extractor behavior | Implicit | ✅ (standard pattern) |
| §6.6 compare_same_type | Full ordering for BIGINT/DECIMAL | AC-11 | ⚠️ Missing wildcard arm note |
| §6.7 coercion | Coercion hierarchy implemented | AC-8 | ⚠️ Incomplete spec |
| §6.7 cast | Trap cases (BIGINT→INTEGER, DECIMAL→BIGINT) | AC-9 | ⚠️ Error type unspecified |
| §6.8 from_typed | Result semantics per type | AC-7 | ✅ |
| §6.8a stoolap_parse_decimal | Standalone parser function | Not in AC | ❌ Missing |
| §6.9 SchemaColumn | decimal_scale: Option<u8>, builder | AC-1, AC-2 | ✅ |
| §6.12 compare | Cross-type comparison | Not in scope | N/A (Phase 3) |

---

## NEW ISSUES

### C1 · HIGH: `as_decimal()` fixed slice vs variable-length Extension data

**Location:** AC-6 (as_decimal extractor), RFC §2 (Value constructors)

**Specification mismatch:** The RFC Value constructors (§2) show:

```rust
// Constructor for DECIMAL:
pub fn decimal(d: Decimal) -> Self {
    let encoding = decimal_to_bytes(&d);  // returns [u8; 24]
    let mut bytes = Vec::with_capacity(1 + 24);
    bytes.push(DataType::Decimal as u8); // tag 14
    bytes.extend_from_slice(&encoding);    // exactly 24 bytes
    Value::Extension(CompactArc::from(bytes))
}
```

And the extractor:
```rust
pub fn as_decimal(&self) -> Option<Decimal> {
    Value::Extension(data) if data.first() == Some(DataType::Decimal as u8) => {
        let encoding_bytes: [u8; 24] = data[1..25].try_into().ok()?; // exactly 24 bytes
        decimal_from_bytes(encoding_bytes).ok()
    }
}
```

The DECIMAL constructor writes exactly 24 bytes and the extractor reads exactly `data[1..25]`. This is internally consistent.

However, the mission's AC-3 (`Value::bigint()`) uses `b.serialize().to_bytes()` but the RFC specifies `BigIntEncoding::to_bytes()`. More critically, the BIGINT extractor uses `&data[1..]` (variable-length) while the mission AC does not specify this. The issue: **the mission does not call out that BIGINT is variable-length and the extractor must handle `&data[1..]` without a fixed bound**, while DECIMAL is fixed at 24 bytes.

**Required fix:** Add a note to AC-3 and AC-5:
> - BIGINT is **variable-length** (1–520 bytes of payload). The `as_bigint()` extractor must use `&data[1..]` (unbounded slice) and pass to `BigInt::deserialize()` which validates exact byte count from the header. Do NOT use a fixed slice bound like `data[1..521]`.
> - DECIMAL is **fixed-length** (24 bytes of payload). The `as_decimal()` extractor uses `data[1..25].try_into()` — the `[u8; 24]` conversion enforces the fixed length.

This distinction is already in the RFC §2 notes but the mission AC does not surface it.

---

### C2 · HIGH: `stoolap_parse_decimal()` is not in the acceptance criteria

**Location:** Mission missing entirely, RFC §6.8a specifies this function

**Problem:** RFC-0202-A §6.8a specifies `stoolap_parse_decimal()` as a **standalone parser function** that Stoolap must implement — it is NOT provided by the determin crate. This function:
- Takes a `&str` input
- Validates format `^[+-]?[0-9]+(\.[0-9]+)?$`
- Rejects scientific notation, bare dots, whitespace-only
- Returns `Result<Decimal, DecimalError>`
- Handles scale computation and mantissa extraction

AC-7 (`from_typed()` for DECIMAL) calls `stoolap_parse_decimal(s)` but this function does not exist yet. It must be implemented in this mission (or explicitly deferred to 0202-b with its own acceptance criterion).

**Required fix:** Add to Acceptance Criteria:

> - [ ] `stoolap_parse_decimal(s: &str) -> Result<Decimal, DecimalError>` implemented in `src/core/value.rs` (or dedicated parser module) per RFC §6.8a specification:
>   - Input format: `^[+-]?[0-9]+(\.[0-9]+)?$`
>   - Rejects: scientific notation, bare dots, whitespace-only strings
>   - Returns `DecimalError::InvalidScale` if fractional digits > 36
>   - Returns `DecimalError::ParseError` for malformed input
>   - Returns `DecimalError::Overflow` if mantissa exceeds i128 range (>38 digits)

---

### C3 · MODERATE: `cast_to_type` traps use unspecified error types

**Location:** AC-9 (cast_to_type for BIGINT→INTEGER and DECIMAL→BIGINT), RFC §6.7

**Problem:** RFC-0202-A §6.7 specifies:

| BIGINT → INTEGER | `TryFrom<BigInt>` | TRAP if out of i64 range |
| DECIMAL → INTEGER | Via BIGINT | TRAP if scale > 0 or out of range |

The mission's AC-9 says "updated for explicit CAST (BIGINT→INTEGER trap, DECIMAL→BIGINT trap)" but does not specify the exact error types to use. The RFC notes that `BigIntError::OutOfRange` should be used for BIGINT overflow (verified in stoolap error.rs during Round 7 review). But what about:
- DECIMAL→BIGINT when scale > 0? The coercion table says "TRAP" but the error is unspecified
- DECIMAL→BIGINT when value overflows i64 range?

**Required fix:** Add to Acceptance Criteria or Mission Notes:

> - BIGINT→INTEGER cast: uses `i64::try_from(&BigInt)` returning `BigIntError::OutOfRange` on overflow (per RFC §6.7 and error.rs verification)
> - DECIMAL→BIGINT cast: blocked in this RFC (RFC-0202-B scope). Return `Error::NotSupported("DECIMAL → BIGINT requires RFC-0202-B (not yet implemented)")` — do NOT return NULL (silent failure blocked per RFC)

Alternatively, split AC-9 into:
> - AC-9a: `cast_to_type` for BIGINT→INTEGER implemented with `BigIntError::OutOfRange`
> - AC-9b: `cast_to_type` for DECIMAL→BIGINT returns `Error::NotSupported` (blocked by RFC-0202-B)

---

### C4 · MODERATE: `coerce_to_type` for DECIMAL→INTEGER coercion path is underspecified

**Location:** AC-8 (coerce_to_type), RFC §6.7 coercion table

**Problem:** The coercion table in RFC-0202-A §6.7 says:

```
DECIMAL → INTEGER | Via BIGINT | TRAP if scale > 0 or out of range
```

The "via BIGINT" means DECIMAL→INTEGER requires two steps:
1. DECIMAL → BIGINT (blocked by RFC-0202-B, currently returns `Error::NotSupported`)
2. BIGINT → INTEGER (i64::try_from)

If the DECIMAL has non-zero scale (e.g., `DECIMAL '123.45'`), step 1 is blocked. The RFC says this is "a known temporary inconsistency" and both paths will be resolved in RFC-0202-B.

The mission AC-8 says "`coerce_to_type()` / `into_coerce_to_type()` updated for BIGINT/DECIMAL coercion hierarchy" but does not specify that:
- DECIMAL→BIGINT coercion currently returns `Error::NotSupported` (not NULL, not a coercion)
- This means `coerce_to_type()` can return an **error**, not just NULL, for DECIMAL→INTEGER

This diverges from the existing coerce_to_type contract (which returns NULL on coercion failure, not errors). The RFC changes this contract for DECIMAL→INTEGER.

**Required fix:** Add to Mission Notes:

> **Coercion contract deviation:** RFC-0202-A §6.7 changes the coerce_to_type contract for DECIMAL→INTEGER. Existing coerce_to_type returns NULL on failure. DECIMAL→INTEGER via BIGINT currently returns `Error::NotSupported` (not NULL) because the intermediate DECIMAL→BIGINT step is blocked by RFC-0202-B. This is intentional per RFC — "silent coercion failure would cause data correctness issues." Callers should handle both `Value::Null` and `Error` returns from coerce_to_type for DECIMAL→INTEGER during Phase 1-2.

---

### C5 · LOW: `as_string()` for BIGINT/DECIMAL is not an explicit acceptance criterion

**Location:** AC-10 (Display), RFC §6.4

**Observation:** AC-10 covers `Value::Display` but RFC §6.4 specifies `as_string()` as a separate method from `Display`. The Display implementation uses `as_bigint().to_string()` and `decimal_to_string()` respectively — Display calls the extractors. The `as_string()` method is used for string coercion and formatting, distinct from `fmt::Display`.

In the existing codebase, `as_string()` and `Display` are related but separate: `as_string()` is for programmatic string conversion, `Display` is for `{}` formatting. Both need BIGINT/DECIMAL cases.

**Recommended fix:** Add to AC-10:

> - [ ] `as_string()` for BIGINT/DECIMAL Extension values (per RFC §6.4 — distinct from Display, uses same extractor pattern)
>   - BIGINT: `as_bigint().map(|bi| bi.to_string())`
>   - DECIMAL: `as_decimal().and_then(|d| decimal_to_string(&d).ok())`

This is LOW because the Display implementation in AC-10 naturally leads to implementing as_string similarly, but making it explicit prevents oversight.

---

### C6 · LOW: `compare_same_type()` wildcard arms not explicitly mentioned in AC

**Location:** AC-11 (compare_same_type), RFC §6.6

**Observation:** AC-11 says "updated for BIGINT/DECIMAL full ordering (calls BigInt::compare and decimal_cmp)". The RFC specifies that the match arms must include:

```rust
n => { debug_assert!(false, "unexpected BigInt::compare result: {}", n); Ordering::Greater }
```

This was added in RFC-0202-A v1.20 (Round 9, N3 fix). While the AC's wording "calls BigInt::compare and decimal_cmp" implies the full implementation, making the wildcard arm explicit prevents a later implementer from simplifying to just the three explicit arms.

**Recommended fix:** Add to AC-11:

> - [ ] `compare_same_type()` for BIGINT and DECIMAL, including wildcard `n => debug_assert!(false, ...) arms for both match blocks (per RFC §6.6 — the wildcard arm handles unexpected return values from BigInt::compare/decimal_cmp gracefully)

---

## Summary Table

| ID | Severity | Issue | Required Action |
|---|---|---|---|
| C1 | HIGH | BIGINT variable-length vs DECIMAL fixed-length distinction not surfaced in AC | Add length distinction note to AC-3/AC-5 |
| C2 | HIGH | `stoolap_parse_decimal()` missing from AC — called by from_typed but not implemented | Add stoolap_parse_decimal to AC as separate criterion |
| C3 | MODERATE | cast_to_type traps use unspecified error types | Split AC-9 into 9a (BIGINT→INTEGER with OutOfRange) and 9b (DECIMAL→BIGINT with NotSupported) |
| C4 | MODERATE | DECIMAL→INTEGER via BIGINT returns Error, not NULL — contract deviation | Add contract deviation note to Mission Notes |
| C5 | LOW | as_string() not explicit in AC (separate from Display) | Add as_string() to AC or Notes |
| C6 | LOW | compare_same_type wildcard arms not explicit in AC | Add wildcard arm note to AC-11 |

---

## Inter-Mission Dependency Note

Mission 0202-b produces the `from_typed()` and coercion infrastructure that mission 0202-d (Phase 3 VM) depends on. Specifically:
- `stoolap_parse_decimal()` (C2) is needed by the SQL parser integration in 0202-d
- `cast_to_type` error semantics (C3) affect how the VM handles CAST operations

The 0202-b → 0202-d dependency chain should be noted so implementers of 0202-d know to re-review 0202-b's implementation of these items when 0202-d begins.

---

## Recommendation

Mission 0202-b is **not ready to start** without resolving **C1** (add length distinction note) and **C2** (add stoolap_parse_decimal to AC). C1 is a clarification that can be handled in PR review; C2 is a missing specification item that must be added before implementation begins — without it, the implementer will discover at PR time that the parser function doesn't exist.

C3 and C4 are specification gaps in the RFC itself that the mission inherits — the implementer should follow the RFC as-specified and note any ambiguity to the RFC maintainer.

After C1 and C2 are addressed, the mission is implementable. C3/C4/C5/C6 are improvements that make the acceptance criteria unambiguous but don't block implementation.
