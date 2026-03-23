# RFC-0130 (Numeric/Math): Stoolap BIGINT and DECIMAL Implementation

## Status

**Version:** 1.2 (2026-03-23)
**Status:** Draft

## Authors

- Author: @agent

## Maintainers

- Maintainer: @ciphercito

## Summary

This RFC specifies the implementation of BIGINT (RFC-0110) and DECIMAL (RFC-0111) types in Stoolap, completing the CipherOcto Numeric Tower integration. BIGINT provides arbitrary-precision integers (up to 4096 bits) and DECIMAL provides high-precision decimals (i128 with 0-36 scale). Implementation uses the `determin` crate for core algorithms and adds Stoolap-specific integration.

## Dependencies

**Requires:**

- RFC-0104 (Numeric/Math): Deterministic Floating-Point (DFP) — Implemented in Stoolap
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA) — Implemented in Stoolap
- RFC-0110 (Numeric/Math): Deterministic BIGINT — **Accepted** (reference spec, algorithms in `determin` crate)
- RFC-0111 (Numeric/Math): Deterministic DECIMAL — **Accepted** (reference spec, algorithms in `determin` crate)
- RFC-0131 (Numeric/Math): BIGINT→DQA Conversion — **Draft** (conversion spec)
- RFC-0132 (Numeric/Math): DQA→BIGINT Conversion — **Draft** (conversion spec)
- RFC-0133 (Numeric/Math): BIGINT→DECIMAL Conversion — **Draft** (conversion spec)
- RFC-0134 (Numeric/Math): DECIMAL→BIGINT Conversion — **Draft** (conversion spec)
- RFC-0135 (Numeric/Math): DECIMAL↔DQA Conversion Review — **Draft** (review of existing functions)

**Optional:**

- RFC-0124 (Numeric/Math): Deterministic Numeric Lowering — DFP→DQA→BIGINT lowering (future work)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | BIGINT type in Stoolap | SQL keyword `BIGINT` parsed to `DataType::Bigint` |
| G2 | DECIMAL type in Stoolap | SQL keyword `DECIMAL`/`NUMERIC` parsed to `DataType::Decimal` |
| G3 | Conversion functions | Explicit casts between all numeric types |
| G4 | Canonical serialization | Wire format matches RFC-0110/RFC-0111 exactly |
| G5 | Gas metering | Consistent with determin crate gas model |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Stoolap                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ src/core/   │  │ src/core/   │  │ src/executor/expression/ │  │
│  │ types.rs    │  │ value.rs    │  │ vm.rs                  │  │
│  │             │  │             │  │                        │  │
│  │ DataType::  │  │ Value::     │  │ BIGINT/DECIMAL ops     │  │
│  │ Bigint      │  │ Extension   │  │ via determin crate     │  │
│  │ Decimal     │  │ (encodes    │  │                        │  │
│  │             │  │  determin   │  │                        │  │
│  │ (10, 11)   │  │  types)     │  │                        │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     determin crate                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ bigint.rs   │  │ decimal.rs  │  │ dqa.rs                │  │
│  │             │  │             │  │                       │  │
│  │ RFC-0110    │  │ RFC-0111     │  │ RFC-0105              │  │
│  │ algorithms  │  │ algorithms  │  │ (DQA↔DECIMAL)         │  │
│  │             │  │             │  │                       │  │
│  │ BIGINT ops  │  │ DECIMAL ops  │  │ Conversions            │  │
│  │ serialization│  │ serialization│  │                       │  │
│  │ gas costs   │  │ gas costs   │  │                       │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

**Key principle:** Core algorithms (RFC-0110/RFC-0111) live in `determin` crate. Stoolap integration adds SQL parsing, type system integration, and VM execution.

---

## Specification

### 1. DataType Enum Extension (Stoolap)

**File:** `src/core/types.rs`

**Required changes to `DataType` enum:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum DataType {
    // ... existing variants (0-9) ...
    Null = 0,
    Integer = 1,
    Float = 2,
    Text = 3,
    Boolean = 4,
    Timestamp = 5,
    Json = 6,
    Vector = 7,
    DeterministicFloat = 8,
    Quant = 9,

    // NEW variants (10-11)
    /// Deterministic BIGINT per RFC-0110
    /// Arbitrary precision integer (up to 4096 bits)
    Bigint = 10,

    /// Deterministic DECIMAL per RFC-0111
    /// i128 scaled integer with 0-36 decimal places
    Decimal = 11,
}
```

**Updated `FromStr` implementation:**

```rust
impl FromStr for DataType {
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let upper = s.to_uppercase();
        if upper.starts_with("VECTOR") {
            return Ok(DataType::Vector);
        }
        if upper.starts_with("DQA") {
            return Ok(DataType::Quant);
        }
        match upper.as_str() {
            "NULL" => Ok(DataType::Null),
            "INTEGER" | "INT" | "SMALLINT" | "TINYINT" => Ok(DataType::Integer),
            "BIGINT" => Ok(DataType::Bigint),           // NEW: was mapped to Integer
            "FLOAT" | "DOUBLE" | "REAL" => Ok(DataType::Float),
            "DECIMAL" | "NUMERIC" => Ok(DataType::Decimal),    // NEW: separate from Float
            "TEXT" | "VARCHAR" | "CHAR" | "STRING" => Ok(DataType::Text),
            "BOOLEAN" | "BOOL" => Ok(DataType::Boolean),
            "TIMESTAMP" | "DATETIME" | "DATE" | "TIME" => Ok(DataType::Timestamp),
            "JSON" | "JSONB" => Ok(DataType::Json),
            "DFP" | "DETERMINISTICFLOAT" => Ok(DataType::DeterministicFloat),
            _ => Err(Error::InvalidColumnType),
        }
    }
}
```

**Updated `as_u8` and `from_u8`:**

```rust
impl DataType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(DataType::Null),
            1 => Some(DataType::Integer),
            2 => Some(DataType::Float),
            3 => Some(DataType::Text),
            4 => Some(DataType::Boolean),
            5 => Some(DataType::Timestamp),
            6 => Some(DataType::Json),
            7 => Some(DataType::Vector),
            8 => Some(DataType::DeterministicFloat),
            9 => Some(DataType::Quant),
            10 => Some(DataType::Bigint),    // NEW
            11 => Some(DataType::Decimal),     // NEW
            _ => None,
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // ... existing matches ...
            DataType::DeterministicFloat => write!(f, "DFP"),
            DataType::Quant => write!(f, "DQA"),
            DataType::Bigint => write!(f, "BIGINT"),    // NEW
            DataType::Decimal => write!(f, "DECIMAL"),   // NEW
        }
    }
}
```

---

### 2. Value Type Extension (Stoolap)

**File:** `src/core/value.rs`

**Extension variant usage for BIGINT and DECIMAL:**

BIGINT and DECIMAL values are stored in the `Extension` variant using the determin crate's canonical serialization formats. This avoids duplicating type definitions.

```rust
// In src/core/value.rs, extend the import:
use octo_determin::{BigInt, Decimal, Dfp, DfpClass, DfpEncoding, Dqa};

// Add constructors:
impl Value {
    /// Create a BIGINT value from a determin crate BigInt
    pub fn bigint(b: BigInt) -> Self {
        let encoding = b.serialize(); // Returns BigIntEncoding per RFC-0110
        let mut bytes = Vec::with_capacity(1 + encoding.len());
        bytes.push(DataType::Bigint as u8);
        bytes.extend_from_slice(&encoding.to_bytes());
        Value::Extension(CompactArc::from(bytes))
    }

    /// Create a DECIMAL value from a determin crate Decimal
    pub fn decimal(d: Decimal) -> Self {
        let encoding = d.to_bytes(); // Returns [u8; 24] per RFC-0111
        let mut bytes = Vec::with_capacity(1 + 24);
        bytes.push(DataType::Decimal as u8);
        bytes.extend_from_slice(&encoding);
        Value::Extension(CompactArc::from(bytes))
    }

    /// Extract BIGINT as determin crate BigInt
    pub fn as_bigint(&self) -> Option<BigInt> {
        match self {
            Value::Extension(data)
                if data.first().copied() == Some(DataType::Bigint as u8) =>
            {
                let encoding_bytes = &data[1..];
                BigInt::deserialize(encoding_bytes).ok()
            }
            _ => None,
        }
    }

    /// Extract DECIMAL as determin crate Decimal
    pub fn as_decimal(&self) -> Option<Decimal> {
        match self {
            Value::Extension(data)
                if data.first().copied() == Some(DataType::Decimal as u8) =>
            {
                let encoding_bytes: [u8; 24] = data[1..25].try_into().ok()?;
                Decimal::from_bytes(encoding_bytes).ok()
            }
            _ => None,
        }
    }
}
```

---

### 3. Serialization Formats (determin crate)

These are defined in RFC-0110 and RFC-0111. The determin crate provides the canonical implementations.

#### BIGINT Wire Format (RFC-0110 §Canonical Byte Format)

```
┌─────────────────────────────────────────────────────────────┐
│ Byte 0: Version (0x01)                                      │
│ Byte 1: Sign (0x00 = positive, 0xFF = negative)            │
│ Bytes 2-3: Reserved (0x0000)                              │
│ Byte 4: Number of limbs (u8, range 1–64)                 │
│ Bytes 5-7: Reserved (MUST be 0x00)                        │
│ Byte 8+: Limb array (little-endian u64 × num_limbs)       │
└─────────────────────────────────────────────────────────────┘
```

**Maximum size:** 8 + (64 × 8) = 520 bytes

#### DECIMAL Wire Format (RFC-0111 §Canonical Byte Format)

```
┌─────────────────────────────────────────────────────────────┐
│ Byte 0: Version (0x01)                                     │
│ Byte 1: Reserved (MUST be 0x00)                           │
│ Bytes 2-3: Reserved (MUST be 0x00)                        │
│ Byte 4: Scale (u8, range 0-36)                            │
│ Bytes 5-7: Reserved (MUST be 0x00)                        │
│ Bytes 8-23: Mantissa (i128 big-endian, two's complement)  │
└─────────────────────────────────────────────────────────────┘
```

**Total size:** 24 bytes

---

### 4. Conversion Matrix

Conversion specifications are defined in separate RFCs:

| From | To | RFC | Notes |
|------|----|-----|-------|
| BIGINT | DECIMAL | RFC-0133 | Full BigInt→DECIMAL |
| DECIMAL | BIGINT | RFC-0134 | TRAP if scale > 0 |
| BIGINT | DQA | RFC-0131 | TRAP if exceeds i64 range |
| DQA | BIGINT | RFC-0132 | Always valid |
| DQA | DECIMAL | RFC-0135 | Existing impl verified correct |
| DECIMAL | DQA | RFC-0135 | TRAP if scale > 18 |
| DFP | DECIMAL | RFC-0124 | Via lowering pass |
| DFP | BIGINT | RFC-0124 | Via lowering pass |
| INTEGER | BIGINT | Via From impl | Always valid |
| BIGINT | INTEGER | Via TryFrom | TRAP if out of range |
| DECIMAL | String | RFC-0111 | Existing impl |
| i128 | DECIMAL | RFC-0111 | Existing `bigint_to_decimal(i128)` |
| DECIMAL | i128 | RFC-0111 | Existing `decimal_to_bigint` |

---

### 5. Arithmetic Operations

All arithmetic operations use the determin crate implementations:

| Operation | BIGINT Function | DECIMAL Function |
|-----------|----------------|------------------|
| ADD | `bigint_add(a, b)` | `decimal_add(a, b)` |
| SUB | `bigint_sub(a, b)` | `decimal_sub(a, b)` |
| MUL | `bigint_mul(a, b)` | `decimal_mul(a, b)` |
| DIV | `bigint_div(a, b)` | `decimal_div(a, b, target_scale)` |
| MOD | `bigint_mod(a, b)` | N/A |
| CMP | `a.compare(&b)` (method) | `decimal_cmp(a, b)` |
| SQRT | N/A | `decimal_sqrt(a)` |
| SHL | `bigint_shl(a, shift)` | N/A |
| SHR | `bigint_shr(a, shift)` | N/A |

---

### 6. Gas Model

Gas costs are defined in the determin crate per RFC-0110 and RFC-0111:

**BIGINT Gas (RFC-0110):**

| Operation | Formula | Example (64 limbs) |
|-----------|---------|-------------------|
| ADD/SUB | 10 + limbs | 74 |
| MUL | 50 + 2 × limbs_a × limbs_b | 8,242 |
| DIV/MOD | 50 + 3 × limbs_a × limbs_b | 12,362 |
| CMP | 5 + limbs | 69 |
| SHL/SHR | 10 + limbs | 74 |

**DECIMAL Gas (RFC-0111):**

| Operation | Formula | Max (scales=36) |
|-----------|---------|------------------|
| ADD/SUB | 10 + 2 × \|scale_a - scale_b\| | 82 |
| MUL | 20 + 3 × scale_a × scale_b | 3,908 |
| DIV | 50 + 3 × scale_a × scale_b | 3,938 |
| SQRT | 100 + 5 × scale | 280 |

**Per-block budget:** 50,000 gas (matches RFC-0110/RFC-0111)

---

### 7. Type Gap Analysis

#### Current Stoolap State

| Type | SQL Keyword | Internal | Status |
|------|-------------|----------|--------|
| INTEGER | INTEGER, INT, SMALLINT, TINYINT | i64 | Implemented |
| FLOAT | FLOAT, DOUBLE, REAL | IEEE-754 f64 | Implemented |
| DFP | DFP, DETERMINISTICFLOAT | 113-bit | Implemented |
| DQA | DQA | i64 + scale | Implemented |
| BIGINT | BIGINT | BigInt | **Missing** |
| DECIMAL | DECIMAL, NUMERIC | Decimal | **Missing** |

#### Target State (After Implementation)

| Type | SQL Keyword | Internal | Status |
|------|-------------|----------|--------|
| INTEGER | INTEGER, INT, SMALLINT, TINYINT | i64 | Implemented |
| BIGINT | BIGINT | BigInt (≤4096 bits) | Implemented |
| FLOAT | FLOAT, DOUBLE, REAL | IEEE-754 f64 | Implemented |
| DFP | DFP, DETERMINISTICFLOAT | 113-bit | Implemented |
| DQA | DQA | i64 + scale (0-18) | Implemented |
| DECIMAL | DECIMAL, NUMERIC | i128 + scale (0-36) | Implemented |

---

## Implementation Phases

### Phase 1: Conversion RFCs (0131-0135)

**Objective:** Create conversion specifications (see separate RFCs).

- [ ] RFC-0131: BIGINT→DQA Conversion
- [ ] RFC-0132: DQA→BIGINT Conversion
- [ ] RFC-0133: BIGINT→DECIMAL Conversion
- [ ] RFC-0134: DECIMAL→BIGINT Conversion
- [ ] RFC-0135: DECIMAL↔DQA Review

### Phase 2: determin Crate Implementation

**Objective:** Implement conversion functions per RFC-0131, RFC-0132, RFC-0133, RFC-0134.

- [ ] Implement `bigint_to_dqa` per RFC-0131
- [ ] Implement `dqa_to_bigint` per RFC-0132
- [ ] Implement `bigint_to_decimal_full` per RFC-0133
- [ ] Implement `decimal_to_bigint_full` per RFC-0134
- [ ] Verify all conversions pass RFC test vectors

### Phase 3: Stoolap Core Types

**Objective:** Add BIGINT and DECIMAL to Stoolap's type system.

- [ ] Add `DataType::Bigint = 10` and `DataType::Decimal = 11` to `src/core/types.rs`
- [ ] Update `FromStr` to parse `BIGINT` and `DECIMAL`/`NUMERIC` keywords
- [ ] Update `Display` to render `BIGINT` and `DECIMAL`
- [ ] Add `Value::bigint()` and `Value::decimal()` constructors
- [ ] Add `Value::as_bigint()` and `Value::as_decimal()` extractors

### Phase 4: Expression VM Support

**Objective:** Execute BIGINT and DECIMAL operations in the query VM.

- [ ] Add BIGINT operation dispatch in `src/executor/expression/vm.rs`
- [ ] Add DECIMAL operation dispatch
- [ ] Wire up gas metering for new types
- [ ] Add cost estimates for optimizer

### Phase 5: Integration Testing

**Objective:** Verify end-to-end functionality.

- [ ] Integration tests with RFC-0110 test vectors
- [ ] Integration tests with RFC-0111 test vectors
- [ ] SQL parser tests for BIGINT and DECIMAL keywords
- [ ] Cast expression tests

---

## Security Considerations

### Overflow Handling

- BIGINT operations that exceed 4096 bits MUST return error
- DECIMAL operations that exceed ±(10^36 - 1) MUST return error
- Division by zero MUST return error
- All error handling uses determin crate error types

### Canonicalization Enforcement

- The determin crate enforces canonical form after every operation
- Stoolap's Value constructors use the determin crate's serialization
- Non-canonical inputs are rejected at deserialization

### Determinism Requirements

All operations MUST be deterministic per RFC-0110 and RFC-0111:

1. Algorithm locked (no implementation variance)
2. No Karatsuba for BIGINT multiplication
3. No SIMD or hardware carry flags
4. Fixed iteration bounds for division
5. 128-bit intermediate arithmetic for limb operations
6. Post-operation canonicalization

---

## Test Vectors

### Reference Test Vectors

BIGINT test vectors are defined in RFC-0110 §Test Vectors (56 entries with Merkle root).

DECIMAL test vectors are defined in RFC-0111 §Test Vectors (57 entries with Merkle root).

### Additional Integration Tests

| Test | SQL | Expected |
|------|-----|----------|
| BIGINT literal | `SELECT BIGINT '12345678901234567890'` | BigInt with correct limbs |
| DECIMAL literal | `SELECT DECIMAL '123.45'` | Decimal { mantissa: 12345, scale: 2 } |
| BIGINT add | `SELECT BIGINT '1' + BIGINT '2'` | BigInt '3' |
| DECIMAL add | `SELECT DECIMAL '1.5' + DECIMAL '2.5'` | Decimal '4.0' |
| BIGINT overflow | `SELECT BIGINT '2' ^ 4096` | Error: overflow |
| DECIMAL scale overflow | `SELECT DECIMAL '1' / DECIMAL '3'` | Canonical result with scale 6 |

---

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| Re-implement RFC-0110/RFC-0111 in Stoolap | Full control | Duplication, consensus risk |
| Use external bigint/decimal crates | Faster implementation | Not deterministic, dependency risk |
| **Use determin crate** | RFC-compliant, consensus-safe | Requires conversion functions |

**Decision:** Use determin crate for core algorithms, add Stoolap-specific integration. This is the only approach that guarantees consensus compatibility.

---

## Key Files to Modify

### determin crate

Conversion implementations are specified in separate RFCs:
- RFC-0131: BIGINT→DQA (`bigint_to_dqa`)
- RFC-0132: DQA→BIGINT (`dqa_to_bigint`)
- RFC-0133: BIGINT→DECIMAL (`bigint_to_decimal_full`)
- RFC-0134: DECIMAL→BIGINT (`decimal_to_bigint_full`)

### Stoolap

| File | Change |
|------|--------|
| `src/core/types.rs` | Add `DataType::Bigint`, `DataType::Decimal` |
| `src/core/value.rs` | Add `Value::bigint()`, `Value::decimal()`, extractors |
| `src/executor/expression/vm.rs` | Add BIGINT/DECIMAL operation dispatch |
| `src/executor/expression/ops.rs` | Add BIGINT/DECIMAL operators |

---

## Future Work

- F1: RFC-0124 DFP→DQA→BIGINT lowering integration
- F2: DECIMAL aggregate functions (SUM, AVG with exact arithmetic)
- F3: Vectorized BIGINT/DECIMAL operations for analytical queries
- F4: ZK circuit commitments for BIGINT/DECIMAL (per RFC-0110/RFC-0111)

---

## Rationale

### Why not re-implement RFC-0110/RFC-0111?

Re-implementing the algorithms would introduce consensus risk. Two implementations of the same algorithm may produce different results due to:
- Different iteration orders
- Different overflow handling
- Different rounding behavior

The determin crate is the reference implementation. Using it ensures Stoolap produces identical results to other compliant implementations.

### Why not use external crates like `bigdecimal`?

External crates:
- May change between versions
- May not be deterministic
- Introduce supply chain risk

The determin crate's RFC-0110/RFC-0111 implementations are:
- Algorithm-locked (no implementation variance)
- Consensus-verified (Merkle root commitments)
- Version-pinned (numeric_spec_version)

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-23 | Initial draft |
| 1.1 | 2026-03-23 | Fixed critical issues: wire format references to RFC-0110/RFC-0111, removed duplicate algorithm specs, clarified determin crate role |
| 1.2 | 2026-03-23 | Separated conversion specs into RFC-0131-0135, updated dependencies, revised conversion matrix to reference separate RFCs, restructured implementation phases |

---

## Related RFCs

- RFC-0104 (Numeric/Math): Deterministic Floating-Point (DFP)
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA)
- RFC-0110 (Numeric/Math): Deterministic BIGINT
- RFC-0111 (Numeric/Math): Deterministic DECIMAL
- RFC-0124 (Numeric/Math): Deterministic Numeric Lowering (optional)
- RFC-0131 (Numeric/Math): BIGINT→DQA Conversion
- RFC-0132 (Numeric/Math): DQA→BIGINT Conversion
- RFC-0133 (Numeric/Math): BIGINT→DECIMAL Conversion
- RFC-0134 (Numeric/Math): DECIMAL→BIGINT Conversion
- RFC-0135 (Numeric/Math): DECIMAL↔DQA Conversion Review

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
