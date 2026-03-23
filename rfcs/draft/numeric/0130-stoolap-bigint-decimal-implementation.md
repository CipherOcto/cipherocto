# RFC-0130 (Numeric/Math): Stoolap BIGINT and DECIMAL Implementation

## Status

**Version:** 1.0 (2026-03-23)
**Status:** Draft

## Authors

- Author: @agent

## Maintainers

- Maintainer: @ciphercito

## Summary

This RFC specifies the implementation of BIGINT (RFC-0110) and DECIMAL (RFC-0111) types in Stoolap, completing the CipherOcto Numeric Tower integration. The implementation adds arbitrary-precision integers (up to 4096 bits) and high-precision decimals (i128 with 0-36 scale) to Stoolap's type system, with full conversion support to/from existing types (DFP, DQA, IEEE-754 Float).

## Dependencies

**Requires:**

- RFC-0104 (Numeric/Math): Deterministic Floating-Point (DFP) — Implemented in Stoolap
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA) — Implemented in Stoolap
- RFC-0110 (Numeric/Math): Deterministic BIGINT — Accepted (reference spec)
- RFC-0111 (Numeric/Math): Deterministic DECIMAL — Accepted (reference spec)

**Optional:**

- RFC-0124 (Numeric/Math): Deterministic Numeric Lowering — DFP→DQA lowering

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Implement BIGINT type | SQL keyword `BIGINT` parsed to new type |
| G2 | Implement DECIMAL type | SQL keyword `DECIMAL`/`NUMERIC` parsed to new type |
| G3 | Conversion functions | Explicit casts between all numeric types |
| G4 | Lazy canonicalization | Zero = `{0, 0}`, trailing zeros stripped |
| G5 | Gas metering | Consistent with DQA/DFP gas model |

## Motivation

The CipherOcto Numeric Tower requires BIGINT and DECIMAL for financial and high-precision computing use cases. Stoolap currently lacks these types despite accepted RFCs:

- **BIGINT**: Arbitrary precision integers (up to 4096 bits) for exact arithmetic without overflow concerns
- **DECIMAL**: i128 scaled integers (0-36 scale) for financial calculations where IEEE-754 binary floating-point introduces unacceptable rounding errors

The existing Stoolap types:
- `INTEGER` → i64 (fixed precision)
- `FLOAT`/`DECIMAL`/`NUMERIC` → IEEE-754 (rounding errors)
- `DFP` → 113-bit deterministic floating-point
- `DQA` → i64 with 0-18 scale

Neither BIGINT nor DECIMAL exist, creating a critical gap in the Numeric Tower.

## Specification

### 4.1 SQL Keyword Mapping

Stoolap parser (`src/core/types.rs`) currently maps:

```rust
"FLOAT" | "DOUBLE" | "REAL" | "DECIMAL" | "NUMERIC" => DataType::Float
```

**Required changes:**

```rust
match upper.as_str() {
    // Existing
    "INTEGER" | "INT" | "BIGINT" | "SMALLINT" | "TINYINT" => DataType::Integer,
    "FLOAT" | "DOUBLE" | "REAL" => DataType::Float,
    "DFP" | "DETERMINISTICFLOAT" => DataType::DeterministicFloat,
    "DQA" => DataType::Quant,
    // New
    "BIGINT" => DataType::Bigint,
    "DECIMAL" | "NUMERIC" => DataType::Decimal,
}
```

### 4.2 DataType Enum Extension

```rust
// src/core/types.rs

pub enum DataType {
    // ... existing variants ...
    Bigint,
    Decimal { scale: u8 },  // scale 0-36
}
```

Note: `DECIMAL` and `NUMERIC` without explicit scale default to scale 0 (equivalent to BIGINT behavior for literals like `DECIMAL '123'`).

### 4.3 Value Representation

#### BIGINT

```rust
pub struct BigintValue {
    limbs: Vec<u64>,  // Little-endian, 1 limb = 64 bits
    sign: bool,       // true = negative
}
```

- Maximum size: 4096 bits (64 limbs)
- Canonical form: No leading zero limbs, `{0}` for zero value
- Serialization: Big-endian bytes with length prefix

#### DECIMAL

```rust
pub struct DecimalValue {
    mantissa: i128,
    scale: u8,  // 0-36
}
```

- Mantissa range: -(10^36 - 1) to (10^36 - 1)
- Scale range: 0-36
- Canonical form: Zero = `{0, 0}`, trailing zeros stripped from mantissa

### 4.4 POW10 Table

```rust
const DECIMAL_POW10: [i128; 37] = [
    1_i128,
    10_i128,
    100_i128,
    // ... through 10^36
    1_000_000_000_000_000_000_000_000_000_000_000_000_i128,
];
```

### 4.5 Serialization Format

#### BIGINT Wire Format

| Field | Size | Encoding |
|-------|------|----------|
| Length | 1 byte | Number of 8-byte limbs (1-64) |
| Sign | 1 byte | 0x00 = positive, 0x01 = negative |
| Limbs | N×8 bytes | Little-endian u64 array |

#### DECIMAL Wire Format

| Field | Size | Encoding |
|-------|------|----------|
| Mantissa | 16 bytes | Big-endian i128 |
| Scale | 1 byte | 0-36 |
| Reserved | 7 bytes | Must be zero (canonical check) |

### 4.6 Conversion Matrix

| From | To | Method | Notes |
|------|----|--------|-------|
| BIGINT | DECIMAL | `DECIMAL(BIGINT, 0)` | Scale 0 |
| DECIMAL | BIGINT | Truncate to integer | Error if scale > 0 or precision loss |
| BIGINT | DQA | `DQA(BIGINT, 0)` | Only if fits in i64 range |
| DQA | DECIMAL | `DECIMAL(DQA.mantissa, DQA.scale)` | Scale preserved if ≤ 36 |
| DECIMAL | DQA | Truncate scale | Error if scale > 18 |
| DFP | DECIMAL | `DECIMAL(DFP mantissa, DFP exponent)` | Via lowering pass |
| DFP | BIGINT | Truncate to integer | Via lowering pass |
| INTEGER | BIGINT | Zero-extend | Always valid |
| BIGINT | INTEGER | Truncate | Error if out of range |

### 4.7 Arithmetic Operations

BIGINT arithmetic follows RFC-0110 exactly:

- ADD: i256 intermediate, result truncated to 4096 bits
- SUB: i256 intermediate, result truncated to 4096 bits
- MUL: Schoolbook or Karatsuba based on size
- DIV: Truncating division (toward zero)
- MOD: Remainder matching dividend sign
- CMP: Compare as magnitudes first, then signs

DECIMAL arithmetic follows RFC-0111 exactly:

- Scale alignment via multiplication by POW10
- RNE rounding on division
- Canonicalization after every operation

### 4.8 Gas Model

| Operation | Gas Units |
|-----------|-----------|
| BIGINT ADD/SUB | 10 |
| BIGINT MUL | 50 |
| BIGINT DIV | 100 |
| BIGINT CMP | 5 |
| DECIMAL ADD/SUB | 15 (includes scale alignment) |
| DECIMAL MUL | 80 |
| DECIMAL DIV | 150 |
| Conversion | 20 |

## Type Gap Analysis

### Current Stoolap State

| Type | SQL Keyword | Internal | Status |
|------|-------------|----------|--------|
| INTEGER | INTEGER, INT, SMALLINT, TINYINT | i64 | Implemented |
| FLOAT | FLOAT, DOUBLE, REAL | IEEE-754 f64 | Implemented |
| DFP | DFP, DETERMINISTICFLOAT | 113-bit | Implemented |
| DQA | DQA | i64 + scale | Implemented |
| BIGINT | BIGINT | BigintValue | **Missing** |
| DECIMAL | DECIMAL, NUMERIC | DecimalValue | **Missing** |

### Target State

| Type | SQL Keyword | Internal | Status |
|------|-------------|----------|--------|
| INTEGER | INTEGER, INT, SMALLINT, TINYINT | i64 | Implemented |
| BIGINT | BIGINT | Vec<u64> (≤4096 bits) | Implemented |
| FLOAT | FLOAT, DOUBLE, REAL | IEEE-754 f64 | Implemented |
| DFP | DFP, DETERMINISTICFLOAT | 113-bit | Implemented |
| DQA | DQA | i64 + scale (0-18) | Implemented |
| DECIMAL | DECIMAL, NUMERIC | i128 + scale (0-36) | Implemented |

## Implementation Phases

### Phase 1: Core BIGINT

- [ ] Add `DataType::Bigint` variant to enum
- [ ] Implement `BigintValue` struct with limb vector
- [ ] Implement canonical form enforcement
- [ ] Implement serialization/deserialization
- [ ] Implement ADD, SUB, CMP operations
- [ ] Add BIGINT SQL parser support

### Phase 2: Core DECIMAL

- [ ] Add `DataType::Decimal { scale: u8 }` variant
- [ ] Implement `DecimalValue` struct
- [ ] Implement POW10 table
- [ ] Implement canonicalization (trailing zeros)
- [ ] Implement serialization/deserialization
- [ ] Implement ADD, SUB, MUL, DIV operations
- [ ] Add DECIMAL/NUMERIC SQL parser support

### Phase 3: Conversions

- [ ] BIGINT ↔ DECIMAL conversion
- [ ] BIGINT ↔ DQA conversion (bounds check)
- [ ] DECIMAL ↔ DQA conversion
- [ ] BIGINT ↔ INTEGER conversion
- [ ] DECIMAL literal parsing with explicit scale

### Phase 4: Integration

- [ ] Expression VM support for new types
- [ ] Cost-based optimizer estimates
- [ ] Semantic query caching compatibility
- [ ] Gas metering integration
- [ ] Integration tests with RFC-0110/RFC-0111 vectors

## Security Considerations

### Overflow Handling

- BIGINT operations that exceed 4096 bits MUST return TRAP
- DECIMAL overflow MUST return error, not wrap
- Division by zero MUST return error

### Canonicalization Enforcement

- Deserialized values MUST be checked for canonical form
- Non-canonical representations MUST be rejected with `NonCanonical` error
- Zero MUST be represented as `{0, 0}` for DECIMAL

### Determinism Requirements

- All operations MUST be deterministic across implementations
- No reliance on host architecture byte order (big-endian serialization)
- Round-trip conversions MUST preserve exact values

## Test Vectors

### BIGINT Tests

| Input | Operation | Expected |
|-------|-----------|----------|
| `BIGINT '0'` | Canonical | `{0}` |
| `BIGINT '12345678901234567890'` | Parse | limbs = [0x4F3B5E2F2A, 0xAB54A6] |
| `BIGINT '-1'` | Parse | sign = true, limbs = [1] |
| `2^4095 + 1` | Overflow check | TRAP |

### DECIMAL Tests

| Input | Operation | Expected |
|-------|-----------|----------|
| `DECIMAL '0'` | Canonical | mantissa = 0, scale = 0 |
| `DECIMAL '123.4500'` | Canonical | mantissa = 12345, scale = 2 |
| `DECIMAL '1.25'` | ADD `DECIMAL '0.75'` | mantissa = 200, scale = 2 (2.00) |
| Scale 37 | Parse | Error: InvalidScale |

### Conversion Tests

| From | To | Input | Expected |
|------|----|-------|----------|
| BIGINT | DECIMAL | 42 | mantissa = 42, scale = 0 |
| DECIMAL (scale=2) | BIGINT | 123.45 | Error: scale > 0 |
| DECIMAL (scale=0) | BIGINT | 123 | 123 |
| DQA | DECIMAL | {mantissa=1234, scale=6} | mantissa=1234, scale=6 |

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| Use i256 only | Simpler implementation | Limited to 256 bits |
| Use bigdecimal crate | Complete solution | Not deterministic, external dependency |
| IEEE-754 Decimal | Standard | Not deterministic for all operations |
| **Custom Bigint + Decimal** | Deterministic, no deps | More implementation work |

## Key Files to Modify

| File | Change |
|------|--------|
| `src/core/types.rs` | Add Bigint and Decimal variants to DataType enum |
| `src/core/value.rs` | Add BigintValue and DecimalValue structs |
| `src/parser/lexer.rs` | Add BIGINT, DECIMAL, NUMERIC tokens |
| `src/parser/ast.rs` | Add BigintLiteral, DecimalLiteral variants |
| `src/expression/vm.rs` | Add Bigint and Decimal execution support |
| `src/functions/arithmetic.rs` | Implement BIGINT/DECIMAL operations |
| `src/storage/serialize.rs` | Add serialization for new types |
| `src/optimizer/cost.rs` | Add cost estimates for new types |

## Future Work

- F1: RFC-0124 DFP→DQA lowering integration with DECIMAL
- F2: DECIMAL aggregate functions (SUM, AVG with exact arithmetic)
- F3: Vectorized BIGINT/DECIMAL operations for analytical queries

## Rationale

BIGINT and DECIMAL are essential for financial and high-precision computing. The existing IEEE-754 FLOAT type introduces rounding errors unacceptable for monetary calculations. The DFP type provides deterministic floating-point but lacks the integer precision needed for large values. DQA provides scale but is limited to i64 range.

By implementing RFC-0110 and RFC-0111 as specified, we gain:
- Arbitrary precision integers without overflow concerns
- Decimal arithmetic with exact rounding (no binary floating-point errors)
- Full conversion support to/from existing Numeric Tower types

The custom implementation (rather than external crates) ensures determinism and avoids dependency issues.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-23 | Initial draft |

## Related RFCs

- RFC-0104 (Numeric/Math): Deterministic Floating-Point (DFP)
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA)
- RFC-0110 (Numeric/Math): Deterministic BIGINT
- RFC-0111 (Numeric/Math): Deterministic DECIMAL
- RFC-0124 (Numeric/Math): Deterministic Numeric Lowering

## Related Use Cases

- [Enhanced Quota Router Gateway](../../docs/use-cases/enhanced-quota-router-gateway.md)

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
