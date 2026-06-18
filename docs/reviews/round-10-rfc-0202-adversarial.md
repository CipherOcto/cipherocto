# Adversarial Review Round 10 — RFC-0202-A (v1.10) / RFC-0202-B (v1.6)

**Reviewer:** @ciphercito  
**Date:** 2026-03-31  
**Method:** Cross-reference every claim against Stoolap codebase (`/home/mmacedoeu/_w/databases/stoolap`) and determin crate (`/home/mmacedoeu/_w/ai/cipherocto/crates/determin`)

---

## Codebase State Verified

| File | Commit/Latest | Key Observation |
|------|---------------|-----------------|
| `stoolap/src/core/types.rs` | `DataType` enum stops at `Blob = 10` | No `Bigint`/`Decimal` variants exist |
| `stoolap/src/core/value.rs` | `FromStr` maps `BIGINT` → `Integer`, `DECIMAL`/`NUMERIC` → `Float` | Confirmed — migration needed |
| `stoolap/src/core/schema.rs` | `SchemaColumn` has `quant_scale: u8`, no `decimal_scale` | Confirmed |
| `stoolap/src/storage/mvcc/persistence.rs` | Wire tags 0–12, no `NUMERIC_SPEC_VERSION` | Confirmed |
| `stoolap/src/executor/expression/vm.rs` | `compare_same_type()` Extension arm: equality-only | **Bug confirmed** |
| `determin/src/lib.rs` | Top-level exports: `decimal_from_bytes`, `decimal_to_bytes`, `Decimal`, `DecimalError`, etc. | 12 functions NOT in `pub use` list |
| `determin/src/bigint.rs:191` | `BigInt::compare()` → `i32` (-1, 0, +1) | Matches RFC |
| `determin/src/decimal.rs:732` | `decimal_cmp()` → `i32` (-1, 0, +1) | Matches RFC |
| `determin/src/decimal.rs:176` | `Decimal::new()` → `Result<Decimal, DecimalError>` | Matches RFC |
| `determin/src/decimal.rs:140-151` | `DecimalError` has 5 variants, **no `ParseError`** | Confirmed — gap acknowledged |

---

## Findings

### C1 — CRITICAL: DFP/DQA Same-Type Ordering Broken in Existing `compare_same_type()`

**Location:** RFC-0202-A §6.6, Stoolap `value.rs:555-566`

**Current code:**
```rust
(Value::Extension(a), Value::Extension(b)) => {
    if a.first() != b.first() { return Err(Error::IncomparableTypes); }
    if a == b { Ok(Ordering::Equal) } else { Err(Error::IncomparableTypes) }
}
```

**Problem:** This returns `IncomparableTypes` for ALL non-equal Extension values, including DFP vs DFP and DQA vs DQA. The RFC adds BIGINT/DECIMAL dispatch to this block but doesn't acknowledge that DFP/DQA are **also broken** for same-type ordering today. A query like `WHERE dfp_col1 > dfp_col2` returns an error.

**Impact:** The RFC's §6.6 code only fixes BIGINT/DECIMAL but leaves DFP/DQA broken. The `is_orderable()` claim at §6.2 that DFP is orderable is already false in production.

**Fix:** The RFC's §6.6 code should also add DFP and DQA dispatch arms (using `compare_dfp()` and `dqa_cmp()` respectively), or at minimum document this as a known pre-existing bug that should be fixed in the same PR.

---

### C2 — CRITICAL: BIGINT/DECIMAL vs DFP/Quant Cross-Type Comparison Will PANIC

**Location:** RFC-0202-A §6.12

**Problem:** The RFC places `IncomparableTypes` guards for BIGINT/DECIMAL vs DFP/Quant at the BOTTOM of the numeric comparison path:

```rust
// RFC proposed placement (§6.12):
if self.data_type().is_numeric() && other.data_type().is_numeric() {
    // ... BIGINT/DECIMAL coercion block ...
    // ... DFP special block ...
    // ... as_float64().unwrap() fallback ...  ← PANICS for Extension types
    
    // IncomparableTypes guards come TOO LATE ↑
    if matches!(self_dt, Bigint | Decimal) && matches!(other_dt, Dfp | Quant) {
        return Err(Error::IncomparableTypes);
    }
}
```

The existing code at `value.rs:496-497`:
```rust
let v1 = self.as_float64().unwrap();  // Returns None for Extension → PANIC
let v2 = other.as_float64().unwrap(); // Returns None for Extension → PANIC
```

`as_float64()` returns `None` for ALL Extension types (DFP, Quant, and the future BIGINT/DECIMAL). The `IncomparableTypes` guards are never reached — a comparison like `WHERE bigint_col > dfp_col` panics.

**Fix:** Move the IncomparableTypes guards for BIGINT/DECIMAL vs DFP/Quant BEFORE the `as_float64()` fallback:

```rust
if self.data_type().is_numeric() && other.data_type().is_numeric() {
    // BIGINT/DECIMAL coercion block (existing RFC code) ...
    
    // IncomparableTypes guard MUST come before as_float64 fallback
    if matches!(self_dt, Bigint | Decimal) && matches!(other_dt, Dfp | Quant)
    || matches!(other_dt, Bigint | Decimal) && matches!(self_dt, Dfp | Quant) {
        return Err(Error::IncomparableTypes);
    }
    
    // DFP special block ...
    // as_float64() fallback ...
}
```

---

### C3 — CRITICAL: `stoolap_parse_decimal` Whitespace Handling Contradicts Spec

**Location:** RFC-0202-A §6.8a

**Spec says (§6.8a format constraints):**
> No leading/trailing whitespace

**Code does (§6.8a implementation):**
```rust
let s = s.trim();  // Line 695 — accepts and strips whitespace
```

These contradict. Either:
- The spec should say "Leading/trailing whitespace is stripped before parsing" (more user-friendly, common in SQL), OR
- The code should NOT trim and reject whitespace (stricter)

**Recommendation:** Change the spec to say whitespace is stripped. SQL parsers typically accept `' 123.45 '` for typed literals.

---

### H1 — HIGH: §6.15 "Compile-Blocking Prerequisites" Claim Is Wrong

**Location:** RFC-0202-A §6.15

**RFC says:**
> "Status: NOT exported — These MUST be added to `determin/src/lib.rs` before Stoolap implementation begins. They are compile-blocking prerequisites."

**Reality:** All 12 listed functions are `pub fn` inside `pub mod decimal` and `pub mod bigint`. They ARE accessible via module paths:

```rust
use octo_determin::decimal::decimal_cmp;     // Works today
use octo_determin::bigint::bigint_shl;       // Works today
```

The existing Stoolap VM already uses this pattern at `vm.rs:33`:
```rust
use octo_determin::dqa::{dqa_add, dqa_div, dqa_mul, dqa_sub, Dqa};
```

**Fix:** Downgrade the language. These are convenience additions for `lib.rs`, not compile-blocking. Change to:
> "Status: Accessible via `octo_determin::decimal::*` module path. Recommend adding to top-level `pub use` for ergonomics. Not compile-blocking — Stoolap can import via module path today."

---

### H2 — HIGH: Ord Fix Doesn't Cover Existing DFP/DQA Types

**Location:** RFC-0202-A §6.11

**Problem:** The RFC correctly identifies that raw byte comparison is wrong for BIGINT/DECIMAL and proposes a fix. But the SAME raw byte comparison at `value.rs:1461` is also wrong for DFP and DQA:

- **DFP:** Uses sign-magnitude encoding. Raw byte comparison treats negative numbers as "greater" because the sign byte `0x01` (negative) > `0x00` (positive). BTree indices for DFP columns produce wrong ordering.
- **DQA:** Uses big-endian i64 + scale. Raw byte comparison treats values with different scales incorrectly.

**Impact:** Production BTree indices on DFP/DQA columns are silently corrupted. The RFC should either fix all Extension types in the Ord impl or document the pre-existing bug with a tracking issue.

**Fix:** Add DFP and DQA dispatch arms to the Ord impl alongside BIGINT/DECIMAL:
```rust
t if t == DataType::DeterministicFloat as u8 => { /* compare_dfp */ }
t if t == DataType::Quant as u8 => { /* dqa_cmp */ }
```

---

### H3 — HIGH: Bare `DECIMAL` (No Parameters) Maps to `decimal_scale=0`

**Location:** RFC-0202-A §6.9

**RFC says:**
```
DECIMAL       → DataType::Decimal, decimal_scale=0
DECIMAL(10)   → DataType::Decimal, decimal_scale=0  (precision only)
DECIMAL(10,2) → DataType::Decimal, decimal_scale=2
```

With `decimal_scale=0`, the INSERT-time rounding rule would round ALL fractional values to integers:
> "values with more decimal places than `decimal_scale` are rounded using `decimal_round(d, decimal_scale, RoundHalfEven)`"

This means `INSERT INTO t(d) VALUES (DECIMAL '123.45')` where column `d` is bare `DECIMAL` would store `123` (scale=0, rounds to integer). This is unexpected — most SQL databases treat bare `DECIMAL` as unconstrained precision.

**Recommendation:** Choose one:
1. Bare `DECIMAL` → `decimal_scale=0` (current: rounds to integer) — document this clearly as a design choice
2. Bare `DECIMAL` → no scale enforcement (set a sentinel value like `255` meaning "no limit") — more SQL-standard
3. Bare `DECIMAL` → `decimal_scale=36` (max scale, accepts everything) — simplest

---

### M1 — MEDIUM: `SchemaColumn::Display` Missing DECIMAL(p,s) Output

**Location:** RFC-0202-A §6.9

The RFC specifies adding `decimal_scale: u8` to `SchemaColumn` and a `set_last_decimal_scale()` builder method. But it doesn't show the `Display` impl update for `SchemaColumn`.

**Current Display impl** (`schema.rs:191-208`):
```rust
if self.data_type == DataType::Quant && self.quant_scale > 0 {
    write!(f, "{} DQA({})", self.name, self.quant_scale)?;
}
```

**Missing:** A parallel branch for DECIMAL:
```rust
if self.data_type == DataType::Decimal && self.decimal_scale > 0 {
    write!(f, "{} DECIMAL(36,{})", self.name, self.decimal_scale)?;
}
```

Without this, `SchemaColumn` Display prints `price DECIMAL` instead of `price DECIMAL(36,2)` for parameterized columns.

---

### M2 — MEDIUM: `decimal_to_string` Error in Display Produces Empty String

**Location:** RFC-0202-A §6.3

**RFC code:**
```rust
return write!(f, "{}", decimal_to_string(&d).unwrap_or_default());
```

`decimal_to_string()` returns `Result<String, DecimalError>`. On error, `unwrap_or_default()` produces an empty string `""`. But the RFC's BIGINT path shows `<invalid bigint>` for deserialization failure. The DECIMAL path should be consistent:

```rust
return write!(f, "{}", decimal_to_string(&d).unwrap_or_else(|_| "<invalid decimal>".to_string()));
```

---

### M3 — MEDIUM: No WAL Header Read/Write Infrastructure for `NUMERIC_SPEC_VERSION`

**Location:** RFC-0202-A §4, §4a

The RFC specifies storing `NUMERIC_SPEC_VERSION` at "Bytes 0–3 of the WAL segment header" but:
1. `WALManager` has no API for reading/writing header fields
2. No code path passes spec version to schema loading during WAL replay
3. The `from_str_versioned()` function requires the spec version, but there's no plumbing to get it from the WAL header to the DDL replay callback

The RFC should specify:
- Which `WALManager` methods to modify
- How recovery passes the spec version to DDL replay
- Where `from_str_versioned()` is called (exact file and function)

---

### M4 — MEDIUM: RFC-0202-B Missing Compiler CAST Integration Details

**Location:** RFC-0202-B Phase 3

Phase 3 says:
> "Compile CAST expressions in `src/executor/expression/compiler.rs`"

But doesn't show how the SQL parser produces AST nodes for `CAST(expr AS BIGINT)` or `CAST(expr AS DECIMAL)`. The compiler needs:
1. AST support for `BIGINT` and `DECIMAL` as cast target types
2. A mapping from `ast::CastTarget::Bigint` → `Op::Cast(DataType::Bigint)`
3. Error handling for invalid cast targets

The RFC should show the compiler match arm, similar to how existing types are compiled.

---

### M5 — MEDIUM: RFC-0202-B Test Vectors Use DQA Typed Literals Without Specification

**Location:** RFC-0202-B §Test Vectors

Test vectors use syntax like `CAST(DQA '12345' AS BIGINT)` and `CAST(BIGINT '42' AS DQA(0))`.

The `DQA '...'` typed literal syntax is acknowledged as not formally specified: "the `DQA '...'` typed literal syntax is used for test clarity but is not yet formally specified in any RFC."

**Problem:** These test vectors are not implementable until DQA typed literal parsing is specified. The RFC should either:
1. Specify `DQA '...'` literal parsing in this RFC (small addition)
2. Use programmatic value construction in test vectors instead (e.g., `Value::quant(Dqa::new(12345, 0))`)
3. Mark these test vectors as blocked on a future RFC

---

### L1 — LOW: RFC-0202-A §6.7 `bigint_to_decimal(i128)` Naming Confusion

The RFC correctly notes the naming confusion: `bigint_to_decimal(value: i128)` takes `i128` not `BigInt`, while the RFC-0202-B function `bigint_to_decimal_full(b: BigInt, scale: u8)` takes actual `BigInt`. The rename recommendation to `i128_to_decimal` is noted.

**No action needed** — the RFC already documents this. Flagged for implementation awareness.

---

### L2 — LOW: RFC-0202-A §6.5 NULL Display Consistency

The RFC says `Value::Null(DataType::Bigint)` displays as `"NULL"`. This matches existing behavior. No issue.

---

### L3 — LOW: RFC-0202-A §6.8a `stoolap_parse_decimal` Code Uses `DecimalError::ParseError` Placeholder

The RFC acknowledges this variant doesn't exist in `DecimalError`. This is a known gap that doesn't block RFC acceptance.

---

## Summary Table

| ID | Severity | RFC Section | Description | Action Required |
|----|----------|-------------|-------------|-----------------|
| C1 | CRITICAL | §6.6 | DFP/DQA same-type ordering broken | Fix all Extension types or document |
| C2 | CRITICAL | §6.12 | BIGINT/DECIMAL vs DFP/Quant PANIC | Move guards before `as_float64()` |
| C3 | CRITICAL | §6.8a | Whitespace spec/code contradiction | Align spec with code (accept whitespace) |
| H1 | HIGH | §6.15 | "Compile-blocking" claim wrong | Downgrade language |
| H2 | HIGH | §6.11 | Ord fix doesn't cover DFP/DQA | Add DFP/DQA dispatch arms |
| H3 | HIGH | §6.9 | Bare DECIMAL rounds to integer | Choose and document default |
| M1 | MEDIUM | §6.9 | Missing DECIMAL(p,s) Display | Add Display branch |
| M2 | MEDIUM | §6.3 | `decimal_to_string` error → empty string | Use `<invalid decimal>` |
| M3 | MEDIUM | §4, §4a | No WAL header infrastructure | Specify WALManager changes |
| M4 | MEDIUM | 0202-B §Phase 3 | Missing compiler CAST details | Show compiler match arm |
| M5 | MEDIUM | 0202-B §Test Vectors | DQA literal syntax unspecified | Specify or use programmatic tests |
| L1 | LOW | §6.7 | Naming confusion noted | Implementation awareness |
| L2 | LOW | §6.5 | NULL display consistent | No action |
| L3 | LOW | §6.8a | ParseError variant gap | Known, not blocking |

**Total: 3 CRITICAL, 3 HIGH, 5 MEDIUM, 3 LOW**

---

## Pre-existing Bugs Discovered (Not RFC-0202 Scope)

These bugs exist in the current Stoolap codebase and should be tracked separately:

1. **`Value::quant()` constructor / `extract_dqa_from_extension()` mismatch:** Constructor writes 10 bytes, extractor requires ≥17 bytes. All DQA values created via `Value::quant()` fail extraction. The 7 "reserved" bytes are never written due to a misunderstanding of `Vec::with_capacity` (it allocates but doesn't initialize).

2. **DFP same-type comparison broken:** `Value::dfp(1.0).compare(&Value::dfp(2.0))` returns `Err(IncomparableTypes)` because the DFP special path in `compare()` only fires for cross-type comparisons.

3. **DFP/Quant BTree index ordering broken:** Raw byte comparison in `Ord for Value` produces wrong numeric order for DFP and Quant Extension values. BTree indices on DFP/Quant columns are unreliable.
