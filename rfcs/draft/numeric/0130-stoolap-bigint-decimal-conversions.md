# RFC-0130-B (Numeric/Math): BIGINT and DECIMAL Conversions

## Status

**Version:** 1.0 (2026-03-28)
**Status:** Draft

## Authors

- Author: @agent

## Maintainers

- Maintainer: @ciphercito

## Summary

This RFC specifies conversion functions between BIGINT (RFC-0110), DECIMAL (RFC-0111), and DQA (RFC-0105) types. This RFC is the **second phase** of the Stoolap numeric tower — it depends on **RFC-0130-A** (Core Types) being implemented first, and on conversion RFCs **0131-0135** being **Accepted**.

Conversions NOT covered by this RFC (handled by other mechanisms):
- INTEGER ↔ BIGINT: handled by Rust `From`/`TryFrom` impls
- DFP ↔ BIGINT/DECIMAL: handled by RFC-0124 (Numeric Lowering, future work)

## Dependencies

**Requires:**

- **RFC-0130-A** (Numeric/Math): Stoolap BIGINT and DECIMAL Core Types — **Must be implemented first**
- RFC-0110 (Numeric/Math): Deterministic BIGINT — **Accepted**
- RFC-0111 (Numeric/Math): Deterministic DECIMAL — **Accepted**
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA) — Implemented
- RFC-0131 (Numeric/Math): BIGINT→DQA Conversion — **Draft** v1.27
- RFC-0132 (Numeric/Math): DQA→BIGINT Conversion — **Draft** v1.23
- RFC-0133 (Numeric/Math): BIGINT→DECIMAL Conversion — **Draft** v1.1
- RFC-0134 (Numeric/Math): DECIMAL→BIGINT Conversion — **Draft** v1.1
- RFC-0135 (Numeric/Math): DECIMAL↔DQA Conversion Review — **Draft** v1.0

**⚠ Critical dependency note:** RFC-0131 and RFC-0132 have a mutual dependency via `BigIntWithScale` (defined in RFC-0132, used in RFC-0131). Both must be Accepted before this RFC's Phase 2 can complete.

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | BIGINT↔DQA conversion | Explicit cast between BIGINT and DQA types |
| G2 | BIGINT↔DECIMAL conversion | Explicit cast between BIGINT and DECIMAL types |
| G3 | Lossless conversion enforcement | TRAP on precision-losing conversions |
| G4 | SQL CAST expressions | `CAST(expr AS BIGINT)`, `CAST(expr AS DECIMAL)` |

---

## Conversion Matrix

| From | To | RFC | Notes |
|------|----|-----|-------|
| BIGINT | DECIMAL | RFC-0133 | Full BigInt→DECIMAL |
| DECIMAL | BIGINT | RFC-0134 | **TRAP if scale > 0** (lossless requires scale=0). `DECIMAL '123.45'` (scale=2) → TRAP; `DECIMAL '123.00'` (scale=0) → OK. Returns `DecimalError`. |
| BIGINT | DQA | RFC-0131 | TRAP if exceeds i64 range. Returns `BigIntToDqaError`. |
| DQA | BIGINT | RFC-0132 | Always valid for canonical DQA inputs |
| DQA | DECIMAL | RFC-0135 | Existing impl verified correct |
| DECIMAL | DQA | RFC-0135 | **TRAP if scale > 18**. Returns error. |
| DFP | DECIMAL | RFC-0124 | Via lowering pass (future work) |
| DFP | BIGINT | RFC-0124 | Via lowering pass (future work) |
| INTEGER | BIGINT | Via From impl | Always valid |
| BIGINT | INTEGER | Via TryFrom | **TRAP if out of range**. Returns `TryFromBigIntError`. |
| DECIMAL | String | RFC-0111 | Existing impl |
| i128 | DECIMAL | RFC-0111 | Existing `bigint_to_decimal(i128)` |
| DECIMAL | i128 | RFC-0111 | Existing `decimal_to_bigint` |

---

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
pub fn bigint_with_scale_to_dqa(v: &BigIntWithScale) -> Result<Dqa, BigIntToDqaError>;
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
- `dqa_to_decimal(dqa: &Dqa) -> Decimal`

---

## Implementation Phases

### Phase 1: Accept Conversion RFCs

**Objective:** Ensure all conversion specifications (0131-0135) are Accepted.

- [ ] RFC-0131: BIGINT→DQA Conversion (Draft v1.27) — **mutual dependency with RFC-0132**
- [ ] RFC-0132: DQA→BIGINT Conversion (Draft v1.23) — **mutual dependency with RFC-0131**
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
- [ ] Implement `bigint_with_scale_to_dqa(v: &BigIntWithScale)` per RFC-0131 v1.27
- [ ] Implement `bigint_to_decimal_full(b: BigInt, scale: u8)` per RFC-0133 v1.1
- [ ] Implement `decimal_to_bigint_full(d: &Decimal)` per RFC-0134 v1.1
- [ ] Verify all conversions pass RFC test vectors

### Phase 3: Stoolap CAST Integration

**Objective:** Add SQL CAST expressions for numeric conversions.

- [ ] Add CAST parsing for `CAST(expr AS BIGINT)`, `CAST(expr AS DECIMAL)`
- [ ] Add CAST evaluation using conversion functions from Phase 2
- [ ] Add error handling for TRAP conditions (e.g., DECIMAL scale > 0 → BIGINT)

---

## Key Files to Modify

### determin crate

| File | Change |
|------|--------|
| `src/bigint.rs` | Add `bigint_to_dqa`, `dqa_to_bigint`, `dqa_to_bigint_with_scale`, `BigIntWithScale` |
| `src/decimal.rs` | Add `bigint_to_decimal_full`, `decimal_to_bigint_full` |

### Stoolap

| File | Change |
|------|--------|
| `src/executor/ddl.rs` | Add CAST parsing for BIGINT/DECIMAL types |
| `src/executor/expression/cast.rs` | Add CAST evaluation for numeric conversions |

---

## Gas Costs

| Conversion | Gas |
|------------|-----|
| `bigint_to_dqa` | 12 (fixed) |
| `dqa_to_bigint` (NumericTower) | 5 |
| `dqa_to_bigint` (StandardSql) | 7 |
| `bigint_to_decimal_full` | 20 + 5 × scale |
| `decimal_to_bigint_full` | 15 (fixed) |

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-28 | Initial draft — conversions only, core types are RFC-0130-A |

---

## Related RFCs

- **RFC-0130-A** (Numeric/Math): Stoolap BIGINT and DECIMAL Core Types (prerequisite)
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

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
