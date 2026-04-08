# Code Review: RFC-0104 DFP Implementation vs Specification

**Reviewer:** @ciphercito
**Date:** 2026-04-01
**RFC Version:** v1.16 (accepted, 2026-03-08)
**Method:** Cross-reference every implementation claim against RFC-0104 v1.16, examining the determin crate (`/home/mmacedoeu/_w/ai/cipherocto/determin/`) and the Stoolap project (`/home/mmacedoeu/_w/databases/stoolap/`).

---

## Codebase State Verified

| File | Commit/Latest | Key Observation |
|------|---------------|-----------------|
| `determin/src/lib.rs` | `Dfp` struct fields: `mantissa, exponent, class, sign` (order differs from RFC) | No `from_signed()` constructor; no `DFP_CANONICAL_NAN` constant |
| `determin/src/arithmetic.rs:289` | Division loop: `for _ in 0..128` | RFC specifies 256 iterations |
| `determin/src/arithmetic.rs:364` | `dfp_sqrt` present with 226-bit scaling and U512 | Exists in determin crate but never called from Stoolap VM |
| `stoolap/src/executor/expression/vm.rs:865-871` | `Op::Div` calls `Self::div_op()` directly | Bypasses `arithmetic_op()` which has DFP dispatch |
| `stoolap/src/executor/expression/vm.rs:873-877` | `Op::Mod` calls `Self::mod_op()` directly | Bypasses `arithmetic_op()` which has DFP dispatch |
| `stoolap/src/executor/expression/vm.rs:881-894` | `Op::Neg` handles `Integer` and `Float` only | No DFP branch; falls through to `Value::Null(DataType::Null)` |
| `stoolap/src/executor/expression/vm.rs:3310-3344` | `arithmetic_op()` has full DFP dispatch for `Add/Sub/Mul/Div/Mod` | Only reached by `Op::Add`, `Op::Sub`, `Op::Mul` -- NOT by `Op::Div` or `Op::Mod` |
| `stoolap/src/executor/expression/vm.rs:3414-3506` | `arithmetic_op_deterministic()` has complete DFP paths | Only reached when `self.deterministic == true` |
| `stoolap/src/core/value.rs:1129-1135` | `into_coerce_to_type()` for `DeterministicFloat` returns `Null` for all inputs | Stub; borrowing version `cast_to_type()` works correctly (lines 803-823) |
| `stoolap/src/core/value.rs:274-283` | `as_float64()` returns `None` for ALL `Extension` types | DFP-to-f64 coercion absent; cross-type comparison falls through to string ordering |
| `stoolap/src/core/value.rs:1164-1173` | `Display` for `Value::Extension` checks `Json` and `Vector` tags only | DFP prints as `<extension:8>` |
| `stoolap/src/core/value.rs:319-344` | `as_string()` has no DFP arm | Generic `Extension` fallback tries UTF-8 decode of binary `DfpEncoding`, fails |
| `stoolap/src/core/value.rs:685-693` | `from_typed()` for `DeterministicFloat` is a stub returning `Null` | `INSERT INTO t(dfp_col) VALUES(1.5)` stores NULL |
| `stoolap/src/storage/expression/cast.rs:79-82` | `perform_cast()` for `DeterministicFloat` returns `Error` for all inputs | `CAST(x AS DFP)` via `CastExpr` always fails |
| `stoolap/src/storage/mvcc/persistence.rs:1008-1028` | Wire tag 11: generic extension deserialization | No DFP-specific validation of 24-byte `DfpEncoding` payload |
| `stoolap/src/executor/expression/ops.rs` | `Op` enum has 7 `Dqa*` opcodes, zero `Dfp*` opcodes | RFC-0104 §Expression VM Opcodes specifies `OP_DFP_ADD/SUB/MUL/DIV` |
| `stoolap/src/executor/expression/compiler.rs` | Zero DFP-specific logic | No deterministic mode propagation from schema to VM |
| `stoolap/src/core/types.rs:510-519` | `test_datatype_is_numeric` tests Integer and Float only | DFP and Quant not in test matrix |
| `stoolap/src/core/types.rs:534-543` | `test_datatype_u8_conversion` iterates 8 types (Null..Vector) | DFP (tag 8), Quant (tag 9), Blob (tag 10) excluded |

---

## Findings

### D1 -- CRITICAL: Division iterations reduced from 256 to 128

**Location:** `determin/src/arithmetic.rs` line 289
**RFC Reference:** RFC-0104 v1.16, "Division Algorithm (Deterministic Long Division)", line 372: `for i in 0..256`

**Code:**
```rust
for _ in 0..128 {
```

**RFC specifies:**
```
// Fixed 256 iterations for determinism
for i in 0..256:
```

**Analysis:** The RFC explicitly requires 256 iterations for the shift-and-subtract long division loop. The implementation uses 128 iterations. The code comment (lines 282-284) states this produces "128 bits of precision (15 guard bits beyond the 113 we keep)" -- which is mathematically sound for 113-bit precision. However, the RFC's Golden Rule #3 (line 1471) states: "No Iteration Short-Circuiting: Execute ALL iterations as specified (256 for division, 226 for SQRT)."

If this was a deliberate optimization, the RFC must be updated to reflect 128 iterations. If the RFC is authoritative, the implementation must be changed to 256.

**Impact:** Reduced precision margin. With 128 iterations and the pre-scaling approach (where `a_m < b_m` is guaranteed), 128 bits of quotient precision yields 15 guard bits above 113 -- sufficient for correct RNE rounding in most cases, but contradicts the specification.

---

### D2 -- HIGH: Missing `from_signed()` constructor

**Location:** `determin/src/lib.rs` `impl Dfp` block
**RFC Reference:** RFC-0104 v1.16, line 141-148

**RFC specifies:**
```rust
pub fn from_signed(mantissa: i128, exponent: i32) -> Self {
    Self {
        class: DfpClass::Normal,
        sign: mantissa < 0,
        mantissa: mantissa.unsigned_abs(),
        exponent,
    }
}
```

**Code:** No `from_signed()` method exists. `Dfp::new()` takes `(mantissa: u128, exponent: i32, class: DfpClass, sign: bool)`, which requires the caller to decompose sign separately.

**Impact:** Ergonomic gap. Any caller with a signed mantissa must replicate the sign-extraction logic. The RFC's constructor signature is part of the public API contract.

---

### D3 -- MEDIUM: Missing `DFP_CANONICAL_NAN` constant

**Location:** `determin/src/lib.rs`
**RFC Reference:** RFC-0104 v1.16, lines 747-753

**RFC specifies:**
```rust
pub const DFP_CANONICAL_NAN: Dfp = Dfp {
    class: DfpClass::NaN,
    sign: false,
    mantissa: 0,
    exponent: 0,
};
```

**Code:** `Dfp::nan()` method exists (line 120) but no `DFP_CANONICAL_NAN` constant. The codebase has `DFP_MAX` and `DFP_MIN` as constants (lines 338, 346) but no NaN equivalent.

**Impact:** Inconsistency with RFC's constant declarations. `Dfp::nan()` produces the correct value but lacks the ergonomic parity with `DFP_MAX`/`DFP_MIN`. Pattern-matching on the constant is not possible.

---

### D4 -- LOW: `Dfp` struct field order differs from RFC

**Location:** `determin/src/lib.rs` lines 87-96
**RFC Reference:** RFC-0104 v1.16, lines 117-127

**RFC field order:** `class, sign, mantissa, exponent`
**Code field order:** `mantissa, exponent, class, sign`

**Analysis:** Neither the RFC's struct nor the code's struct has `#[repr(C)]` -- only `DfpEncoding` carries `#[repr(C, align(8))]`. Since `Dfp` is never serialized directly (only `DfpEncoding` is), field order has no ABI impact. This is a cosmetic discrepancy only.

**Impact:** None functional. Documentation/code cross-referencing may cause confusion.

---

### S1 -- CRITICAL: `Op::Div` bypasses DFP arithmetic, returns NULL

**Location:** `stoolap/src/executor/expression/vm.rs` lines 865-871
**RFC Reference:** RFC-0104 v1.16, "Expression VM Opcodes", lines 485-491

**Code:**
```rust
Op::Div => {
    let b = self.stack.pop().unwrap_or_else(Value::null_unknown);
    let a = self.stack.pop().unwrap_or_else(Value::null_unknown);
    let result = Self::div_op(&a, &b);       // <-- calls div_op directly
    self.stack.push(result);
    pc += 1;
}
```

**`div_op` implementation** (lines 3610-3619) handles only `Integer/Integer`, `Float/Float`, and `Integer/Float` combinations. There is no DFP branch. Any `Value::Extension` with DFP tag falls through to `_ => Value::Null(DataType::Null)`.

**Contrast with `Op::Add`/`Op::Sub`/`Op::Mul`:** These three call `self.arithmetic_op(...)` which has full DFP dispatch at lines 3310-3344 (non-deterministic mode) and lines 3414-3506 (deterministic mode).

**Impact:** DFP division silently returns NULL in non-deterministic mode. This breaks the fundamental arithmetic contract of RFC-0104. A query like `SELECT dfp_col / 2 FROM t` returns NULL instead of a DFP result.

**Note on deterministic mode:** When `self.deterministic == true`, `arithmetic_op()` routes through `arithmetic_op_deterministic()` which handles DFP division correctly. But `Op::Div` never calls `arithmetic_op()` -- it calls `Self::div_op()` directly, bypassing the deterministic flag check entirely. DFP division is broken in BOTH modes.

---

### S2 -- CRITICAL: `Op::Mod` bypasses DFP arithmetic, returns NULL

**Location:** `stoolap/src/executor/expression/vm.rs` lines 873-877

**Same structural issue as S1.** `Op::Mod` calls `Self::mod_op()` directly (lines 3622-3631), which has no DFP handling. The `_ =>` arm returns `Value::Null(DataType::Null)`.

**Impact:** DFP modulo silently returns NULL in all modes.

---

### S3 -- CRITICAL: `into_coerce_to_type()` for DFP always returns NULL

**Location:** `stoolap/src/core/value.rs` lines 1129-1135

**Code:**
```rust
DataType::DeterministicFloat => match self {
    // DFP casts - placeholder until octo-determin integration
    Value::Float(_v) => Value::Null(target_type),
    Value::Integer(_v) => Value::Null(target_type),
    Value::Text(_s) => Value::Null(target_type),
    _ => Value::Null(target_type),
},
```

**Contrast with the borrowing version `cast_to_type()`** (lines 803-823) which correctly handles DFP conversion:
```rust
DataType::DeterministicFloat => {
    match self {
        Value::Extension(data) if /* already DFP */ => self.clone(),
        Value::Integer(v) => Value::dfp(Dfp::from_i64(*v)),
        Value::Float(v) => Value::dfp(Dfp::from_f64(*v)),
        Value::Text(s) => s.parse::<f64>().map(|f| Value::dfp(Dfp::from_f64(f)))...
        Value::Boolean(b) => Value::dfp(Dfp::from_f64(if *b { 1.0 } else { 0.0 })),
        _ => Value::Null(target_type),
    }
}
```

**Impact:** Any code path using the consuming `into_coerce_to_type()` method will silently produce NULLs for DFP targets. The borrowing version works, creating an inconsistent API. The comment "placeholder until octo-determin integration" is stale -- the `cast_to_type()` implementation already proves integration exists.

---

### S4 -- HIGH: `Op::Neg` for DFP returns NULL

**Location:** `stoolap/src/executor/expression/vm.rs` lines 881-894

**Code:**
```rust
Op::Neg => {
    let v = self.stack.pop().unwrap_or_else(Value::null_unknown);
    let result = match v {
        Value::Integer(i) => match i.checked_neg() { ... },
        Value::Float(f) => Value::Float(-f),
        Value::Null(dt) => Value::Null(dt),
        _ => Value::Null(DataType::Null),     // <-- DFP hits this
    };
    self.stack.push(result);
    pc += 1;
}
```

**Impact:** `SELECT -dfp_col FROM t` returns NULL. DFP negation is trivially implementable (flip the `sign` field) but the branch is missing.

---

### S5 -- HIGH: No DFP `sqrt` in VM

**Location:** Stoolap VM has no sqrt opcode or dispatch
**RFC Reference:** RFC-0104 v1.16, "Square Root Algorithm", lines 413-481; "Gas/Fee Modeling" table lists `DFP_SQRT`

**Analysis:** The determin crate implements `dfp_sqrt()` at `arithmetic.rs:364` with the corrected 226-bit algorithm. However, the Stoolap VM has no mechanism to invoke it. There is no `Op::DfpSqrt` or equivalent, and no function dispatch for DFP sqrt. DQA has 7 dedicated opcodes (DqaAdd, DqaSub, DqaMul, DqaDiv, DqaNeg, DqaAbs, DqaCmp) while DFP has zero.

**Impact:** `SELECT SQRT(dfp_col) FROM t` is unreachable. The RFC's Mission 1 acceptance criteria list "sqrt (square root)" as required.

---

### S6 -- HIGH: Value `Display` for DFP prints `<extension:8>`

**Location:** `stoolap/src/core/value.rs` lines 1164-1173

**Code:**
```rust
Value::Extension(data) => {
    let tag = data.first().copied().unwrap_or(0);
    if tag == DataType::Json as u8 { ... }
    else if tag == DataType::Vector as u8 { ... }
    else { write!(f, "<extension:{}>", tag) }     // <-- DFP prints "<extension:8>"
}
```

**Impact:** Any user-facing output of DFP values (query results, error messages, debug output) shows `<extension:8>` instead of the numeric representation. This makes DFP columns unusable in practice for querying and debugging.

---

### S7 -- HIGH: `as_float64()` returns `None` for DFP

**Location:** `stoolap/src/core/value.rs` lines 274-283

**Code:**
```rust
pub fn as_float64(&self) -> Option<f64> {
    match self {
        Value::Null(_) => None,
        Value::Integer(v) => Some(*v as f64),
        Value::Float(v) => Some(*v),
        Value::Text(s) => s.parse::<f64>().ok(),
        Value::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::Timestamp(_) | Value::Extension(_) | Value::Blob(_) => None,
    }
}
```

**Impact:** Cross-type numeric comparison (e.g., `WHERE dfp_col > int_col` or `WHERE dfp_col > float_col`) uses `as_float64().unwrap()` at line 496-497. For DFP values, this returns `None`, causing the comparison to fall through to string-based ordering, producing semantically incorrect results (lexicographic instead of numeric). The `as_dfp()` method exists (line 407) but is not used in the comparison path.

---

### S8 -- HIGH: `from_typed()` for DFP is a stub

**Location:** `stoolap/src/core/value.rs` lines 685-693

**Code:**
```rust
DataType::DeterministicFloat => {
    // DFP support - downcast from string representation
    if let Some(_s) = v.downcast_ref::<String>() {
        // Parse as DFP when implemented
        Value::Null(data_type)
    } else {
        Value::Null(data_type)
    }
}
```

**Impact:** The `from_typed()` method is used by the parameter binding and INSERT value paths. An `INSERT INTO t(dfp_col) VALUES(1.5)` will store NULL instead of the DFP representation of 1.5. The comment "Parse as DFP when implemented" is stale -- `Dfp::from_f64()` exists in the determin crate and is used by `cast_to_type()`.

---

### S9 -- MEDIUM: `as_string()` does not handle DFP

**Location:** `stoolap/src/core/value.rs` lines 319-344

**Analysis:** The `as_string()` method has explicit arms for `Json` and `Vector` extensions but not for `DeterministicFloat`. The generic `Extension` fallback at line 334-340 attempts UTF-8 interpretation of the raw bytes, which will fail for binary `DfpEncoding` data (24 bytes of big-endian binary). The method returns `None` for DFP values.

**Impact:** `CAST(dfp_col AS TEXT)` returns NULL. String concatenation with DFP values (`'price: ' || dfp_col`) returns NULL.

---

### S10 -- MEDIUM: No DETERMINISTIC VIEW enforcement

**RFC Reference:** RFC-0104 v1.16, "SQL Integration", lines 787-800; "Constraints" line 939

**RFC specifies:**
```sql
CREATE DETERMINISTIC VIEW v_portfolio AS
SELECT price * quantity AS total FROM trades;
```

And in the "Deterministic Context Rules" section:
```
FLOAT  -> FORBIDDEN
DOUBLE -> FORBIDDEN
DFP    -> ALLOWED
```

**Analysis:** The Stoolap VM has a `deterministic` flag (used by `arithmetic_op()` to route to `arithmetic_op_deterministic()`), but no SQL syntax for `CREATE DETERMINISTIC VIEW` exists, and nothing automatically sets the flag based on schema context. The compiler (`compiler.rs`) has zero DFP awareness and does not propagate deterministic mode from schema to VM.

**Impact:** There is no way to create a deterministic execution context from SQL. The deterministic mode exists in the VM but is unreachable through the query interface.

---

### S11 -- MEDIUM: Compiler has zero DFP awareness

**Location:** `stoolap/src/executor/expression/compiler.rs`

**Analysis:** Grep for `DFP`, `dfp`, `DeterministicFloat`, or `deterministic` returns zero matches in the compiler. The expression compiler does not:
- Emit DFP-specific opcodes (none exist)
- Set the VM's `deterministic` flag based on column types
- Validate type-mixing rules (DFP vs FLOAT prohibition)
- Generate type promotion code for INT-to-DFP

**Impact:** Even if the VM's DFP dispatch were fully functional, the compiler cannot generate correct instructions for DFP expressions. The entire compile-time-to-runtime pipeline for DFP is missing.

---

### S12 -- MEDIUM: No DFP-specific opcodes

**Location:** `stoolap/src/executor/expression/ops.rs`

**RFC Reference:** RFC-0104 v1.16, lines 485-491

**RFC specifies:**
```rust
pub enum VmOpcode {
    OP_DFP_ADD,
    OP_DFP_SUB,
    OP_DFP_MUL,
    OP_DFP_DIV,
}
```

**Code:** The `Op` enum has 7 DQA-specific opcodes (DqaAdd, DqaSub, DqaMul, DqaDiv, DqaNeg, DqaAbs, DqaCmp) and zero DFP opcodes. DFP arithmetic currently relies on the generic `Op::Add/Sub/Mul/Div` with runtime type detection.

**Analysis:** Runtime type detection works for Add/Sub/Mul (which call `arithmetic_op()`) but is broken for Div/Mod (which bypass `arithmetic_op()` -- see S1/S2). Dedicated opcodes would eliminate this class of bug by ensuring DFP always takes the correct path.

**Impact:** The current design is fragile -- adding a new arithmetic op requires remembering to add DFP dispatch in the right place. DQA's dedicated opcodes demonstrate the correct pattern.

---

### S13 -- MEDIUM: `persistence.rs` does not validate DFP encoding on deserialization

**Location:** `stoolap/src/storage/mvcc/persistence.rs` lines 1008-1028

**Code:** Wire tag 11 uses generic extension deserialization that accepts any payload:
```rust
11 => {
    // Generic extension: dt_u8 + len_u32 + raw bytes
    let dt_byte = rest[0];
    let dt = DataType::from_u8(dt_byte).ok_or_else(|| ...)?;
    let len = u32::from_le_bytes(rest[1..5].try_into().unwrap()) as usize;
    let payload = &rest[5..5 + len];
    // Validate UTF-8 for text-based extension types
    if dt == DataType::Json && std::str::from_utf8(payload).is_err() {
        return Err(Error::internal("corrupted JSON extension: invalid UTF-8"));
    }
    // No DFP validation
```

**Impact:** A corrupted 24-byte `DfpEncoding` (wrong length, invalid class tag, or inconsistent class/mantissa) would be silently accepted during deserialization and only fail (or produce wrong results) when the value is accessed. JSON gets validation; DFP does not.

---

### S14 -- MEDIUM: `CastExpr::perform_cast()` returns Error for DFP

**Location:** `stoolap/src/storage/expression/cast.rs` lines 79-82

**Code:**
```rust
DataType::DeterministicFloat => Err(crate::core::Error::type_conversion(
    format!("{:?}", value),
    "DFP",
)),
```

**Contrast with `Value::cast_to_type()`** (value.rs:803-823) which handles DFP correctly for Integer, Float, Text, and Boolean inputs.

**Impact:** The `CastExpr` path (used in expression evaluation) always fails for `CAST(x AS DFP)`, even though the `Value`-level cast works. Two different code paths for the same logical operation, one works and one does not.

---

### S15 -- LOW: Zero DFP tests in Stoolap

**Analysis:** No DFP-specific integration tests exist anywhere in the Stoolap codebase. The determin crate has extensive unit tests, but there is no end-to-end verification that:
- DFP values survive storage and retrieval
- DFP arithmetic produces correct results through the VM
- DFP display/rendering works
- DFP comparison and ordering work
- DFP parameters bind correctly

**Impact:** The RFC's Mission 1 acceptance criteria require "Test vectors: 500+ verified cases." The determin crate has test vectors but Stoolap has none.

---

### S16 -- LOW: DFP excluded from existing datatype tests

**Location:** `stoolap/src/core/types.rs` tests

**Analysis:**
- `test_datatype_display` (line 477): tests 8 types, does not include DFP
- `test_datatype_is_numeric` (line 510): asserts Integer and Float are numeric, does not assert DFP
- `test_datatype_u8_conversion` (line 533): iterates over 8 types (Null..Vector), does not include DFP (tag 8) or Quant (tag 9)

Note: The `is_numeric()` method itself correctly includes `DeterministicFloat` (line 75), but the test does not verify this.

**Impact:** The type system infrastructure works for DFP, but test coverage does not verify it.

---

## Summary

| ID | Severity | Component | Finding | RFC Section |
|----|----------|-----------|---------|-------------|
| D1 | CRITICAL | determin | Division iterations 128 vs RFC 256 | Division Algorithm, Golden Rule #3 |
| D2 | HIGH | determin | Missing `from_signed()` constructor | Data Structures (line 141) |
| D3 | MEDIUM | determin | Missing `DFP_CANONICAL_NAN` constant | Constants (line 747) |
| D4 | LOW | determin | Struct field order differs from RFC | Data Structures (line 117) |
| S1 | CRITICAL | Stoolap VM | `Op::Div` bypasses DFP dispatch, returns NULL | Expression VM Opcodes |
| S2 | CRITICAL | Stoolap VM | `Op::Mod` bypasses DFP dispatch, returns NULL | Expression VM Opcodes |
| S3 | CRITICAL | Stoolap Value | `into_coerce_to_type()` DFP stub always returns NULL | SQL Integration |
| S4 | HIGH | Stoolap VM | `Op::Neg` for DFP returns NULL | APIs/Interfaces |
| S5 | HIGH | Stoolap VM | No DFP `sqrt` opcode or dispatch | SQRT Algorithm, Mission 1 |
| S6 | HIGH | Stoolap Value | DFP `Display` renders as `<extension:8>` | N/A (usability) |
| S7 | HIGH | Stoolap Value | `as_float64()` returns `None` for DFP | Deterministic Ordering |
| S8 | HIGH | Stoolap Value | `from_typed()` DFP stub stores NULL | SQL Integration |
| S9 | MEDIUM | Stoolap Value | `as_string()` does not handle DFP | SQL Integration |
| S10 | MEDIUM | Stoolap SQL | No DETERMINISTIC VIEW enforcement | SQL Integration, Constraints |
| S11 | MEDIUM | Stoolap Compiler | Zero DFP awareness in expression compiler | Expression VM Opcodes |
| S12 | MEDIUM | Stoolap Ops | No DFP-specific opcodes (DQA has 7) | Expression VM Opcodes |
| S13 | MEDIUM | Stoolap Persistence | No DFP encoding validation on deserialization | Storage Encoding |
| S14 | MEDIUM | Stoolap CastExpr | `perform_cast()` returns Error for DFP | CAST Safety |
| S15 | LOW | Stoolap Tests | Zero DFP integration tests | Mission 1 acceptance criteria |
| S16 | LOW | Stoolap Tests | DFP excluded from datatype unit tests | N/A (test coverage) |

**Totals:** 4 CRITICAL, 5 HIGH, 6 MEDIUM, 3 LOW = 18 findings

---

## Architectural Assessment

The DFP implementation spans two codebases with significantly different maturity levels:

**determin crate** -- Core arithmetic is largely correct. The add, sub, mul algorithms match the RFC. Division works but uses 128 iterations instead of 256 (a spec conformance issue rather than a correctness bug, given the pre-scaling approach). SQRT implements the corrected 226-bit algorithm. The API surface is incomplete (`from_signed`, `DFP_CANONICAL_NAN`).

**Stoolap integration** -- The critical issue is a **dispatch asymmetry**: `Op::Add/Sub/Mul` route through `arithmetic_op()` (which has DFP handling), but `Op::Div/Mod` route through `div_op()/mod_op()` directly (which lack DFP handling). This means DFP addition, subtraction, and multiplication work (both in deterministic and non-deterministic modes), while division and modulo silently return NULL.

The integration gaps (S3-S14) suggest that DFP was wired into the `Value` type system and the `arithmetic_op()` method, but the work was not carried through to the remaining VM paths (Div, Mod, Neg, sqrt), the consuming coercion method, the cast expression, the display layer, or the compiler. The pattern is consistent with an incomplete integration pass.

**Recommendation:** The 3 CRITICAL findings (S1, S2, S3) should be resolved before any DFP-dependent feature is considered usable. The 5 HIGH findings (S4-S8) should be resolved in the same pass, as they represent the same class of incomplete integration.
