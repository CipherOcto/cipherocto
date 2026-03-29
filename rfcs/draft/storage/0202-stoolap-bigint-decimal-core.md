# RFC-0202-A (Storage): Stoolap BIGINT and DECIMAL Core Types

## Status

**Version:** 1.1 (2026-03-29)
**Status:** Draft

## Authors

- Author: @agent

## Maintainers

- Maintainer: @ciphercito

## Summary

This RFC specifies the integration of BIGINT (RFC-0110) and DECIMAL (RFC-0111) **core types** into Stoolap — DataType variants, Value constructors/extractors, SQL keyword parsing, and Expression VM dispatch. Conversion functions between numeric types are covered by **RFC-0202-B** (Conversions), which is a separate RFC for later implementation.

This separation allows the core type infrastructure to proceed independently while the conversion RFCs (0131-0135) complete their adversarial review cycle.

## Dependencies

**Requires:**

- RFC-0104 (Numeric/Math): Deterministic Floating-Point (DFP) — Implemented in Stoolap
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA) — Implemented in Stoolap
- RFC-0110 (Numeric/Math): Deterministic BIGINT — **Accepted** (reference spec, algorithms in `determin` crate)
- RFC-0111 (Numeric/Math): Deterministic DECIMAL — **Accepted** (reference spec, algorithms in `determin` crate)

**Does NOT depend on:**
- RFC-0131, RFC-0132, RFC-0133, RFC-0134, RFC-0135 (conversions — separate RFC-0202-B)

**Optional:**

- RFC-0124 (Numeric/Math): Deterministic Numeric Lowering — DFP→DQA→BIGINT lowering (future work)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | BIGINT type in Stoolap | SQL keyword `BIGINT` parsed to `DataType::Bigint` |
| G2 | DECIMAL type in Stoolap | SQL keyword `DECIMAL`/`NUMERIC` parsed to `DataType::Decimal` |
| G3 | Canonical serialization | Wire format matches RFC-0110/RFC-0111 exactly |
| G4 | VM arithmetic dispatch | BIGINT/DECIMAL ops execute via determin crate |

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
│  │ (13, 14)   │  │  types)     │  │                        │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     determin crate                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │ bigint.rs   │  │ decimal.rs  │  │ dqa.rs                │  │
│  │ RFC-0110    │  │ RFC-0111     │  │ RFC-0105              │  │
│  │ algorithms  │  │ algorithms  │  │                       │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

**Key principle:** Core algorithms (RFC-0110/RFC-0111) live in `determin` crate. Stoolap adds SQL parsing, type system integration, and VM execution. Conversion functions are NOT in scope (RFC-0202-B).

---

## Specification

### 1. DataType Enum Extension (Stoolap)

**File:** `src/core/types.rs`

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

    // NEW variants (10-11 reserved for future use)
    // Note: 12 = Blob (RFC-0201), 13 = DFP (RFC-0104), 14-15 available
    /// Deterministic BIGINT per RFC-0110
    /// Arbitrary precision integer (up to 4096 bits)
    Bigint = 13,

    /// Deterministic DECIMAL per RFC-0111
    /// i128 scaled integer with 0-36 decimal places
    Decimal = 14,
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
            "BIGINT" => Ok(DataType::Bigint),
            "FLOAT" | "DOUBLE" | "REAL" => Ok(DataType::Float),
            "DECIMAL" | "NUMERIC" => Ok(DataType::Decimal),
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
            13 => Some(DataType::Bigint),
            14 => Some(DataType::Decimal),
            _ => None,
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::DeterministicFloat => write!(f, "DFP"),
            DataType::Quant => write!(f, "DQA"),
            DataType::Bigint => write!(f, "BIGINT"),
            DataType::Decimal => write!(f, "DECIMAL"),
            // ... existing matches ...
        }
    }
}
```

---

### 2. Value Type Extension (Stoolap)

**File:** `src/core/value.rs`

BIGINT and DECIMAL values are stored in the `Extension` variant using the determin crate's canonical serialization formats.

```rust
use octo_determin::{BigInt, Decimal, Dfp, DfpClass, DfpEncoding, Dqa};

impl Value {
    /// Create a BIGINT value from a determin crate BigInt
    /// Uses wire tag 13 per RFC-0110 wire format specification
    pub fn bigint(b: BigInt) -> Self {
        let encoding = b.serialize();
        let mut bytes = Vec::with_capacity(1 + encoding.len());
        bytes.push(DataType::Bigint as u8); // tag 13
        bytes.extend_from_slice(&encoding.to_bytes());
        Value::Extension(CompactArc::from(bytes))
    }

    /// Create a DECIMAL value from a determin crate Decimal
    /// Uses wire tag 14 per RFC-0111 wire format specification
    pub fn decimal(d: Decimal) -> Self {
        let encoding = d.to_bytes();
        let mut bytes = Vec::with_capacity(1 + 24);
        bytes.push(DataType::Decimal as u8); // tag 14
        bytes.extend_from_slice(&encoding);
        Value::Extension(CompactArc::from(bytes))
    }

    /// Extract BIGINT as determin crate BigInt
    pub fn as_bigint(&self) -> Option<BigInt> {
        match self {
            Value::Extension(data)
                if data.first().copied() == Some(DataType::Bigint as u8) => // tag 13
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
                if data.first().copied() == Some(DataType::Decimal as u8) => // tag 14
            {
                let encoding_bytes: [u8; 24] = data[1..25].try_into().ok()?;
                Decimal::from_bytes(encoding_bytes).ok()
            }
            _ => None,
        }
    }
}
```

> **Note on canonical form:** `Value::bigint()` relies on `BigInt::serialize()` for canonical form enforcement. Non-canonical BigInt inputs are prevented from entering the system at construction time. DECIMAL deserialization rejects non-canonical inputs per RFC-0111.

---

### 3. Wire Formats

#### BIGINT Wire Format (RFC-0110 §Canonical Byte Format)

> **Naming note:** The wire format is defined by RFC-0110's `BigIntEncoding` type. The DataType variant is `Bigint` (lowercase 'i'); the encoding type is `BigIntEncoding` (uppercase 'I'). These are independent names.

```
┌─────────────────────────────────────────────────────────────┐
│ Byte 0: Version (0x01)                                      │
│ Byte 1: Sign (0 = positive, 0xFF = negative)               │
│ Bytes 2-3: Reserved (0x0000)                                 │
│ Byte 4: Number of limbs (u8, range 1–64)                     │
│ Bytes 5-7: Reserved (MUST be 0x00)                           │
│ Byte 8+: Limb array (little-endian u64 × num_limbs)          │
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
│ Bytes 8-23: Mantissa (i128 big-endian, two's complement)   │
└─────────────────────────────────────────────────────────────┘
```

**Total size:** 24 bytes

---

### 4. Arithmetic Operations (VM Dispatch)

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

### 5. Gas Model

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
| ADD/SUB | 10 + 2 × |scale_a - scale_b| | 82 |
| MUL | 20 + 3 × scale_a × scale_b | 3,908 |
| DIV | 50 + 3 × scale_a × scale_b | 3,938 |
| SQRT | 100 + 5 × scale | 280 |

**Per-block budget:** 50,000 gas

> **Note on gas metering:** The 50,000 gas per-block budget is for the determin crate's internal metering. Stoolap's transaction gas tracking is independent and must wire up to the determin crate's gas counter.

---

## Implementation Phases

### Phase 1: Stoolap Core Types

**Objective:** Add BIGINT and DECIMAL to Stoolap's type system.

- [ ] Add `DataType::Bigint = 13` and `DataType::Decimal = 14` to `src/core/types.rs`
- [ ] Update `FromStr` to parse `BIGINT` and `DECIMAL`/`NUMERIC` keywords
- [ ] Update `Display` to render `BIGINT` and `DECIMAL`
- [ ] Add `Value::bigint()` and `Value::decimal()` constructors
- [ ] Add `Value::as_bigint()` and `Value::as_decimal()` extractors
- [ ] Add `NUMERIC_SPEC_VERSION: u32 = 2` constant to `src/storage/mvcc/persistence.rs` (value = 2 after BigInt implementation, per RFC-0110 governance)

### Phase 2: Expression VM Support

**Objective:** Execute BIGINT and DECIMAL operations in the query VM.

- [ ] Add BIGINT operation dispatch in `src/executor/expression/vm.rs`
- [ ] Add DECIMAL operation dispatch
- [ ] Wire up gas metering for new types
- [ ] Add cost estimates for optimizer

### Phase 3: Integration Testing

**Objective:** Verify end-to-end functionality.

- [ ] Integration tests with RFC-0110 test vectors
- [ ] Integration tests with RFC-0111 test vectors
- [ ] SQL parser tests for BIGINT and DECIMAL keywords

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

## Key Files to Modify

### Stoolap

| File | Change |
|------|--------|
| `src/core/types.rs` | Add `DataType::Bigint`, `DataType::Decimal` |
| `src/core/value.rs` | Add `Value::bigint()`, `Value::decimal()`, extractors |
| `src/executor/expression/vm.rs` | Add BIGINT/DECIMAL operation dispatch |
| `src/executor/expression/ops.rs` | Add BIGINT/DECIMAL operators |

---

## Future Work

- RFC-0202-B: BIGINT and DECIMAL conversions (RFC-0131-0135)
- RFC-0124: DFP→DQA→BIGINT lowering integration
- DECIMAL aggregate functions (SUM, AVG with exact arithmetic)
- Vectorized BIGINT/DECIMAL operations for analytical queries

---

## Rationale

### Why conversions are separate (RFC-0202-B)

Conversion functions (BIGINT↔DQA, BIGINT↔DECIMAL) depend on RFCs 0131-0135 which are still in Draft status with mutual dependencies. Splitting them out allows:

1. **Parallel progress**: Core types can be implemented while conversion RFCs complete review
2. **Smaller scope**: Each RFC is easier to review and implement
3. **Reduced risk**: Core type infrastructure doesn't block on conversion spec finalization

### Why not re-implement RFC-0110/RFC-0111?

Re-implementing the algorithms would introduce consensus risk. The determin crate is the reference implementation. Using it ensures Stoolap produces identical results to other compliant implementations.

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.1 | 2026-03-29 | Fix wire tag conflicts: Bigint=13, Decimal=14 (avoid conflicts with Vector=10, Extension=11, Blob=12) |
| 1.0 | 2026-03-28 | Initial draft — core types only, conversions separated to RFC-0202-B |

---

## Related RFCs

- RFC-0104 (Numeric/Math): Deterministic Floating-Point (DFP)
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA)
- RFC-0110 (Numeric/Math): Deterministic BIGINT
- RFC-0111 (Numeric/Math): Deterministic DECIMAL
- RFC-0124 (Numeric/Math): Deterministic Numeric Lowering (optional)
- **RFC-0202-B** (Numeric/Math): BIGINT and DECIMAL Conversions (later phase)

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
