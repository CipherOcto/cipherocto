# RFC-0202-B (Storage): Stoolap BIGINT and DECIMAL Conversions

## Status

**Version:** 1.6 (2026-03-31)
**Status:** Draft

## Authors

- Author: @agent

## Maintainers

- Maintainer: @ciphercito

## Summary

This RFC specifies conversion functions between BIGINT (RFC-0110), DECIMAL (RFC-0111), and DQA (RFC-0105) types. This RFC is the **second phase** of the Stoolap numeric tower — it depends on **RFC-0202-A** (Core Types) being implemented first, and on conversion RFCs **0131-0135** being **Accepted**.

Conversions NOT covered by this RFC (handled by other mechanisms):
- INTEGER ↔ BIGINT: handled by Rust `From`/`TryFrom` impls
- DFP ↔ BIGINT/DECIMAL: handled by RFC-0124 (Numeric Lowering, future work)

## Dependencies

**Requires:**

- **RFC-0202-A** (Storage): Stoolap BIGINT and DECIMAL Core Types — **Must be implemented first**
- RFC-0110 (Numeric/Math): Deterministic BIGINT — **Accepted**
- RFC-0111 (Numeric/Math): Deterministic DECIMAL — **Accepted**
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA) — **Accepted** (implemented)
- RFC-0131 (Numeric/Math): BIGINT→DQA Conversion — **Draft** v1.27
- RFC-0132 (Numeric/Math): DQA→BIGINT Conversion — **Draft** v1.23
- RFC-0133 (Numeric/Math): BIGINT→DECIMAL Conversion — **Draft** v1.1
- RFC-0134 (Numeric/Math): DECIMAL→BIGINT Conversion — **Draft** v1.1
- RFC-0135 (Numeric/Math): DECIMAL↔DQA Conversion Review — **Draft** v1.0

**⚠ Critical dependency note:** RFC-0131 and RFC-0132 have a mutual dependency via `BigIntWithScale` (defined in RFC-0132, used in RFC-0131). Both must be Accepted before this RFC's Phase 2 can complete.

**Acceptance criteria for dependency RFCs:** RFCs 0131-0135 must complete adversarial review with zero open issues and receive maintainer approval before advancing from Draft to Accepted. Phase 1 of this RFC is blocked until all five are Accepted.

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | BIGINT↔DQA conversion | Explicit cast between BIGINT and DQA types |
| G2 | BIGINT↔DECIMAL conversion | Explicit cast between BIGINT and DECIMAL types |
| G3 | Lossless conversion enforcement | TRAP on precision-losing conversions |
| G4 | SQL CAST expressions | `CAST(expr AS BIGINT)`, `CAST(expr AS DECIMAL)` |

---

## Conversion Matrix

> **Note:** The implicit type coercion hierarchy (which conversions happen automatically vs require explicit CAST) is defined in RFC-0202-A §6.6. This matrix covers the explicit conversion functions used by CAST expressions and the VM.

| From | To | RFC | Notes |
|------|----|-----|-------|
| BIGINT | DECIMAL | RFC-0133 | Full BigInt→DECIMAL |
| DECIMAL | BIGINT | RFC-0134 | **TRAP if scale > 0** (lossless requires scale=0). `DECIMAL '123.45'` (scale=2) → TRAP; `DECIMAL '123'` (scale=0) → OK. Returns `DecimalError`. **Warning:** The existing `decimal_to_bigint(d: &Decimal) -> Result<i128, DecimalError>` from RFC-0111 silently truncates the fractional part (returns `i128`, NOT `BigInt`). This conversion uses `decimal_to_bigint_full` (RFC-0134) which TRAPs on scale > 0 instead of silently truncating. Use `CAST(DECIMAL ... AS BIGINT)` for explicit conversion. |
| BIGINT | DQA | RFC-0131 | TRAP if exceeds i64 range. Returns `BigIntToDqaError`. |
| DQA | BIGINT | RFC-0132 | Always `Ok(BigInt)` for canonical DQA inputs. Returns `Err(DqaToBigIntError)` for non-canonical. |
| DQA | DECIMAL | RFC-0135 | Existing impl verified correct |
| DECIMAL | DQA | RFC-0135 | **TRAP if scale > 18**. Returns `DecimalError::ConversionLoss`. |
| INTEGER | BIGINT | Via From impl | Always valid |
| BIGINT | INTEGER | Via TryFrom | **TRAP if out of range**. Returns `BigIntError::OutOfRange`. |
| DECIMAL | INTEGER | Via BIGINT | Two-step: `decimal_to_bigint_full` (RFC-0134, TRAP if scale > 0) then `TryFrom<BigInt>` for i64 (TRAP if exceeds i64 range). Error propagation: returns first error encountered — `DecimalError::ConversionLoss` if scale > 0, otherwise `BigIntError::OutOfRange` if BigInt exceeds i64. |
| DECIMAL | String | RFC-0111 | Existing impl |
| i128 | DECIMAL | RFC-0111 | Existing `bigint_to_decimal(value: i128)`. **Note:** takes `i128`, NOT `BigInt`. Consider renaming to `i128_to_decimal` in a future RFC-0111 revision to avoid confusion with `bigint_to_decimal_full`. |
| DECIMAL | i128 | RFC-0111 | Existing `decimal_to_bigint(d: &Decimal) -> Result<i128, DecimalError>`. **Note:** returns `i128`, NOT `BigInt`. This function **silently truncates** the fractional part (scale > 0 is NOT an error) — differs from `decimal_to_bigint_full` (RFC-0134) which TRAPs on scale > 0. |

#### Future Conversions (not yet actionable)

| From | To | RFC | Notes |
|------|----|-----|-------|
| DFP | DECIMAL | RFC-0124 | Via lowering pass (Proposed — not yet actionable) |
| DFP | BIGINT | RFC-0124 | Via lowering pass (Proposed — not yet actionable) |

## Conversion Function Signatures

### RFC-0131 v1.27 — BIGINT→DQA

```rust
/// Error variants for BIGINT→DQA conversion
pub enum BigIntToDqaError {
    /// BigInt value exceeds DQA's representable range (i64::MIN to i64::MAX)
    OutOfRange {
        attempted_magnitude: String,
        max_magnitude: u64,
        scale: u8,
    },
    /// Requested scale exceeds DQA's maximum scale (18)
    InvalidScale { requested: u8, max: u8 },
}

/// Convert BIGINT to DQA with overflow_scale as threshold exponent.
/// CANONICALIZE is applied in Step 4, which may reduce output scale.
pub fn bigint_to_dqa(b: &BigInt, overflow_scale: u8) -> Result<Dqa, BigIntToDqaError>;

/// Round-trip safe conversion that preserves scale metadata.
/// Uses BigIntWithScale from RFC-0132.
pub fn bigint_with_scale_to_dqa(bws: &BigIntWithScale) -> Result<Dqa, BigIntToDqaError>;
```

### RFC-0132 v1.23 — DQA→BIGINT

```rust
pub type DqaToBigIntResult = Result<BigInt, DqaToBigIntError>;

/// Convert DQA to BIGINT. Always succeeds for canonical DQA inputs.
/// Scale is ignored (raw mantissa extraction).
pub fn dqa_to_bigint(dqa: &Dqa) -> DqaToBigIntResult;

/// Value-preserving conversion that retains scale metadata.
pub struct BigIntWithScale {
    pub value: BigInt,
    pub scale: u8,
}

pub fn dqa_to_bigint_with_scale(dqa: &Dqa) -> Result<BigIntWithScale, DqaToBigIntError>;
```

### RFC-0133 v1.1 — BIGINT→DECIMAL

```rust
/// Convert BIGINT to DECIMAL with given scale.
/// TRAPs if scale > 36 or |BigInt × 10^scale| exceeds DECIMAL range.
pub fn bigint_to_decimal_full(b: BigInt, scale: u8) -> Result<Decimal, BigIntError>;
```

### RFC-0134 v1.1 — DECIMAL→BIGINT

```rust
/// Convert DECIMAL to BIGINT. TRAPs if scale > 0 (precision loss).
pub fn decimal_to_bigint_full(d: &Decimal) -> Result<BigInt, DecimalError>;
// Returns DecimalError::ConversionLoss if scale > 0
```

### RFC-0135 v1.0 — DECIMAL↔DQA (Review Only)

Existing implementations verified correct in `determin/src/decimal.rs`:
- `decimal_to_dqa(d: &Decimal) -> Result<Dqa, DecimalError>`
- `dqa_to_decimal(dqa: &Dqa) -> Result<Decimal, DecimalError>`

---

## Implementation Phases

### Phase 1: Accept Conversion RFCs

**Objective:** Ensure all conversion specifications (0131-0135) are Accepted.

- [ ] RFC-0131 + RFC-0132: BIGINT↔DQA Conversion (Draft) — **MUST be Accepted as a pair** due to mutual `BigIntWithScale` dependency
- [ ] RFC-0133: BIGINT→DECIMAL Conversion (Draft v1.1)
- [ ] RFC-0134: DECIMAL→BIGINT Conversion (Draft v1.1)
- [ ] RFC-0135: DECIMAL↔DQA Conversion Review (Draft v1.0 — review only)

> **Note:** RFC-0131 and RFC-0132 MUST be Accepted together due to `BigIntWithScale` cross-RFC type dependency.

### Phase 2: determin Crate Implementation

**Objective:** Implement conversion functions per RFC-0131, RFC-0132, RFC-0133, RFC-0134.

- [ ] Implement `bigint_to_dqa(b: &BigInt, overflow_scale: u8)` per RFC-0131 v1.27
- [ ] Implement `dqa_to_bigint(dqa: &Dqa)` returning `DqaToBigIntResult` per RFC-0132 v1.23
- [ ] Implement `dqa_to_bigint_with_scale(dqa: &Dqa)` per RFC-0132 v1.23
- [ ] Implement `BigIntWithScale` struct per RFC-0132 §Input/Output Contract
- [ ] Implement `bigint_with_scale_to_dqa(bws: &BigIntWithScale)` per RFC-0131 v1.27
- [ ] Implement `bigint_to_decimal_full(b: BigInt, scale: u8)` per RFC-0133 v1.1
- [ ] Implement `decimal_to_bigint_full(d: &Decimal)` per RFC-0134 v1.1
- [ ] Verify all conversions pass RFC test vectors

### Phase 3: Stoolap CAST Integration

**Objective:** Add SQL CAST expressions for numeric conversions.

- [ ] Compile CAST expressions in `src/executor/expression/compiler.rs`: `CAST(expr AS BIGINT)` → `Op::Cast(DataType::Bigint)`, `CAST(expr AS DECIMAL)` → `Op::Cast(DataType::Decimal)`
- [ ] Add BIGINT/DECIMAL cases to `Op::Cast` dispatch in `src/executor/expression/vm.rs` using conversion functions from Phase 2
- [ ] Add error handling for TRAP conditions (e.g., DECIMAL scale > 0 → BIGINT)

---

## Key Files to Modify

### determin crate (external dependency `octo_determin`)

| File | Change |
|------|--------|
| `src/bigint.rs` | Add `bigint_to_dqa`, `bigint_with_scale_to_dqa`, `dqa_to_bigint`, `dqa_to_bigint_with_scale`, `BigIntWithScale` (requires `use crate::dqa::Dqa;`). All conversion functions placed in `bigint.rs` to centralize BigInt-dependent logic — no changes to `dqa.rs` required. Exception: `bigint_to_decimal_full` is placed in `decimal.rs` per RFC-0133's implementation specification. |
| `src/decimal.rs` | Add `bigint_to_decimal_full` (per RFC-0133), `decimal_to_bigint_full`. Rationale: `bigint_to_decimal_full` lives in `decimal.rs` because its output type is `Decimal`, placing it with other `Decimal` constructors. This overrides the general "BigInt-dependent logic in bigint.rs" rule. |

### Stoolap

| File | Change |
|------|--------|
| `src/executor/expression/compiler.rs` | Compile `CAST(expr AS BIGINT)` → `Op::Cast(DataType::Bigint)` and `CAST(expr AS DECIMAL)` → `Op::Cast(DataType::Decimal)` |
| `src/executor/expression/vm.rs` | Add BIGINT/DECIMAL cases to existing `Op::Cast` dispatch for numeric conversions |

---

## Gas Costs

| Conversion | Gas | Source |
|------------|-----|--------|
| `bigint_to_dqa` | 12 (fixed) | RFC-0131 v1.27 §Gas Model |
| `dqa_to_bigint` (NumericTower) | 5 | RFC-0132 v1.23 §Gas Model |
| `dqa_to_bigint` (StandardSql) | 7 | RFC-0132 v1.23 §Gas Model |
| `dqa_to_bigint_with_scale` | 5 | RFC-0132 v1.23 §Gas Model (same as NumericTower) |
| `bigint_with_scale_to_dqa` | 12 (fixed) | RFC-0131 v1.27 §Gas Model (same as bigint_to_dqa) |
| `bigint_to_decimal_full` | 20 + 5 × scale | RFC-0133 v1.1 §Gas Model |
| `decimal_to_bigint_full` | 15 (fixed) | RFC-0134 v1.1 §Gas Model |
| `decimal_to_dqa` | 10 (fixed) | Implementation-defined; to be formalized in RFC-0135 revision |
| `dqa_to_decimal` | 10 (fixed) | Implementation-defined; to be formalized in RFC-0135 revision |

> **Note:** Gas costs are as specified in the cited RFC versions. If those RFCs are revised, these costs must be re-verified. Gas is formula-based (not counter-based) — see RFC-0202-A §8 for the integration model.

---

## Test Vectors

### SQL-Level Integration Tests (CAST Path)

> **Prerequisites:** These test vectors assume RFC-0202-A Phase 1 is complete (BIGINT/DECIMAL DataType variants, typed literals `BIGINT '...'` and `DECIMAL '...'` exist in Stoolap). The `DQA '...'` typed literal syntax is used for test clarity but is not yet formally specified in any RFC — the implementation must define DQA literal parsing as part of this phase or rely on programmatic value construction.

| Test | SQL | Expected |
|------|-----|----------|
| BIGINT → DECIMAL | `CAST(BIGINT '123' AS DECIMAL)` | `DECIMAL '123'` (scale=0) |
| DECIMAL → BIGINT (scale=0) | `CAST(DECIMAL '123' AS BIGINT)` | `BIGINT '123'` |
| DECIMAL → BIGINT (TRAP) | `CAST(DECIMAL '123.45' AS BIGINT)` | Error: `DecimalError::ConversionLoss` (scale > 0) |
| BIGINT → DQA (in range) | `CAST(BIGINT '42' AS DQA(0))` | `DQA '42'` |
| BIGINT → DQA (overflow) | `CAST(BIGINT '9223372036854775808' AS DQA(0))` | Error: `BigIntToDqaError::OutOfRange` (exceeds i64) |
| DQA → BIGINT | `CAST(DQA '12345' AS BIGINT)` | `BIGINT '12345'` (raw mantissa) |
| DECIMAL → DQA (scale ≤ 18) | `CAST(DECIMAL '1.5' AS DQA)` | `DQA '1.5'` |
| DECIMAL → DQA (TRAP) | `CAST(DECIMAL '1.0000000000000000001' AS DQA)` (scale=19) | Error: `DecimalError::ConversionLoss` (scale > 18) |
| INTEGER → BIGINT | `CAST(42 AS BIGINT)` | `BIGINT '42'` |
| INTEGER → DECIMAL | `CAST(42 AS DECIMAL)` | `DECIMAL '42'` (scale=0) |
| BIGINT → INTEGER (in range) | `CAST(BIGINT '42' AS INTEGER)` | `42` (i64) |
| BIGINT → INTEGER (TRAP) | `CAST(BIGINT '99999999999999999999' AS INTEGER)` | Error: `BigIntError::OutOfRange` |
| DECIMAL → INTEGER (via BIGINT) | `CAST(DECIMAL '123' AS INTEGER)` | `123` (i64) |
| DECIMAL → INTEGER (TRAP scale) | `CAST(DECIMAL '123.45' AS INTEGER)` | Error: `DecimalError::ConversionLoss` (two-step fails at DECIMAL→BIGINT: scale > 0) |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.6 | 2026-03-31 | Adversarial review round 6: C1 (DECIMAL→BIGINT error: `DecimalError::ConversionLoss`), C2 (distinguish `decimal_to_bigint` truncating vs `decimal_to_bigint_full` strict), H2 (DQA→BIGINT clarify `Result` return), H3 (replace invalid `DECIMAL '1e19'` with valid literal), H4 (`bigint_to_decimal_full` file placement rationale), M1 (DFP rows moved to Future Conversions section), M3 (acceptance criteria for RFC-0131-0135), M4 (DECIMAL→INTEGER error propagation), M5 (DQA literal syntax prerequisite note), L1 (`bigint_to_decimal` rename recommendation), L2 (`decimal_to_bigint` truncation warning). |
| 1.5 | 2026-03-30 | Adversarial review round 5: H1 (cast.rs → vm.rs), H2 (ddl.rs → compiler.rs), H3 (determin crate is external dep), H4 (RFC-0135 gas costs marked implementation-defined), M1 (RFC-0124 Proposed annotation), M2 (Phase 3 compiler.rs step), M3 (centralization rationale exception for bigint_to_decimal_full), M4 (DECIMAL→INTEGER path in conversion matrix), M5 (SQL-level integration test vectors section), L1 (RFC-0105 status → Accepted (implemented)), L2 (DECIMAL→DQA error type: DecimalError::ConversionLoss). |
| 1.4 | 2026-03-30 | Adversarial review round 4: M2 (param name `v`→`bws` matching RFC-0131), M3/M4 (add gas costs for `bigint_with_scale_to_dqa` and `dqa_to_bigint_with_scale`), L3 (conversion matrix example: `DECIMAL '123.00'`→`DECIMAL '123'`), L4 (clarify file placement — all conversions in bigint.rs, no dqa.rs changes). |
| 1.3 | 2026-03-30 | Adversarial review round 3: fix dqa_to_decimal return type (Result, not bare Decimal), add gas costs for RFC-0135 conversions, add cross-module import note |
| 1.2 | 2026-03-30 | Adversarial review round 2: fix bigint_to_decimal/decimal_to_bigint naming (i128 vs BigInt), add gas cost cross-references, merge RFC-0131/0132 into atomic acceptance |
| 1.1 | 2026-03-30 | Fix category reference to RFC-0202-A (Storage), add coercion hierarchy cross-reference |
| 1.0 | 2026-03-28 | Initial draft — conversions only, core types are RFC-0202-A |

---

## Related RFCs

- **RFC-0202-A** (Storage): Stoolap BIGINT and DECIMAL Core Types (prerequisite)
- RFC-0104 (Numeric/Math): Deterministic Floating-Point (DFP)
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA)
- RFC-0110 (Numeric/Math): Deterministic BIGINT
- RFC-0111 (Numeric/Math): Deterministic DECIMAL
- RFC-0124 (Numeric/Math): Deterministic Numeric Lowering (future work)
- RFC-0131 (Numeric/Math): BIGINT→DQA Conversion
- RFC-0132 (Numeric/Math): DQA→BIGINT Conversion
- RFC-0133 (Numeric/Math): BIGINT→DECIMAL Conversion
- RFC-0134 (Numeric/Math): DECIMAL→BIGINT Conversion
- RFC-0135 (Numeric/Math): DECIMAL↔DQA Conversion Review

> **Note:** RFC-0135 exists in both `numeric/` (DECIMAL↔DQA Conversion) and `proof-systems/` (Proof Format Standard). This RFC references the numeric version.

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
