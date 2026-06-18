# Code Review: RFC-0105 DQA Implementation vs Specification

**Reviewer:** @ciphercito
**Date:** 2026-04-01
**RFC Version:** v1.9 (accepted, 2026-03-08)
**Method:** Cross-reference every implementation claim against RFC-0105 v1.9, examining the determin crate (`/home/mmacedoeu/_w/ai/cipherocto/determin/`) and the Stoolap project (`/home/mmacedoeu/_w/databases/stoolap/`).

---

## Codebase State Verified

| File | Commit/Latest | Key Observation |
|------|---------------|-----------------|
| `determin/src/dqa.rs` | `Dqa` struct: `{ value: i64, scale: u8 }` | Matches RFC §3 |
| `determin/src/dqa.rs` | `Dqa::new(value, scale)` validates `scale <= 18` | Matches RFC §3 |
| `determin/src/dqa.rs` | `dqa_add/sub/mul/div` functions present | Free functions, not methods |
| `determin/src/dqa.rs` | `dqa_cmp(a, b) -> i8` returns -1/0/1 | Matches RFC §5 |
| `determin/src/lib.rs` | Top-level exports: `Dqa`, `dqa_cmp`, `dqa_negate`, `dqa_abs` | Missing: `dqa_add`, `dqa_sub`, `dqa_mul`, `dqa_div` |
| `stoolap/src/core/value.rs:205-214` | `Value::quant()` writes tag(1) + i64_BE(8) + scale(1) + reserved(7) = 18 bytes | Fixed in prior commit (was 10 bytes) |
| `stoolap/src/core/value.rs:418-431` | `as_dqa()` extracts via `Dqa::new()` with validation | Correct |
| `stoolap/src/core/value.rs:694-702` | `from_typed()` for Quant returns `Null` | Stub |
| `stoolap/src/core/value.rs:934-951` | `cast_to_type()` for Quant returns `Null` for all inputs | Stub |
| `stoolap/src/core/value.rs:1122-1128` | `into_coerce_to_type()` for Quant returns `Null` for all inputs | Stub |
| `stoolap/src/core/value.rs:1164-1172` | `Display` for Quant prints `<extension:9>` | No DQA-aware formatting |
| `stoolap/src/core/value.rs:274-283` | `as_float64()` returns `None` for ALL `Extension` types | Cross-type comparison panics |
| `stoolap/src/core/value.rs:500-504` | Cross-type numeric comparison uses `as_float64().unwrap()` | PANICS for Quant/DFP vs Integer/Float |
| `stoolap/src/core/value.rs:319-344` | `as_string()` has no Quant arm | Falls through to UTF-8 decode of binary payload |
| `stoolap/src/executor/expression/ops.rs:498-513` | 7 DQA opcodes: DqaAdd/Sub/Mul/Div/Neg/Abs/Cmp | All present |
| `stoolap/src/executor/expression/vm.rs:899-972` | All 7 DQA opcodes dispatched correctly | Full DQA arithmetic in VM |
| `stoolap/src/executor/expression/vm.rs:3269-3272` | `is_quant_value()` check in `arithmetic_op()` | DQA takes precedence over DFP |
| `stoolap/src/executor/expression/vm.rs:881-894` | `Op::Neg` handles Integer/Float only | Quant silently returns NULL |
| `stoolap/src/executor/expression/vm.rs:3384-3401` | `extract_dqa_from_extension()` uses `Dqa { value, scale }` directly | Bypasses `Dqa::new()` validation |
| `stoolap/src/executor/expression/compiler.rs` | Zero DQA-specific compilation logic | Never emits DqaAdd/etc. opcodes |
| `stoolap/src/storage/expression/cast.rs:83-86` | `perform_cast()` for Quant returns Error | Hard block |
| `stoolap/src/storage/mvcc/persistence.rs:1008-1028` | Wire tag 11: generic extension, no Quant validation | Corrupted scale > 18 accepted |
| `stoolap/src/core/schema.rs:65-66` | `quant_scale: u8` field | Matches RFC |
| `stoolap/src/core/schema.rs:191-208` | Display: `column_name DQA(N)` when scale > 0 | Correct |
| `stoolap/src/parser/statements.rs:1725-1750` | `DQA(N)` parsing with scale validation 0-18 | Correct |

---

## Findings

### D1 -- MEDIUM: Missing `CANONICAL_ZERO` constant

**Location:** `determin/src/dqa.rs`
**RFC Reference:** RFC-0105 v1.9, line 438

**RFC specifies:**
```rust
pub const CANONICAL_ZERO: Dqa = Dqa { value: 0, scale: 0 };
```

**Code:** No `CANONICAL_ZERO` constant exists. `Dqa::new(0, 0).unwrap()` produces the correct value but lacks ergonomic parity.

**Impact:** Minor API gap. Pattern-matching on the constant is not possible.

---

### D2 -- MEDIUM: Missing RFC division test vectors

**Location:** `determin/src/dqa.rs` test module
**RFC Reference:** RFC-0105 v1.9, §7 "Test Vectors"

**RFC specifies** these division test vectors:
- `dqa(1, 0) / dqa(3, 0) = dqa(0, 0)` (remainder truncated)
- `dqa(-1, 0) / dqa(3, 0) = dqa(0, 0)`
- `dqa(2, 0) / dqa(3, 0) = dqa(0, 0)`
- `dqa(1, 0) / dqa(6, 0) = dqa(0, 0)`
- `dqa(20000, 3) / dqa(30000, 3) = dqa(0, 0)` (= 2.0/3)
- `dqa(-20000, 3) / dqa(30000, 3) = dqa(0, 0)` (= -2.0/3)

**Code:** None of these division test vectors exist in the test module.

**Impact:** Division correctness for fractional results is undertested.

---

### D3 -- MEDIUM: Missing "Brutal Edge Case" test vectors

**Location:** `determin/src/dqa.rs` test module
**RFC Reference:** RFC-0105 v1.9, §7 "Brutal Edge Cases"

**RFC specifies** these edge case tests:
- `i64::MIN / dqa(-1, 0)` -- overflow behavior
- Chain operations: `(a + b) - c + d` at different scales
- Scale alignment overflow: values with max-scale difference

**Code:** No edge case tests exist beyond basic arithmetic.

**Impact:** Overflow and scale-alignment edge cases are untested.

---

### D4 -- LOW: Free functions not re-exported from crate root

**Location:** `determin/src/lib.rs`
**RFC Reference:** RFC-0105 v1.9, §8 "Public API"

**RFC specifies** `dqa_add`, `dqa_sub`, `dqa_mul`, `dqa_div` as public API. These exist in `determin/src/dqa.rs` as `pub fn` but are not in the `pub use` list in `lib.rs`. Accessible via `octo_determin::dqa::dqa_add` but not `octo_determin::dqa_add`.

**Impact:** Ergonomic gap. Stoolap already works around this by importing from the module path (`use octo_determin::dqa::{dqa_add, dqa_sub, dqa_mul, dqa_div, Dqa}`).

---

### D5 -- LOW: Method naming differs from RFC

**Location:** `determin/src/dqa.rs`
**RFC Reference:** RFC-0105 v1.9, §8

**RFC specifies:** `dqa.sub()`, `dqa.mul()`, `dqa.div()`
**Code implements:** `dqa.subtract()`, `dqa.multiply()`, `dqa.divide()`

**Impact:** Cosmetic. Free functions (`dqa_add`, etc.) match RFC names.

---

### D6 -- LOW: Multiplication test uses wrong value

**Location:** `determin/src/dqa.rs` test module
**RFC Reference:** RFC-0105 v1.9, §7

**RFC specifies:** `dqa(200, 3) * dqa(30, 1)` (= 0.200 * 3.0 = 0.6)
**Code tests:** `dqa(2000, 3) * dqa(30, 1)` (= 2.000 * 3.0 = 6.0)

The test exercises a different magnitude. Functionally correct but doesn't verify the RFC's specific vector.

**Impact:** Minor. The multiplication logic is still tested.

---

### D7 -- LOW: No `size_of::<DqaEncoding>()` compile-time assertion

**Location:** `determin/src/dqa.rs`
**RFC Reference:** RFC-0105 v1.9, §4 (16-byte encoding)

**RFC specifies:** `DqaEncoding` is exactly 16 bytes.
**Code:** No `const _: () = assert!(size_of::<DqaEncoding>() == 16);` assertion.

**Impact:** A future change could silently change the encoding size without compile-time failure.

---

### S1 -- CRITICAL: Cross-type numeric comparison PANICS for Quant values

**Location:** `stoolap/src/core/value.rs` lines 500-504
**RFC Reference:** RFC-0105 v1.9, §5 "Comparison"

**Code:**
```rust
// Cross-type numeric comparison (integer vs float vs DFP vs DQA)
if self.data_type().is_numeric() && other.data_type().is_numeric() {
    // Convert to f64 for comparison
    let v1 = self.as_float64().unwrap();  // PANICS for Extension types!
    let v2 = other.as_float64().unwrap();
    return Ok(compare_floats(v1, v2));
}
```

**`as_float64()` returns `None` for ALL `Extension` types** (line 281):
```rust
Value::Timestamp(_) | Value::Extension(_) | Value::Blob(_) => None,
```

**Both Quant and DFP are `is_numeric()`** (types.rs line 75):
```rust
DataType::Integer | DataType::Float | DataType::DeterministicFloat | DataType::Quant
```

**Impact:** Any cross-type comparison involving a Quant value panics the server:
- `WHERE quant_col > 5` -- Quant vs Integer → PANIC
- `WHERE quant_col > 3.14` -- Quant vs Float → PANIC
- `WHERE quant_col > dfp_col` -- Quant vs DFP → PANIC
- `WHERE 10 < quant_col` -- Integer vs Quant → PANIC

The same issue affects DFP values (noted in DFP review S7). This is a server-crash bug on legitimate SQL queries.

---

### S2 -- HIGH: `cast_to_type()` for Quant is a stub

**Location:** `stoolap/src/core/value.rs` lines 934-951
**RFC Reference:** RFC-0105 v1.9, §6 "Type Coercion"

**Code:**
```rust
DataType::Quant => {
    // Convert to DQA - cast from Float or Integer
    match self {
        Value::Float(_v) => {
            // TODO: Convert f64 to DQA when octo-determin is integrated
            Value::Null(target_type)
        }
        Value::Integer(_v) => {
            // TODO: Convert i64 to DQA when octo-determin is integrated
            Value::Null(target_type)
        }
        Value::Text(_s) => {
            // TODO: Parse string as DQA when octo-determin is integrated
            Value::Null(target_type)
        }
        _ => Value::Null(target_type),
    }
}
```

**Note:** There is a dead-code duplicate `DataType::Quant` arm at line 986 that is unreachable.

**Contrast with DFP:** `cast_to_type()` for `DeterministicFloat` at line 803-823 has WORKING conversions using `Dfp::from_i64()`, `Dfp::from_f64()`, and string parsing. The analogous DQA conversions (`Dqa::new(i64_value, 0)`, parsing text to scaled integer) are trivial but not implemented.

**Impact:** `CAST(123 AS DQA)` silently returns NULL. The TODO comment "when octo-determin is integrated" is stale -- the determin crate provides all necessary conversion functions.

---

### S3 -- HIGH: `into_coerce_to_type()` for Quant is a stub

**Location:** `stoolap/src/core/value.rs` lines 1122-1128

**Code:**
```rust
DataType::Quant => match self {
    // DQA casts - placeholder until octo-determin integration
    Value::Float(_v) => Value::Null(target_type),
    Value::Integer(_v) => Value::Null(target_type),
    Value::Text(_s) => Value::Null(target_type),
    _ => Value::Null(target_type),
},
```

**Impact:** The consuming coercion path is used by INSERT value handling. `INSERT INTO t(dqa_col) VALUES(1.5)` silently stores NULL. Same stale TODO as S2.

---

### S4 -- HIGH: `from_typed()` for Quant is a stub

**Location:** `stoolap/src/core/value.rs` lines 694-702
**RFC Reference:** RFC-0105 v1.9, §6 "SQL Integration"

**Code:**
```rust
DataType::Quant => {
    // DQA support - downcast from string representation
    if let Some(_s) = v.downcast_ref::<String>() {
        // Parse as DQA when implemented
        Value::Null(data_type)
    } else {
        Value::Null(data_type)
    }
}
```

**Impact:** Parameter binding for DQA columns always produces NULL. External drivers/tools that use `from_typed()` for value construction cannot insert DQA values.

---

### S5 -- HIGH: Quant `Display` renders as `<extension:9>`

**Location:** `stoolap/src/core/value.rs` lines 1164-1172
**RFC Reference:** RFC-0105 v1.9, §6.4 "Display Format"

**Code:**
```rust
Value::Extension(data) => {
    let tag = data.first().copied().unwrap_or(0);
    if tag == DataType::Json as u8 { ... }
    else if tag == DataType::Vector as u8 { ... }
    else { write!(f, "<extension:{}>", tag) }  // Quant prints "<extension:9>"
}
```

**RFC specifies:** Quant values should display as their decimal representation (e.g., `1.23` for value=123, scale=2).

**Impact:** All user-facing output of DQA values shows `<extension:9>`. Query results, error messages, and debug output are unusable for DQA columns.

---

### S6 -- HIGH: `Op::Neg` silently returns NULL for Quant

**Location:** `stoolap/src/executor/expression/vm.rs` lines 881-894

**Code:**
```rust
Op::Neg => {
    let v = self.stack.pop().unwrap_or_else(Value::null_unknown);
    let result = match v {
        Value::Integer(i) => match i.checked_neg() { ... },
        Value::Float(f) => Value::Float(-f),
        Value::Null(dt) => Value::Null(dt),
        _ => Value::Null(DataType::Null),  // Quant hits this
    };
    self.stack.push(result);
    pc += 1;
}
```

**Analysis:** The compiler never emits `Op::DqaNeg` (see S8). It always emits `Op::Neg`. Since `Op::Neg` doesn't handle Extension types, Quant negation silently returns NULL.

**Contrast with DQA arithmetic:** `Op::Add/Sub/Mul` route through `arithmetic_op()` which has runtime Quant detection (line 3269). `Op::Neg` has no such detection.

**Impact:** `SELECT -quant_col FROM t` returns NULL.

---

### S7 -- MEDIUM: `perform_cast()` hard-errors on Quant

**Location:** `stoolap/src/storage/expression/cast.rs` lines 83-86

**Code:**
```rust
DataType::Quant => Err(crate::core::Error::type_conversion(
    format!("{:?}", value),
    "DQA",
)),
```

**Contrast with `Value::cast_to_type()`:** The Value-level cast (stub, returns NULL) and the storage-level CastExpr (hard error) have different failure modes for the same logical operation.

**Impact:** `CAST(x AS DQA)` via CastExpr returns an error. Via Value::cast_to_type() it silently returns NULL. Inconsistent error handling.

---

### S8 -- MEDIUM: Compiler never emits DQA-specific opcodes

**Location:** `stoolap/src/executor/expression/compiler.rs`

**Analysis:** The compiler emits generic `Op::Add/Sub/Mul/Div/Mod/Neg` for all arithmetic. It never inspects column types to emit `Op::DqaAdd/DqaSub/etc.` All DQA arithmetic flows through runtime type detection in `arithmetic_op()` (line 3269-3272), which checks `is_quant_value()` and routes to `arithmetic_op_quant()`.

This works for Add/Sub/Mul (which call `arithmetic_op()`), but:
- `Op::Div` calls `div_op()` directly (bypasses Quant detection) -- **BROKEN** (returns NULL)
- `Op::Mod` calls `mod_op()` directly (bypasses Quant detection) -- **BROKEN** (returns NULL)
- `Op::Neg` has no Quant handling -- **BROKEN** (returns NULL, see S6)

**Wait -- re-checking:** The DQA-specific opcodes at lines 899-972 are dispatched in the main execution loop. If the compiler emitted them, they would work. But the compiler doesn't emit them, so they're dead code.

**However:** For `Op::Div` and `Op::Mod`, the generic path does NOT route to `arithmetic_op()`. These call `div_op()`/`mod_op()` directly at lines 865-877, which lack Quant handling. This means DQA division and modulo via the generic path silently return NULL -- the same issue as DFP review findings S1/S2.

**Impact:** DQA division and modulo return NULL via the generic opcode path. The dedicated `Op::DqaDiv` opcode works but is never emitted. This is architecturally fragile.

---

### S9 -- MEDIUM: `extract_dqa_from_extension()` bypasses `Dqa::new()` validation

**Location:** `stoolap/src/executor/expression/vm.rs` lines 3384-3401

**Code:**
```rust
fn extract_dqa_from_extension(data: &crate::common::CompactArc<[u8]>) -> Option<Dqa> {
    if data.first().copied() == Some(DataType::Quant as u8) {
        if data.len() >= 10 {
            let value_bytes: [u8; 8] = data[1..9].try_into().ok()?;
            let scale = data[9];
            let value = i64::from_be_bytes(value_bytes);
            Some(Dqa { value, scale })  // Direct construction, no validation
        } else {
            None
        }
    } else {
        None
    }
}
```

**Contrast with `Value::as_dqa()`:** Uses `Dqa::new(value, scale).ok()` which validates `scale <= 18`.

**Impact:** A deserialized Quant value with `scale > 18` would be accepted by the VM's extraction function but rejected by the public `as_dqa()` method. Inconsistent behavior.

---

### S10 -- LOW: `as_string()` and `as_float64()` have no Quant handling

**Location:** `stoolap/src/core/value.rs` lines 274-283, 319-344

**`as_float64()` (line 281):** All `Extension` types return `None`. No DQA-to-f64 conversion.

**`as_string()` (lines 334-340):** Falls through to generic Extension handler that tries UTF-8 decode of binary payload. DQA payload (i64 BE + scale byte + reserved) is not valid UTF-8, so returns `None`.

**Impact:** `CAST(quant_col AS TEXT)` returns NULL. String concatenation with DQA values returns NULL. Cross-type numeric comparison panics (covered by S1).

---

### S11 -- LOW: Zero DQA integration tests in Stoolap

**Analysis:** No DQA-specific integration tests exist in the Stoolap codebase. The determin crate has unit tests, but there is no end-to-end verification that:
- DQA values survive storage and retrieval
- DQA arithmetic produces correct results through the VM
- DQA display/rendering works
- DQA comparison and ordering work
- DQA parameters bind correctly

**Impact:** RFC-0105 §7 acceptance criteria require verified test vectors. Stoolap has none.

---

### S12 -- LOW: DQA excluded from datatype tests

**Location:** `stoolap/src/core/types.rs`

**Analysis:**
- `test_datatype_is_numeric()` (line 510): Asserts Integer and Float are numeric, does not assert Quant or DFP
- `test_datatype_u8_conversion()` (line 534): Iterates 8 types (Null..Vector), excludes Quant (tag 9)

Note: The `is_numeric()` method correctly includes `Quant` (line 75), but the test does not verify this.

**Impact:** The type system works for Quant, but test coverage doesn't verify it.

---

### S13 -- LOW: No Quant-specific validation on deserialization

**Location:** `stoolap/src/storage/mvcc/persistence.rs` lines 1008-1028

**Analysis:** Wire tag 11 uses generic extension deserialization. JSON gets UTF-8 validation; Quant gets no validation. A corrupted payload with `scale > 18` or wrong length would be silently accepted.

**Impact:** Corrupted Quant values survive deserialization and only fail (or produce wrong results) when accessed.

---

## Summary

| ID | Severity | Component | Finding | RFC Section |
|----|----------|-----------|---------|-------------|
| D1 | MEDIUM | determin | Missing `CANONICAL_ZERO` constant | §8 Constants |
| D2 | MEDIUM | determin | Missing RFC division test vectors | §7 Test Vectors |
| D3 | MEDIUM | determin | Missing edge case test vectors | §7 Brutal Edge Cases |
| D4 | LOW | determin | Free functions not re-exported from crate root | §8 Public API |
| D5 | LOW | determin | Method names differ from RFC (subtract/sub etc.) | §8 |
| D6 | LOW | determin | Multiplication test uses wrong value | §7 |
| D7 | LOW | determin | No `size_of::<DqaEncoding>()` assertion | §4 |
| S1 | CRITICAL | Stoolap Value | Cross-type comparison PANICS for Quant | §5 |
| S2 | HIGH | Stoolap Value | `cast_to_type()` for Quant is a stub | §6 |
| S3 | HIGH | Stoolap Value | `into_coerce_to_type()` for Quant is a stub | §6 |
| S4 | HIGH | Stoolap Value | `from_typed()` for Quant is a stub | §6 |
| S5 | HIGH | Stoolap Value | Quant `Display` renders as `<extension:9>` | §6.4 |
| S6 | HIGH | Stoolap VM | `Op::Neg` silently returns NULL for Quant | §5 |
| S7 | MEDIUM | Stoolap CastExpr | `perform_cast()` hard-errors on Quant | §6 |
| S8 | MEDIUM | Stoolap Compiler | DQA opcodes defined but never emitted | §5 |
| S9 | MEDIUM | Stoolap VM | `extract_dqa_from_extension()` bypasses validation | §4 |
| S10 | LOW | Stoolap Value | `as_string()`/`as_float64()` have no Quant handling | §6 |
| S11 | LOW | Stoolap Tests | Zero DQA integration tests | §7 |
| S12 | LOW | Stoolap Tests | DQA excluded from datatype tests | N/A |
| S13 | LOW | Stoolap Persistence | No Quant validation on deserialization | §4 |

**Totals:** 1 CRITICAL, 4 HIGH, 5 MEDIUM, 7 LOW = 17 findings (7 determin, 10 Stoolap)

---

## Architectural Assessment

The DQA implementation has a different maturity profile than DFP:

**determin crate** -- Core arithmetic is complete and correct. All 7 operations (add, sub, mul, div, neg, abs, cmp) are implemented with proper scale alignment, overflow detection, and RoundHalfEven. The API surface is nearly complete, missing only `CANONICAL_ZERO`. Test coverage is adequate but missing RFC-specified vectors.

**Stoolap integration** -- The VM-level DQA arithmetic is the **strongest part** of the integration. All 7 DQA opcodes are defined and dispatched correctly. The `arithmetic_op_quant()` method handles DQA+DQA and Integer+DQA arithmetic with proper scale-aware dispatch. Schema support (`quant_scale`, `DQA(N)` parsing, Display) is also well-implemented.

However, the integration suffers from the same **dispatch asymmetry** as DFP: `Op::Add/Sub/Mul` route through `arithmetic_op()` (which has Quant detection), but `Op::Div/Mod` bypass it entirely. The dedicated `Op::DqaDiv` opcode exists but is never emitted by the compiler. This means DQA division via the generic path silently returns NULL -- identical to DFP review findings S1/S2.

The most severe issue is the **cross-type comparison panic** (S1). `WHERE quant_col > 5` crashes the server because `as_float64()` returns `None` for Quant, and the cross-type numeric path uses `.unwrap()`. This same bug affects DFP (DFP review S7) and will affect any future numeric Extension types. The fix requires either:
1. Adding `as_float64()` support for Quant/DFP (lossy but safe), or
2. Restructuring the cross-type numeric comparison to handle Extension types before falling through to `as_float64()`.

The stub methods (`cast_to_type`, `into_coerce_to_type`, `from_typed`) and missing Display are the same class of incomplete integration found in the DFP review. The stale "when octo-determin is integrated" TODO comments indicate these were left as placeholders during initial wiring.

**Recommendation:** The CRITICAL finding (S1) should be resolved immediately -- it crashes the server. The 4 HIGH findings (S2-S5) represent the same incomplete integration as DFP and should be resolved in the same pass.

---

## Cross-Reference with DFP Review

This review shares structural patterns with the DFP code review (`docs/reviews/rfc-0104-dfp-code-review.md`):

| Pattern | DFP Review | DQA Review |
|---------|-----------|-----------|
| `Op::Div` bypasses type dispatch | S1 (CRITICAL) | S8 (MEDIUM, DqaDiv exists) |
| `Op::Mod` bypasses type dispatch | S2 (CRITICAL) | S8 (MEDIUM, no DqaMod opcode) |
| `cast_to_type()` stub | N/A (DFP works) | S2 (HIGH) |
| `into_coerce_to_type()` stub | S3 (CRITICAL) | S3 (HIGH) |
| `Op::Neg` returns NULL | S4 (HIGH) | S6 (HIGH) |
| `Display` shows `<extension:8>` | S6 (HIGH) | S5 (HIGH, `<extension:9>`) |
| `as_float64()` returns None | S7 (HIGH) | S1 (CRITICAL, causes panic) |
| `from_typed()` stub | S8 (HIGH) | S4 (HIGH) |
| `as_string()` missing | S9 (MEDIUM) | S10 (LOW) |
| No integration tests | S15 (LOW) | S11 (LOW) |
| No deserialization validation | S13 (MEDIUM) | S13 (LOW) |
