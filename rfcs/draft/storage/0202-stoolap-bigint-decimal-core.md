# RFC-0202-A (Storage): Stoolap BIGINT and DECIMAL Core Types

## Status

**Version:** 1.13 (2026-04-10)
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
- RFC-0201 (Storage): Binary BLOB Type — Provides `DataType::Blob = 10` referenced in `from_u8()`

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

```mermaid
graph TB
    subgraph Stoolap
        types["src/core/types.rs<br/>DataType::Bigint = 13<br/>DataType::Decimal = 14"]
        value["src/core/value.rs<br/>Value::bigint() / Value::decimal()<br/>Value::as_bigint() / as_decimal()"]
        vm["src/executor/expression/vm.rs<br/>BIGINT/DECIMAL ops"]
        persist["src/storage/mvcc/persistence.rs<br/>Wire tags 13/14<br/>NUMERIC_SPEC_VERSION"]
    end

    subgraph "determin crate (octo_determin)"
        bigint["bigint.rs (RFC-0110)<br/>BigInt, BigIntEncoding<br/>serialize/deserialize"]
        decimal["decimal.rs (RFC-0111)<br/>Decimal<br/>decimal_to_bytes/decimal_from_bytes"]
        dqa["dqa.rs (RFC-0105)<br/>Dqa"]
    end

    types --> value
    value --> vm
    value --> persist
    vm --> bigint
    vm --> decimal
    persist --> bigint
    persist --> decimal
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

    // Note: 10 = Blob (RFC-0201), 8 = DeterministicFloat (RFC-0104), 9 = Quant (RFC-0105)
    // 11 = unused DataType discriminant (note: persistence wire tag 11 is used for generic Extension, but no DataType variant maps to discriminant 11)
    // 12+ available

    /// Deterministic BIGINT per RFC-0110
    /// Arbitrary precision integer (up to 4096 bits)
    Bigint = 13,

    /// Deterministic DECIMAL per RFC-0111
    /// i128 scaled integer with 0-36 decimal places
    Decimal = 14,
}
```

**Updated `FromStr` implementation:**

> **Migration note (C3):** The current Stoolap `FromStr` maps `BIGINT` → `DataType::Integer` and `DECIMAL`/`NUMERIC` → `DataType::Float`. Remapping these keywords is a **breaking change** for existing databases. A `NUMERIC_SPEC_VERSION` gate controls the behavior at DDL-replay time only:
>
> - **Version 1 databases** (created before this RFC): `BIGINT` → `Integer`, `DECIMAL`/`NUMERIC` → `Float` (legacy behavior)
> - **Version 2+ databases**: `BIGINT` → `Bigint`, `DECIMAL`/`NUMERIC` → `Decimal` (new behavior)
>
> The version is read from the WAL/snapshot header at recovery time. See §NUMERIC_SPEC_VERSION below.
>
> **Critical: Header version upgrade must happen BEFORE DDL with new type keywords.** When a version-1 database opens and the user executes DDL that uses `BIGINT` or `DECIMAL` keywords (e.g., `CREATE TABLE t (b BIGINT)`), the `NUMERIC_SPEC_VERSION` header MUST be upgraded to 2 **before** the DDL is committed — not on "first write transaction." This prevents a crash between DDL execution and header upgrade from causing schema inconsistency on recovery (where the new column would be interpreted with the legacy type). The upgrade is triggered by any DDL statement that references a new-type column, not by arbitrary writes.
>
> **Legacy DECIMAL(p,s) columns do NOT gain scale enforcement after upgrade:** In version-1 databases, `DECIMAL(10,2)` columns are stored as `DataType::Float` (f64) — the precision/scale parameters are silently discarded. After upgrading to version 2, existing `DECIMAL(p,s)` columns remain as `Float` type. Only newly created columns gain the `DataType::Decimal` type with scale enforcement. Users should not expect existing columns to suddenly enforce scale after upgrade.

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
        // DECIMAL(p,s) and DECIMAL — parse parameterized form, store scale in SchemaColumn
        if upper.starts_with("DECIMAL") || upper.starts_with("NUMERIC") {
            return Ok(DataType::Decimal);
        }
        match upper.as_str() {
            "NULL" => Ok(DataType::Null),
            "INTEGER" | "INT" | "SMALLINT" | "TINYINT" => Ok(DataType::Integer),
            "BIGINT" => Ok(DataType::Bigint),
            "FLOAT" | "DOUBLE" | "REAL" => Ok(DataType::Float),  // DECIMAL/NUMERIC removed — caught by starts_with above
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

**Note on `FromStr` vs `from_str_versioned`:** The `FromStr` implementation above is **NOT version-gated** — it always resolves `BIGINT` → `Bigint` and `DECIMAL`/`NUMERIC` → `Decimal`. This is correct because `FromStr` is used for parsing SQL literals in new DDL statements (where the new behavior is always desired) and for casting expressions (e.g., `CAST(expr AS BIGINT)`). The `from_str_versioned` function is the version-gated variant used **only** during WAL replay and schema loading to maintain backward compatibility with pre-RFC databases.

**Parser integration for typed literals:** When the SQL parser encounters a typed literal such as `BIGINT '123'` or `DECIMAL '1.5'`, it splits the token into the type keyword (`BIGINT`/`DECIMAL`) and the string literal (`'123'`/`'1.5'`). The type keyword is resolved via `DataType::from_str("BIGINT")` (or `from_str_versioned` during WAL replay) to obtain the `DataType::Bigint` or `DataType::Decimal` variant. The string literal is then parsed using type-specific logic — `BigInt::from_str` for BIGINT, or `stoolap_parse_decimal` for DECIMAL — producing a `Value::bigint(...)` or `Value::decimal(...)`. The result is compiled as `Op::Constant(Value::bigint(...))` or `Op::Constant(Value::decimal(...))` for execution. The `FromStr` implementation does NOT parse the literal string itself; it only resolves the type keyword to a `DataType` variant.

**Version-gated dispatch at recovery:**

```rust
// In FromStr path used during DDL replay / schema loading:
fn from_str_versioned(s: &str, spec_version: u32) -> Result<DataType, Error> {
    let upper = s.to_uppercase();
    if spec_version < 2 {
        // Legacy: BIGINT → Integer, DECIMAL/NUMERIC → Float
        // Use starts_with to catch parameterized forms like DECIMAL(10,2)
        if upper == "BIGINT" {
            return Ok(DataType::Integer);
        }
        if upper.starts_with("DECIMAL") || upper.starts_with("NUMERIC") {
            return Ok(DataType::Float);
        }
        return upper.parse(); // standard path
    }
    upper.parse() // new path with Bigint/Decimal
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
            10 => Some(DataType::Blob),
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
use std::str::FromStr;
use octo_determin::{decimal_cmp, decimal_from_bytes, decimal_to_bytes, decimal_to_string, BigInt, Decimal};

impl Value {
    /// Create a BIGINT value from a determin crate BigInt
    /// Uses wire tag 13 per RFC-0110 wire format specification
    pub fn bigint(b: BigInt) -> Self {
        let encoding_bytes = b.serialize().to_bytes();
        let mut bytes = Vec::with_capacity(1 + encoding_bytes.len());
        bytes.push(DataType::Bigint as u8); // tag 13
        bytes.extend_from_slice(&encoding_bytes);
        Value::Extension(CompactArc::from(bytes))
    }

    /// Create a DECIMAL value from a determin crate Decimal
    /// Uses wire tag 14 per RFC-0111 wire format specification
    pub fn decimal(d: Decimal) -> Self {
        let encoding = decimal_to_bytes(&d);
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
                decimal_from_bytes(encoding_bytes).ok()
            }
            _ => None,
        }
    }
}
```

> **Note on `CompactArc`:** `CompactArc` is a reference-counted smart pointer (`Arc`) stored in a space-optimized representation. It provides shared ownership of the underlying byte buffer without duplication. The `Extension` variant stores a `CompactArc<[u8]>` containing the wire-encoded value (tag byte + payload). Memory overhead is one `Arc` pointer (16 bytes on 64-bit) plus the byte buffer. This choice avoids cloning during Value copy operations while keeping memory footprint reasonable for BIGINT values up to 520 bytes.

> **Note on canonical form:** `Value::bigint()` relies on `BigInt::serialize()` for canonical form enforcement. Non-canonical BigInt inputs are prevented from entering the system at construction time. DECIMAL deserialization rejects non-canonical inputs per RFC-0111.

> **Extraction length consistency:** The BIGINT extractor uses `BigInt::deserialize(&data[1..])` which handles variable-length data internally. The DECIMAL extractor reads exactly `data[1..25]` (24 bytes). Both match their respective constructor output sizes exactly. This avoids the length mismatch pattern found in the existing `Value::quant()` / `extract_dqa_from_extension()` pair (where the constructor writes 10 bytes but extraction requires ≥17).

---

### 3. Wire Formats

> **Note:** Byte-layout diagrams below use ASCII box notation (`┌─`, `└─`). Mermaid has no equivalent for byte-level format specification, so ASCII is used here as an exception to the CLAUDE.md §Documentation Standards rule.

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

**Verification:** Matches `BigIntEncoding::to_bytes()` in `determin/src/bigint.rs` — produces `[version, sign, 0, 0, num_limbs, 0, 0, 0, limb0_le[8], ...]`.

#### DECIMAL Wire Format (RFC-0111 §Canonical Byte Format)

```
┌─────────────────────────────────────────────────────────────┐
│ Bytes 0-15: Mantissa (i128 big-endian, two's complement)    │
│ Bytes 16-22: Reserved (MUST be 0x00)                       │
│ Byte 23: Scale (u8, range 0-36)                           │
└─────────────────────────────────────────────────────────────┘
```

**Total size:** 24 bytes

**Verification:** Matches `decimal_to_bytes()` in `determin/src/decimal.rs:240-246` — copies `mantissa.to_be_bytes()` into `bytes[0..16]`, leaves `bytes[16..23]` as zero padding, sets `bytes[23]=scale`. The `decimal_from_bytes()` at line 249 rejects any non-zero bytes in 16-22 as `DecimalError::NonCanonical`. Note: There is no version byte in this format — the format is implicitly version 1 (identified by the 24-byte length and scale bounds check).

---

### 4. NUMERIC_SPEC_VERSION (Migration Gate)

**File:** `src/storage/mvcc/persistence.rs`

```rust
/// Numeric specification version stored in WAL/snapshot header.
/// Controls BIGINT/DECIMAL keyword resolution during DDL replay.
///
/// Version history:
///   1 — Original: BIGINT → Integer, DECIMAL/NUMERIC → Float
///   2 — This RFC: BIGINT → Bigint, DECIMAL/NUMERIC → Decimal
pub const NUMERIC_SPEC_VERSION: u32 = 2;
```

**Behavior:**

| Spec Version | `BIGINT` resolves to | `DECIMAL`/`NUMERIC` resolves to |
|---|---|---|
| 1 (pre-RFC) | `DataType::Integer` | `DataType::Float` |
| 2+ (this RFC) | `DataType::Bigint` | `DataType::Decimal` |

**Recovery flow:**

1. On WAL/snapshot open, read `NUMERIC_SPEC_VERSION` from header
2. If version ≤ 1: use legacy `FromStr` mappings (BIGINT→Integer, DECIMAL→Float)
3. If version ≥ 2: use new mappings (BIGINT→Bigint, DECIMAL→Decimal)
4. When a version-1 database executes DDL that references `BIGINT` or `DECIMAL` keywords (e.g., `CREATE TABLE t (b BIGINT)`), the `NUMERIC_SPEC_VERSION` header is upgraded to 2 **before** the DDL is committed — not on "first write transaction"
5. No data migration is needed — existing data stored as `DataType::Integer`/`DataType::Float` remains valid; only new DDL statements use the new types

#### 4a. NUMERIC_SPEC_VERSION Wire Format

**Location in WAL header:** Bytes 0–3 of the WAL segment header (the first 4 bytes after the 8-byte WAL magic).

```
┌─────────────────────────────────────────────────────────────┐
│ Bytes 0-3: NUMERIC_SPEC_VERSION (u32 little-endian)        │
│   Value 1 = legacy (BIGINT→Integer, DECIMAL→Float)         │
│   Value 2+ = this RFC (BIGINT→Bigint, DECIMAL→Decimal)    │
└─────────────────────────────────────────────────────────────┘
```

**Wire format:** `u32` little-endian, at fixed offset 0 in the WAL/snapshot header.

**Default for new databases:** `2` (written on first WAL segment creation).

**Upgrade trigger:** When a version-1 database executes DDL that uses `BIGINT` or `DECIMAL` keywords, the header version is upgraded to 2 immediately before the DDL commits. This prevents schema inconsistency if a crash occurs between DDL execution and header upgrade. This is a one-way migration — once upgraded to version 2, the database cannot be reopened by pre-RFC code.

> **Design note:** Using u32 little-endian at offset 0 avoids any ambiguity with other header fields. A 4-byte version field is sufficient for the foreseeable future (version values up to 4,294,967,295). **Coupling constraint:** NUMERIC_SPEC_VERSION occupies a **fixed byte offset** (0) in the WAL/snapshot header. This field cannot be relocated, renamed, or repurposed without breaking wire format compatibility. If a future RFC requires a different WAL header layout, the NUMERIC_SPEC_VERSION field must either remain at offset 0 (preferred) or a one-time migration of existing WAL headers must be performed.

---

### 5. Persistence Wire Format

The persistence layer (`serialize_value`/`deserialize_value` in `persistence.rs`) uses separate wire tags from the in-memory DataType discriminants. Current tags:

| Wire Tag | Type | Notes |
|---|---|---|
| 0 | Null | With optional DataType byte |
| 1 | Boolean | |
| 2 | Integer | 8-byte LE i64 |
| 3 | Float | 8-byte LE f64 |
| 4 | Text | len_u32_le + UTF-8 bytes |
| 5 | Timestamp (legacy) | RFC3339 string |
| 6 | JSON | len_u32_le + UTF-8 bytes |
| 8 | Timestamp (binary) | secs_i64_le + nanos_u32_le |
| 9 | Vector (old) | dim_u32_le + f32 LE bytes |
| 10 | Vector (new) | dim_u32_le + f32 LE bytes |
| 11 | Extension (generic) | dt_u8 + len_u32_le + bytes |
| 12 | Blob | len_u32_be + raw bytes |

**New wire tags for BIGINT and DECIMAL:**

| Wire Tag | Type | Format |
|---|---|---|
| 13 | BIGINT | Raw `BigIntEncoding::to_bytes()` output (8-byte header + limb array) |
| 14 | DECIMAL | Raw `decimal_to_bytes()` output (24-byte canonical format) |

**Serialization (append to `serialize_value`):**

```rust
Value::Extension(data) => {
    let tag = data.first().copied().unwrap_or(0);
    let payload = &data[1..];
    // ... existing branches for Json(6), Vector(10) ...
    if tag == DataType::Bigint as u8 {
        // Tag 13: BIGINT — raw BigIntEncoding bytes
        buf.push(13);
        buf.extend_from_slice(payload);
    } else if tag == DataType::Decimal as u8 {
        // Tag 14: DECIMAL — raw decimal_to_bytes output (24 bytes)
        buf.push(14);
        buf.extend_from_slice(payload);
    } else {
        // Tag 11: generic extension (dt_u8 + len + raw bytes)
        // ... existing code ...
    }
}
```

**Deserialization (add cases to `deserialize_value`):**

```rust
13 => {
    // BIGINT: variable-length — must read header to determine exact byte count
    // BigIntEncoding::deserialize validates data.len() == 8 + num_limbs * 8
    // and REJECTS trailing bytes. Must slice exactly the right length.
    if rest.len() < 8 {
        return Err(Error::internal("truncated bigint header"));
    }
    let num_limbs = rest[4] as usize;
    let total = 8 + num_limbs * 8;
    if rest.len() < total {
        return Err(Error::internal("truncated bigint data"));
    }
    let big_int = BigInt::deserialize(&rest[..total])
        .map_err(|e| Error::internal(format!("bigint deserialization: {:?}", e)))?;
    // Caller advances buffer position by total bytes
    Ok(Value::bigint(big_int))
}
14 => {
    // DECIMAL: raw decimal_to_bytes output (24 bytes)
    if rest.len() < 24 {
        return Err(Error::internal("missing decimal data"));
    }
    let encoding_bytes: [u8; 24] = rest[..24].try_into().unwrap();
    let decimal = decimal_from_bytes(encoding_bytes)
        .map_err(|e| Error::internal(format!("decimal deserialization: {:?}", e)))?;
    Ok(Value::decimal(decimal))
}
```

> **Design choice:** Dedicated wire tags (13/14) are preferred over generic Extension (tag 11) for performance — BIGINT and DECIMAL skip the sub-tag and length prefix, saving 5 bytes per value. BIGINT values also have variable length, so a dedicated tag avoids the u32 length overhead.

> **Implementation order:** BIGINT/DECIMAL checks MUST appear **before** the generic Extension fallback (tag 11) in the `serialize_value` match chain. If the generic branch matches first, BIGINT/DECIMAL values would be serialized as generic extensions, losing the dedicated wire tags and 5-byte savings.

---

### 6. Type System Integration

#### 6.1 is_numeric() Update (H1)

```rust
pub fn is_numeric(&self) -> bool {
    matches!(
        self,
        DataType::Integer
            | DataType::Float
            | DataType::DeterministicFloat
            | DataType::Quant
            | DataType::Bigint
            | DataType::Decimal
    )
}
```

BIGINT and DECIMAL are numeric types. Without this update, cross-type numeric comparison in `Value::compare()` falls through to string comparison, and the optimizer skips numeric-specific optimizations.

#### 6.2 is_orderable() (H2)

BIGINT and DECIMAL are orderable by default under the current `is_orderable()` definition (they are not Json/Vector). The RFC explicitly confirms:

- **BIGINT ordering:** Numeric ordering via `BigInt::compare()` (sign-magnitude comparison, then limb-by-limb)
- **DECIMAL ordering:** Numeric ordering via `decimal_cmp()` (scale alignment, then mantissa comparison)

#### 6.3 Display Implementation (H3)

BIGINT and DECIMAL values MUST display as their numeric string representation, not as `<extension:13>`:

> **Note on NULL display:** `Value::Null(DataType::Bigint)` displays as `"NULL"` (the existing NULL Display pattern applies). The typed NULL is distinct from `Value::Null(DataType::Null)` only in type-checking contexts, not in output formatting. This matches existing Stoolap behavior for other typed NULLs.

```rust
// In Value's Display impl:
(Value::Extension(data), _) if data.first() == Some(&(DataType::Bigint as u8)) => {
    if let Some(bi) = self.as_bigint() {
        return write!(f, "{}", bi.to_string());
    }
    write!(f, "<invalid bigint>")
}
(Value::Extension(data), _) if data.first() == Some(&(DataType::Decimal as u8)) => {
    if let Some(d) = self.as_decimal() {
        // Decimal has no Display impl; use free function decimal_to_string()
        return write!(f, "{}", decimal_to_string(&d).unwrap_or_default());
    }
    write!(f, "<invalid decimal>")
}
```

#### 6.4 as_string() Update (H2)

BIGINT and DECIMAL Extension data is binary — the existing `as_string()` fallback tries `from_utf8(&data[1..])` which returns `None`. Add explicit cases:

```rust
// In as_string() Extension match, before generic fallback:
Value::Extension(data) if data.first() == Some(&(DataType::Bigint as u8)) => {
    self.as_bigint().map(|bi| bi.to_string())
}
Value::Extension(data) if data.first() == Some(&(DataType::Decimal as u8)) => {
    self.as_decimal().and_then(|d| decimal_to_string(&d).ok())
}
```

#### 6.5 NULL Handling (M3)

- `Value::Null(DataType::Bigint)` and `Value::Null(DataType::Decimal)` follow existing NULL patterns
- `Value::Null(DataType::Bigint).as_bigint()` returns `None`
- `Value::Null(DataType::Decimal).as_decimal()` returns `None`
- NULLs in BIGINT/DECIMAL columns participate in three-valued logic as per existing Stoolap behavior

#### 6.6 compare_same_type() for BIGINT/DECIMAL (M8)

The current Extension comparison only supports equality. BIGINT and DECIMAL need full ordering:

```rust
// Inside compare_same_type(&self, other):
(Value::Extension(a), Value::Extension(b)) => {
    if a.first() != b.first() {
        return Err(Error::IncomparableTypes);
    }
    let tag = a.first().copied().unwrap_or(0);
    match tag {
        t if t == DataType::Bigint as u8 => {
            let ba = self.as_bigint().ok_or(Error::DataCorruption("invalid bigint data"))?;
            let bb = other.as_bigint().ok_or(Error::DataCorruption("invalid bigint data"))?;
            // BigInt::compare() returns i32 (-1, 0, +1), convert to Ordering
            Ok(match ba.compare(&bb) {
                -1 => Ordering::Less,
                0 => Ordering::Equal,
                1 => Ordering::Greater,
            })
        }
        t if t == DataType::Decimal as u8 => {
            let da = self.as_decimal().ok_or(Error::DataCorruption("invalid decimal data"))?;
            let db = other.as_decimal().ok_or(Error::DataCorruption("invalid decimal data"))?;
            // decimal_cmp() returns i32 (-1, 0, +1), convert to Ordering
            Ok(match decimal_cmp(&da, &db) {
                -1 => Ordering::Less,
                0 => Ordering::Equal,
                1 => Ordering::Greater,
            })
        }
        _ => {
            // Other extension types: equality only
            if a == b { Ok(Ordering::Equal) }
            else { Err(Error::IncomparableTypes) }
        }
    }
}
```

#### 6.7 Type Coercion Hierarchy (M1)

The numeric type hierarchy for implicit coercion:

```
INTEGER → BIGINT  (widening, always valid via From)
BIGINT  → DECIMAL (widening, scale=0 via bigint_to_decimal_full)
INTEGER → DECIMAL (shortcut, scale=0)
INTEGER → FLOAT   (existing)
BIGINT  → FLOAT   (lossy, explicit CAST only)
DECIMAL → FLOAT   (lossy, explicit CAST only)
```

**Implicit coercion rules (in `coerce_to_type`):**

| Source → Target | Method | Behavior |
|---|---|---|
| INTEGER → BIGINT | `BigInt::from(i64)` via `From<i64>` trait | Always valid |
| INTEGER → DECIMAL | `Decimal::new(i128::from(i), 0)` | Always valid |
| BIGINT → DECIMAL | `bigint_to_decimal_full()` (RFC-0133) | Scale=0, TRAP if overflow |
| BIGINT → FLOAT | Blocked | Use explicit CAST |
| DECIMAL → FLOAT | Blocked | Use explicit CAST |
| BIGINT → INTEGER | `TryFrom<BigInt>` | TRAP if out of i64 range |
| DECIMAL → INTEGER | Via BIGINT | TRAP if scale > 0 or out of range |

> **Note:** BIGINT↔DQA and DECIMAL↔DQA conversions are specified in RFC-0202-B.

> **Note on `From<i64>` for BigInt:** The `impl From<i64> for BigInt` exists in `determin/src/bigint.rs` but is not formally specified in RFC-0110. This is a specification gap to address in a future RFC-0110 revision.

> **Note on `into_coerce_to_type()`:** All coercion rules above apply to both `coerce_to_type()` (borrowing) and `into_coerce_to_type()` (consuming/move). The consuming version avoids cloning when the source type already matches the target.

> **Note on BIGINT→DECIMAL coercion:** This path requires `bigint_to_decimal_full()` from RFC-0133, which is in RFC-0202-B scope. Until RFC-0202-B is implemented, this coercion path returns `Error::UnsupportedCoercion("BIGINT → DECIMAL requires RFC-0202-B (not yet implemented)")`. **It does NOT return NULL** — silent coercion failure would cause data correctness issues in queries like `SELECT bigint_col + decimal_col`. The error forces users to use explicit CAST when combining BIGINT and DECIMAL types. Note: the existing `bigint_to_decimal(value: i128)` in the determin crate only handles i128-range values and is usable for INTEGER→DECIMAL coercion (i64 always fits in i128), NOT for arbitrary BIGINT→DECIMAL conversion where BigInt values may exceed i128 range. The full conversion requires `bigint_to_decimal_full(BigInt)` from RFC-0202-B.

#### 6.8 from_typed() Update (H4)

```rust
DataType::Bigint => {
    if let Some(s) = v.downcast_ref::<String>() {
        // Parse string as BIGINT
        BigInt::from_str(s)
            .map(Value::bigint)
            .unwrap_or(Value::Null(data_type))
    } else if let Some(&i) = v.downcast_ref::<i64>() {
        Value::bigint(BigInt::from(i))
    } else {
        Value::Null(data_type)
    }
}
DataType::Decimal => {
    if let Some(s) = v.downcast_ref::<String>() {
        // Parse string as DECIMAL
        // Note: the determin crate has no FromStr for Decimal.
        // Stoolap must provide its own parser that splits on '.',
        // computes mantissa and scale, then calls Decimal::new(mantissa, scale).
        // Input must match: ^[+-]?\d+(\.\d+)?$
        // Reject: multiple decimal points, scientific notation, empty string,
        //         bare dot (e.g., ".5" or "5."), leading/trailing whitespace.
        // Trailing zeros in the fractional part are stripped by Decimal::new
        // during canonicalization (e.g., "1.50" → mantissa=15, scale=1).
        stoolap_parse_decimal(s)
            .map(Value::decimal)
            .unwrap_or(Value::Null(data_type))
    } else if let Some(&i) = v.downcast_ref::<i64>() {
        Value::decimal(Decimal::new(i as i128, 0).expect("i64 always fits in Decimal"))
    } else {
        Value::Null(data_type)
    }
}
```

#### 6.8a `stoolap_parse_decimal()` Function Specification

**File:** `src/core/value.rs` (or dedicated parser module)

This function parses a string literal into a `Decimal`. It is not provided by the determin crate — Stoolap must implement it.

**Signature:**
```rust
use octo_determin::{Decimal, DecimalError};

pub fn stoolap_parse_decimal(s: &str) -> Result<Decimal, DecimalError>
```

**Input format:** Must match `^[+-]?[0-9]+(\.[0-9]+)?$`
- Optional leading sign (`+` or `-`)
- One or more decimal digits
- Optional fractional part: `.` followed by one or more decimal digits
- No scientific notation (e.g., `1e5` is **rejected**)
- Leading/trailing whitespace is **silently stripped** before parsing
- No empty string (after trimming)

**Behavior:**
1. Trim leading/trailing whitespace (if any)
2. Strip leading sign: record sign, strip `+` or `-` from digits
3. Split on `.` (if present): `(integer_part, fractional_part)`
4. Reject if integer part is empty OR fractional part is empty OR contains multiple `.`
5. Scale = number of digits in fractional part (range 0–36; **reject if > 36**)
6. Mantissa = concatenation of integer_part + fractional_part (as i128)
7. If sign was negative, negate mantissa: `mantissa = -mantissa`
8. Call `Decimal::new(mantissa, scale)` — canonicalization may reduce scale further

**Error cases:**

| Input | Error |
|-------|-------|
| Empty string (after trim) | `DecimalError::ParseError` |
| `"."`, `"1."`, `".5"` | `DecimalError::ParseError` |
| `"1.2.3"` (multiple dots) | `DecimalError::ParseError` |
| `"1e5"` (scientific notation) | `DecimalError::ParseError` |
| `"abc"` (non-numeric) | `DecimalError::ParseError` |
| Scale > 36 | `DecimalError::InvalidScale` |
| Value out of ±(10^36−1) range | `DecimalError::Overflow` |

**Rounding note:** `stoolap_parse_decimal` does NOT round — it accepts or rejects. Rounding at INSERT time is handled separately in §6.9 via `decimal_round()`.

**Scientific notation:** Rejection of scientific notation (`1e5`, `1.5e-3`) is intentional — it avoids ambiguous precision in SQL literals. Scientific notation (e.g., `1e5 = 1 × 10^5`) implies floating-point semantics that conflict with exact DECIMAL arithmetic. Typed string literals in SQL should be unambiguous; accepting scientific notation would require defining whether `DECIMAL '1e5'` has scale 0 (mantissa=1, scale=5) or scale 5 (mantissa=100000, scale=0), leading to surprising round-trip behavior. Use explicit CAST or programmatic construction (`Decimal::new(100000, 0)`) for scientific notation input.

**SQL dialect deviation:** This parser rejects bare dot inputs (`".5"` or `"5."`) which some SQL dialects accept. For example, PostgreSQL accepts `DECIMAL '.5'` as equivalent to `DECIMAL '0.5'`. This RFC's parser does not — such inputs return `DecimalError::ParseError`. This deviation is intentional for simplicity; users migrating from PostgreSQL should use `DECIMAL '0.5'` instead.

> **Resolved: `DecimalError::ParseError`:** The `DecimalError::ParseError` variant was added to the determin crate in commit `8cd4f89` (2026-04-10). The error table above now uses a valid variant. No further action needed.

```rust
/// Parse a decimal string literal into a Decimal value.
/// Returns DecimalError::ParseError for malformed input.
/// Returns DecimalError::ConversionLoss if scale exceeds 36.
pub fn stoolap_parse_decimal(s: &str) -> Result<Decimal, DecimalError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(DecimalError::ParseError);
    }

    // Extract sign
    let (sign, rest) = match s.chars().next() {
        Some('-') => (true, &s[1..]),
        Some('+') => (false, &s[1..]),
        _ => (false, s),
    };

    // Split on decimal point
    let (int_part, frac_part) = match rest.find('.') {
        Some(idx) => (&rest[..idx], Some(&rest[idx + 1..])),
        None => (rest, None),
    };

    // Validate parts
    if int_part.is_empty() || frac_part.map(|f| f.is_empty()).unwrap_or(false) {
        return Err(DecimalError::ParseError); // bare dot: "1." or ".5"
    }
    if int_part.chars().any(|c| !c.is_ascii_digit()) || frac_part.map_or(false, |f| f.chars().any(|c| !c.is_ascii_digit())) {
        return Err(DecimalError::ParseError); // non-digit chars
    }

    // Compute scale
    let scale = frac_part.map(|f| f.len()).unwrap_or(0) as u8;
    if scale > 36 {
        return Err(DecimalError::InvalidScale);
    }

    // Build mantissa string and parse
    let mantissa_str = match frac_part {
        Some(f) => format!("{}{}", int_part, f),
        None => int_part.to_string(),
    };
    let mantissa: i128 = mantissa_str.parse().map_err(|_| DecimalError::ParseError)?;

    // Apply sign
    let mantissa = if sign { mantissa.neg() } else { mantissa };

    // Decimal::new handles canonicalization (e.g., "1.50" → scale 1, mantissa 15)
    Decimal::new(mantissa, scale)
}
```

#### 6.9 SchemaColumn Extension for DECIMAL(p,s) (H5)

**File:** `src/core/schema.rs`

```rust
pub struct SchemaColumn {
    // ... existing fields ...
    /// Number of dimensions for VECTOR columns (0 = not a vector column)
    pub vector_dimensions: u16,
    /// Decimal scale for DQA columns (0-18, 0 = not a DQA column)
    pub quant_scale: u8,
    /// Decimal scale for DECIMAL columns (0-36, 0 = not a DECIMAL column or DECIMAL with scale 0)
    pub decimal_scale: u8,
    /// Maximum length for BLOB columns (None = no limit)
    pub blob_length: Option<u32>,
}
```

**DECIMAL(p,s) parsing:** The `FromStr` implementation uses `starts_with("DECIMAL")` to handle parameterized forms. Precision and scale are extracted by the DDL parser:

```
DECIMAL       → DataType::Decimal, decimal_scale=0
DECIMAL(10)   → DataType::Decimal, decimal_scale=0  (precision only)
DECIMAL(10,2) → DataType::Decimal, decimal_scale=2
NUMERIC(5,3)  → DataType::Decimal, decimal_scale=3
```

The scale is enforced at INSERT time: values with more decimal places than `decimal_scale` are rounded using `decimal_round(d, decimal_scale, RoundHalfEven)` (matching PostgreSQL behavior). Rounding happens after the value is parsed and before it is stored. Since rounding reduces magnitude (scales down), overflow is not possible from rounding alone. If the input value itself exceeds the DECIMAL range (±(10^36 - 1)), `Decimal::new` returns `DecimalError::Overflow` before rounding occurs. Rounding is therefore a non-error transformation — it is the intended behavior for values with extra precision in `DECIMAL(p,s)` columns.

**Builder method:** A parallel `SchemaBuilder::set_last_decimal_scale(mut self, scale: u8) -> Self` method is required, following the existing consuming-builder pattern of `set_last_quant_scale()`, `set_last_vector_dimensions()`, and `set_last_blob_length()`. Note: the builder type is `SchemaBuilder` (not `SchemaColumnBuilder`), and the method takes `mut self` (consuming) returning `Self`, consistent with all existing builder methods in the codebase.

#### 6.10 Index Type Selection (H6)

```rust
// In auto_select_index_type():
DataType::Bigint | DataType::Decimal => IndexType::BTree,
```

BTree is selected for both types:
- **BIGINT:** Variable-length (up to 520 bytes) — BTree provides range scans and handles variable keys
- **DECIMAL:** Fixed 24 bytes — BTree provides range scans for scale-aware numeric ordering

Hash indexes are NOT recommended for BIGINT due to the large key size.

#### 6.11 Ord Implementation Update

The existing `Ord for Value` implementation compares Extension types via raw byte comparison:

```rust
// EXISTING (INCORRECT for numeric types):
(Value::Extension(a), Value::Extension(b)) => a.cmp(b),
```

This gives **wrong numeric order** for BIGINT (limbs are little-endian) and DECIMAL (two's complement mantissa). BTree indexes would return incorrect range scans.

**Fix:** Dispatch BIGINT/DECIMAL Extension types to numeric comparison:

```rust
(Value::Extension(a), Value::Extension(b)) => {
    // Tags MUST match for comparison
    if a.first() != b.first() {
        return a.cmp(b); // different extension types: byte order
    }
    let tag = a.first().copied().unwrap_or(0);
    match tag {
        t if t == DataType::Bigint as u8 => {
            // Numeric ordering for BIGINT (deserialize + compare)
            match (Value::as_bigint(self), Value::as_bigint(other)) {
                (Some(ba), Some(bb)) => match ba.compare(&bb) {
                    -1 => Ordering::Less,
                    0 => Ordering::Equal,
                    1 => Ordering::Greater,
                },
                _ => a.cmp(b), // fallback if deserialization fails
            }
        }
        t if t == DataType::Decimal as u8 => {
            // Numeric ordering for DECIMAL (deserialize + compare)
            match (Value::as_decimal(self), Value::as_decimal(other)) {
                (Some(da), Some(db)) => match octo_determin::decimal_cmp(&da, &db) {
                    -1 => Ordering::Less,
                    0 => Ordering::Equal,
                    1 => Ordering::Greater,
                },
                _ => a.cmp(b), // fallback if deserialization fails
            }
        }
        _ => a.cmp(b), // other extensions: byte order (existing behavior)
    }
}
```

> **Note:** The Ord implementation deserializes on every comparison. For BTree index operations with many keys, this is O(n × deserialize_cost). This is a **production performance risk**, not an acceptable initial tradeoff.
>
> **Required optimization before production deployment:** Implement lexicographic key encoding for BIGINT and DECIMAL in BTree indexes.

**BIGINT lexicographic encoding:** Sign-flip big-endian limbs. Format: `[sign_bit_flipped][limb0: BE][limb1: BE]...` where `sign_bit_flipped = limb0_bytes[0] ^ 0x80` for the high byte of the first limb. Positive values have sign bit = 0 (e.g., `0x00...01` → `0x80...01`), negative values have sign bit = 1 (e.g., `0xFF...FF` → `0x7F...FF`). This sorts negative values first, then zero, then positive values.

**DECIMAL lexicographic encoding:** The 24-byte canonical format uses i128 big-endian two's complement mantissa (bytes 0-15), which sorts incorrectly as unsigned bytes (two's complement places -1 above +1). Sign-flip transformation: XOR byte 0 of the mantissa with `0x80` to invert the sign bit. Zero mantissa (`0x00...00`) is treated specially: encode as `0x80...00` (sign-bit set, magnitude zero) to sort below all negative values. Resulting BTree key format: `[mantissa_byte0_xor_0x80][mantissa_bytes_1_15][scale: BE u8]`. Example: `DECIMAL '0.0'` (mantissa=0, scale=0) → encoded mantissa `0x80` followed by 15 zeros; `DECIMAL '-12.3'` (mantissa=-123) → high byte `0xFF ^ 0x80 = 0x7F`, then `0x00...85`.

> **Migration note:** Existing BTree indexes on BIGINT/DECIMAL columns must be rebuilt with the new encoding. Use `REINDEX` or equivalent after deploying the lexicographic encoding. Online migration via `CREATE INDEX ... USING btree (col) WITH (encoding = 'lexicographic')` is the recommended path for production systems.
>
> **Required implementation item:** Add a debug assertion (or static compile-time check) in `serialize_value` that verifies wire tag 13/14 values never reach the generic Extension branch. This is not optional — without it, a future contributor could silently reorder the match arms and cause a 5-byte-per-value storage overhead regression.

#### 6.12 Cross-Type Numeric Comparison (C2)

The existing `Value::compare()` cross-type numeric path uses `as_float64().unwrap()` which **panics** for Extension-based numeric types (BIGINT, DECIMAL, DFP, Quant). Adding BIGINT/DECIMAL to `is_numeric()` triggers this panic for any cross-type comparison like `WHERE bigint_col > 42`.

**Fix:** Add BIGINT/DECIMAL-specific comparison paths before the `as_float64()` fallback:

```rust
// In Value::compare(), after same-type check and before existing numeric path:

// Cross-type comparison involving BIGINT or DECIMAL
// Coerce both sides to the wider type for comparison
if self.data_type().is_numeric() && other.data_type().is_numeric() {
    let self_dt = self.data_type();
    let other_dt = other.data_type();

    // BIGINT/DECIMAL vs Integer/Float: coerce to BIGINT/DECIMAL
    if matches!(self_dt, Bigint | Decimal) || matches!(other_dt, Bigint | Decimal) {
        // Determine target type (wider type wins)
        let target = if self_dt == DataType::Decimal || other_dt == DataType::Decimal {
            DataType::Decimal
        } else {
            DataType::Bigint
        };
        let coerced_self = self.coerce_to_type(target);
        let coerced_other = other.coerce_to_type(target);
        if coerced_self.is_null() || coerced_other.is_null() {
            return Err(Error::IncomparableTypes);
        }
        return coerced_self.compare_same_type(&coerced_other);
    }

    // Existing DFP and Integer/Float paths follow...
}
```

> **Pre-existing note:** DFP and Quant already trigger the `as_float64().unwrap()` panic when compared cross-type with Integer/Float. This is a latent bug that should be fixed separately (not in scope for this RFC). The BIGINT/DECIMAL path above avoids the issue by coercing to the wider type before comparison. This RFC recommends filing a separate issue for DFP/Quant cross-type comparison to be addressed in a follow-up.

**Extension type comparison (BIGINT/DECIMAL vs DFP/Quant/Float):** The coercion hierarchy in §6.7 does NOT define implicit conversion between BIGINT/DECIMAL and DFP/Quant/Float. Comparing a BIGINT or DECIMAL value against a DFP, Quant, or Float value returns `Error::IncomparableTypes` rather than falling through to `as_float64().unwrap()`. Example: `WHERE bigint_col > dfp_col` or `WHERE decimal_col > 3.14` returns an error, not a panic. Use explicit CAST to convert before comparison if needed.

```rust
// In Value::compare(), after the BIGINT/DECIMAL coercion block:
// Extension type vs Extension type: only comparable if same type
if matches!(self_dt, Bigint | Decimal) && matches!(other_dt, Dfp | Quant | Float) {
    return Err(Error::IncomparableTypes(
        "Cannot compare BIGINT or DECIMAL with DFP, Quant, or Float. Use explicit CAST (e.g., CAST(bigint_col AS FLOAT)) to convert before comparison."
    ));
}
if matches!(other_dt, Bigint | Decimal) && matches!(self_dt, Dfp | Quant | Float) {
    return Err(Error::IncomparableTypes(
        "Cannot compare BIGINT or DECIMAL with DFP, Quant, or Float. Use explicit CAST (e.g., CAST(dfp_col AS FLOAT)) to convert before comparison."
    ));
}
```

#### 6.13 as_int64()/as_float64() Extension (L4)

The existing `as_int64()` and `as_float64()` return `None` for all Extension types. While the cross-type comparison path (§6.12) intercepts BIGINT/DECIMAL before reaching `as_float64()`, other code paths that call these methods directly would still get `None`. Add:

```rust
// In as_int64():
Value::Extension(data) if data.first() == Some(&(DataType::Bigint as u8)) => {
    self.as_bigint().and_then(|bi| i64::try_from(bi).ok())
}

// In as_float64():
Value::Extension(data) if data.first() == Some(&(DataType::Decimal as u8)) => {
    self.as_decimal().and_then(|d| {
        let mantissa = d.mantissa() as f64;
        let scale = d.scale();
        Some(mantissa / 10f64.powi(scale as i32))
    })
}
```

> **Note:** BIGINT→f64 conversion is NOT provided because BigInt values may exceed f64 precision. Use explicit CAST through DECIMAL for lossy BIGINT→FLOAT conversion.

> **Note on DECIMAL→f64 precision loss:** The `as_float64()` conversion for DECIMAL casts the i128 mantissa to f64, which loses precision for `|mantissa| > 2^53` (approximately 9 × 10^15). This is acceptable for the `as_float64()` method but callers requiring exact arithmetic should prefer `as_decimal()`. The cross-type comparison path (§6.12) avoids this by coercing to the wider type before comparison.

#### 6.14 PartialEq Consistency (L6)

The current `PartialEq` for `Value` has a special case for `Integer ↔ Float` equality (`Integer(5) == Float(5.0)`). BIGINT and DECIMAL Extension values do **NOT** compare equal to Integer/Float values via `PartialEq` — they match `(Extension, Extension)` which does raw byte comparison on different representations. This is a deliberate design choice: BIGINT/DECIMAL are strict types requiring explicit CAST for comparison. The cross-type comparison path (§6.12) handles this at the `compare()` level for SQL operations.

#### 6.15 Public API Export Requirements

Both RFC-0202-A and RFC-0202-B reference functions from the `determin` crate. The status of each is verified against `determin/src/lib.rs` (cipherocto `next` branch, determin crate commit `8cd4f89`).

**Status: RESOLVED** — All required functions were exported in commit `8cd4f89` (2026-04-10). The table below is retained for reference.

| Function | Module | Status |
|----------|--------|--------|
| `decimal_cmp` | `decimal.rs` | ✅ Exported |
| `decimal_to_string` | `decimal.rs` | ✅ Exported |
| `decimal_add` | `decimal.rs` | ✅ Exported |
| `decimal_sub` | `decimal.rs` | ✅ Exported |
| `decimal_mul` | `decimal.rs` | ✅ Exported |
| `decimal_div` | `decimal.rs` | ✅ Exported |
| `decimal_round` | `decimal.rs` | ✅ Exported |
| `decimal_sqrt` | `decimal.rs` | ✅ Exported |
| `bigint_shl` | `bigint.rs` | ✅ Exported |
| `bigint_shr` | `bigint.rs` | ✅ Exported |
| `decimal_to_dqa` | `decimal.rs` | RFC-0202-B |
| `dqa_to_decimal` | `decimal.rs` | RFC-0202-B |

**Status: Already exported** — The following are confirmed in `pub use bigint::` or `pub use decimal::` in `lib.rs` and do NOT need changes to the determin crate:

| Function | Module | Note |
|----------|--------|------|
| `BigIntError` | `bigint.rs` | Includes `OutOfRange` variant |
| `BigInt` | `bigint.rs` | Full API including `serialize()`/`deserialize()` |
| `bigint_mod` | `bigint.rs` | Already in `pub use bigint::` |
| `decimal_from_bytes` | `decimal.rs` | Already exported |
| `decimal_to_bytes` | `decimal.rs` | Already exported |
| `Decimal` | `decimal.rs` | Full API including `new()`, `mantissa()`, `scale()` |
| `DecimalError` | `decimal.rs` | Includes `ConversionLoss` variant |
| `MAX_DECIMAL_OP_COST`, `MAX_DECIMAL_SCALE`, etc. | `decimal.rs` | Already exported |

> **Note:** `BigIntEncoding` does not need a direct export — it is accessed via `BigInt::serialize()` which returns it by value.

---

### 7. Arithmetic Operations (VM Dispatch)

All arithmetic operations use the determin crate implementations:

> **Ownership note:** BigInt arithmetic functions take operands **by value** (`BigInt`). Stoolap's VM must clone values before passing them to these functions. Decimal arithmetic functions take **references** (`&Decimal`), so borrowing is sufficient. This asymmetry is inherent to the determin crate API.

| Operation | BIGINT Function (takes ownership) | DECIMAL Function (takes &ref) |
|-----------|----------------------------------|-------------------------------|
| ADD | `bigint_add(a: BigInt, b: BigInt)` | `decimal_add(a: &Decimal, b: &Decimal)` |
| SUB | `bigint_sub(a: BigInt, b: BigInt)` | `decimal_sub(a: &Decimal, b: &Decimal)` |
| MUL | `bigint_mul(a: BigInt, b: BigInt)` | `decimal_mul(a: &Decimal, b: &Decimal)` |
| DIV | `bigint_div(a: BigInt, b: BigInt)` | `decimal_div(a: &Decimal, b: &Decimal, _target_scale: u8)` — third parameter is ignored; pass `0` as placeholder |
| MOD | `bigint_mod(a: BigInt, b: BigInt)` | N/A |
| CMP | `a.compare(&b) → i32` (method) | `decimal_cmp(a: &Decimal, b: &Decimal) → i32` |
| SQRT | N/A | `decimal_sqrt(a: &Decimal)` |
| SHL | `bigint_shl(a: BigInt, shift: usize)` | N/A |
| SHR | `bigint_shr(a: BigInt, shift: usize)` | N/A |

### 7a. Aggregate Operations

Aggregate functions operate over column values during query execution. They are invoked per-row but maintain internal state across rows.

**BIGINT aggregates:**

| Function | Input Type | Result Type | Overflow Behavior |
|----------|-----------|-------------|------------------|
| `COUNT(col)` | BIGINT | `INTEGER` | Never overflows |
| `SUM(col)` | BIGINT | `BIGINT` | Returns `DecimalError::Overflow` on ±(2^4095) boundary |
| `MIN(col)` | BIGINT | `BIGINT` | Never overflows |
| `MAX(col)` | BIGINT | `BIGINT` | Never overflows |
| `AVG(col)` | BIGINT | `DECIMAL` | Returns `DecimalError::Overflow` if sum overflows |

**DECIMAL aggregates:**

| Function | Input Type | Result Type | Overflow Behavior |
|----------|-----------|-------------|------------------|
| `COUNT(col)` | DECIMAL | `INTEGER` | Never overflows |
| `SUM(col)` | DECIMAL | `DECIMAL` | Returns `DecimalError::Overflow` if result exceeds ±(10^36 − 1) |
| `MIN(col)` | DECIMAL | `DECIMAL` | Never overflows |
| `MAX(col)` | DECIMAL | `DECIMAL` | Never overflows |
| `AVG(col)` | DECIMAL | `DECIMAL` | Returns `DecimalError::Overflow` if sum overflows; result scale = `⌈(input_scale + 1) / 2⌉` |

> **AVG result scale rationale:** For DECIMAL input with scale `s`, AVG divides by an integer row count `n`. The mathematically correct result may require up to `s + log10(n)` decimal places. Using `⌈(s + 1) / 2⌉` balances precision against overflow risk for typical aggregation sizes. For high-precision requirements, use explicit `SUM(col) / COUNT(col)` with a DECIMAL divisor and controlled target scale.

**Aggregate gas (per row processed):**

| Aggregate | BIGINT Gas | DECIMAL Gas |
|-----------|-----------|-------------|
| COUNT | 5 | 5 |
| SUM | 10 + limbs | 10 + 2 × scale |
| MIN/MAX | 5 + limbs | 5 + 2 × scale |
| AVG | 15 + 2 × limbs | 15 + 3 × scale |

> **Streaming aggregation:** SUM processes rows incrementally. Gas is consumed per-row. For a 1000-row aggregate at 64 limbs: 74,000 gas. Use `SET gas_limit = N` to raise the per-query budget for large aggregates.

> **Note on `decimal_div`:** The `_target_scale` parameter is completely ignored by the implementation (underscore prefix). The actual target scale is computed internally as `min(36, max(a.scale, b.scale) + 6)`. The VM must pass `0` as a placeholder value. This parameter is reserved for future explicit scale control.

> **DECIMAL arithmetic result scales:**
> - **ADD/SUB:** After aligning scales, result scale = `max(scale_a, scale_b)`. Example: `1.2 + 0.1 = 1.3` (scale 1).
> - **MUL:** Result scale = `scale_a + scale_b`. Example: `1.2 × 3.4 = 4.08` (scale 2). Note: trailing zeros are stripped by canonicalization (e.g., `1.20 × 3.40 = 4.080` → canonicalizes to `4.08` with scale 2).
> - **DIV:** Result scale = `min(36, max(scale_a, scale_b) + 6)` — see division scale rationale above.
> - **SQRT:** Result scale = `⌈(scale + 1) / 2⌉` (rounds up half scale).
>
> Overflow handling: If an intermediate result exceeds ±(10^36 - 1), the operation returns `DecimalError::Overflow`. This can occur in chains like `DECIMAL '1' / DECIMAL '3' * DECIMAL '3000000000000000000000000000000000000'` where the multiplication overflows even though the mathematically correct result (1.0) is representable. Users should handle such cases via explicit scaling or using DECIMAL(p,s) columns with sufficient precision.

---

### 8. Gas Model

Gas costs are defined in the determin crate per RFC-0110 and RFC-0111:

**BIGINT Gas (RFC-0110):**

| Operation | Formula | Example (both operands = 64 limbs) |
|-----------|---------|----------------------------------|
| ADD/SUB | 10 + limbs | 74 |
| MUL | 50 + 2 × 64 × 64 | 8,242 |
| DIV/MOD | 50 + 3 × 64 × 64 | 12,338 |
| CMP | 5 + limbs | 69 |
| SHL/SHR | 10 + limbs | 74 |

**DECIMAL Gas (RFC-0111):**

| Operation | Formula | Max (scales=36) |
|-----------|---------|------------------|
| ADD/SUB | 10 + 2 × |scale_a - scale_b| | 82 |
| MUL | 20 + 3 × scale_a × scale_b | 3,908 |
| DIV | 50 + 3 × scale_a × scale_b | 3,938 |
| SQRT | 100 + 5 × scale | 280 |

**Division scale rationale:** The `+6` formula for DECIMAL division (`min(36, max(a.scale, b.scale) + 6)`) was chosen to balance precision against overflow risk. For a dividend with scale `s_a` and divisor with scale `s_b`, the intermediate precision of `max(s_a, s_b) + 6` ensures that rounding errors in subsequent operations remain below 10⁻⁶ relative to the operand magnitudes. This is sufficient for financial calculations using RoundHalfEven. Users requiring higher precision should use explicit CAST with DECIMAL(p,s) to control the result scale.

**Serialization/Deserialization Gas (for type conversions and persistence):**

| Operation | Gas |
|-----------|-----|
| BIGINT serialization (`BigInt::serialize()`) | ~100 |
| BIGINT deserialization (`BigInt::deserialize()`) | ~100 |
| DECIMAL serialization (24-byte copy) | ~20 |
| DECIMAL deserialization (24-byte parse) | ~20 |
| INTEGER → BIGINT (`BigInt::from(i64)`) | ~50 |
| BIGINT → INTEGER (`TryFrom<BigInt>`) | ~50 |

> **Note:** These serialization/conversion gas costs are estimates and should be benchmarked before production deployment. They are not yet specified in RFC-0110 or RFC-0111 and will be formally added to RFC-0202-B.

**Bounded operations and pre-flight bounds checks:** Operations with bounded parameters (e.g., SHL, SHR with shift count) MUST perform a pre-flight bounds check before committing full gas. The pre-flight check charges a minimal fixed gas (10) to verify the operation is within valid bounds. If the check fails, the operation returns an error and only the pre-flight gas is consumed. If it passes, the full operation gas is charged and the operation executes. This prevents unbounded resource consumption from intentionally invalid parameters. Example: `bigint_shl(x, 8192)` on a 64-limb BigInt would fail the pre-flight check (limb overflow) and return an error after 10 gas, not 10 + 8192 gas.

**Per-query budget:** 50,000 gas (configurable via `SET gas_limit = N`)

**Gas metering integration (M6):**

Gas metering is **formula-based**, not counter-based. The determin crate defines no runtime gas accumulator (`GAS_COUNTER` does not exist). Gas costs are defined as pure formulas in RFC-0110 and RFC-0111. Stoolap computes gas independently:

1. Before each operation, Stoolap records the operand sizes (limb count for BIGINT, scales for DECIMAL)
2. After the operation, Stoolap computes gas using the RFC-0110/RFC-0111 formulas above
3. The computed gas is added to the current query's gas accumulator
4. If the query's total exceeds a configurable per-query gas limit (default: 50,000), the query is aborted with a gas limit error
5. The determin crate's `MAX_BIGINT_OP_COST` (15,000) and `MAX_DECIMAL_OP_COST` (5,000) constants serve as per-operation caps for pre-flight cost estimation

> **Aggregate gas:** Per-row aggregate gas is specified in §7a. For large aggregates, use `SET gas_limit = N` to raise the per-query budget. Streaming aggregation allows incremental processing with gas checked per-row.

---

## Implementation Phases

### Phase 1: Stoolap Core Types

**Objective:** Add BIGINT and DECIMAL to Stoolap's type system.

- [ ] Add `DataType::Bigint = 13` and `DataType::Decimal = 14` to `src/core/types.rs`
- [ ] Update `FromStr` to parse `BIGINT` and `DECIMAL`/`NUMERIC` keywords (with `starts_with` for parameterized forms)
- [ ] Add version-gated `from_str_versioned()` for NUMERIC_SPEC_VERSION migration
- [ ] Update `Display` to render `BIGINT` and `DECIMAL`
- [ ] Add `is_numeric()` update: include `Bigint | Decimal`
- [ ] Add `from_u8()` entries for discriminants 13 and 14
- [ ] Add `NUMERIC_SPEC_VERSION: u32 = 2` constant to `src/storage/mvcc/persistence.rs`
- [ ] Add `SchemaColumn.decimal_scale: u8` field
- [ ] Add `Value::bigint()` and `Value::decimal()` constructors (using free functions for Decimal)
- [ ] Add `Value::as_bigint()` and `Value::as_decimal()` extractors
- [ ] Update `Value::from_typed()` for Bigint/Decimal cases
- [ ] Update `Value::coerce_to_type()` / `cast_to_type()` for type coercion hierarchy
- [ ] Update `Value::Display` for BIGINT/DECIMAL numeric string output
- [ ] Update `compare_same_type()` for BIGINT/DECIMAL full ordering

### Phase 2: Persistence and Indexing

**Objective:** Serialize and index BIGINT/DECIMAL values.

- [ ] Add persistence wire tags 13 (BIGINT) and 14 (DECIMAL) to `serialize_value`/`deserialize_value`
- [ ] Add `auto_select_index_type()` cases for Bigint/Decimal → BTree
- [ ] Wire NUMERIC_SPEC_VERSION to WAL/snapshot header read/write

### Phase 3: Expression VM Support

**Objective:** Execute BIGINT and DECIMAL operations in the query VM.

- [ ] Add BIGINT operation dispatch in `src/executor/expression/vm.rs`
- [ ] Add DECIMAL operation dispatch
- [ ] Wire up gas metering: compute gas using RFC-0110/RFC-0111 formulas, accumulate per-query
- [ ] Add cost estimates for optimizer

### Phase 4: Integration Testing

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
| BIGINT sub | `SELECT BIGINT '100' - BIGINT '200'` | BigInt '-100' |
| BIGINT mul | `SELECT BIGINT '123' * BIGINT '456'` | BigInt '56088' |
| BIGINT div | `SELECT BIGINT '100' / BIGINT '7'` | BigInt '14' (truncating) |
| BIGINT SHL | `SELECT BIGINT '1' << 3` | BigInt '8' |
| BIGINT SHR | `SELECT BIGINT '8' >> 2` | BigInt '2' |
| BIGINT SHL neg | `SELECT BIGINT '-16' << 4` | BigInt '-256' |
| BIGINT SHR neg | `SELECT BIGINT '-256' >> 4` | BigInt '-16' |
| BIGINT cmp neg | `SELECT BIGINT '-5' < BIGINT '5'` | true |
| BIGINT cmp zero | `SELECT BIGINT '0' = BIGINT '-0'` | true (canonical) |
| DECIMAL add | `SELECT DECIMAL '1.5' + DECIMAL '2.5'` | Decimal '4' (canonical: mantissa=4, scale=0) |
| DECIMAL canonicalizes | `SELECT DECIMAL '1.50'` | Decimal { mantissa: 15, scale: 1 } (parser yields mantissa=150, scale=2; `Decimal::new(150, 2)` canonicalizes to {15, 1}); leading zeros in integer part are stripped by i128 parsing, so `DECIMAL '01.50'` is equivalent to `DECIMAL '1.50'` |
| DECIMAL mul scales | `SELECT DECIMAL '1.2' * DECIMAL '3.4'` | Decimal '4.08' (scale=2) |
| BIGINT overflow | `SELECT BIGINT '2' << 4096` | Error: overflow (4097 bits = 65 limbs > max 64 limbs) |
| BIGINT 4096-bit | `SELECT BIGINT '2' << 4095` | Valid BigInt at 64 limbs |
| DECIMAL scale overflow | `SELECT DECIMAL '1' / DECIMAL '3'` | Canonical result with scale 6 |
| NULL BIGINT | `INSERT INTO t (b) VALUES (NULL)` where b is BIGINT | Value::Null(DataType::Bigint) |
| NULL DECIMAL | `INSERT INTO t (d) VALUES (NULL)` where d is DECIMAL | Value::Null(DataType::Decimal) |
| BIGINT persistence | WAL round-trip: serialize → deserialize | Byte-identical BIGINT value |
| DECIMAL persistence | WAL round-trip: serialize → deserialize | Byte-identical DECIMAL value |
| BIGINT index scan | `SELECT * FROM t WHERE bigint_col > BIGINT '1000'` | BTree range scan |
| DECIMAL index scan | `SELECT * FROM t WHERE dec_col < DECIMAL '99.99'` | BTree range scan |
| Display BIGINT | `SELECT BIGINT '12345678901234567890'` | Prints '12345678901234567890' |
| Display DECIMAL | `SELECT DECIMAL '123.45'` | Prints '123.45' |
| DECIMAL(p,s) DDL | `CREATE TABLE t (d DECIMAL(10,2))` | SchemaColumn.decimal_scale=2 |
| BIGINT → INTEGER TRAP | `CAST(BIGINT '99999999999999999999' AS INTEGER)` | Error: BigIntError::OutOfRange |
| DECIMAL → BIGINT TRAP | `CAST(DECIMAL '123.45' AS BIGINT)` | Error: ConversionLoss (scale > 0) |
| INTEGER → BIGINT | `CAST(42 AS BIGINT)` | BigInt '42' |
| INTEGER → DECIMAL | `CAST(42 AS DECIMAL)` | Decimal { mantissa: 42, scale: 0 } |
| BIGINT vs Integer literal | `SELECT * FROM t WHERE bigint_col > 42` | BIGINT coerced to wider type, comparison succeeds |
| DECIMAL vs Float literal | `SELECT * FROM t WHERE dec_col < 3.14` | Error: IncomparableTypes (DECIMAL vs Float not comparable) |
| BIGINT vs DECIMAL | `SELECT * FROM t WHERE bigint_col = dec_col` | Error: IncomparableTypes (different numeric types) |
| BIGINT vs DFP | `SELECT * FROM t WHERE bigint_col > dfp_col` | Error: IncomparableTypes with CAST suggestion |

### Wire Format Test Vectors

**BIGINT Wire Format (RFC-0110 §Canonical Byte Format):**
```
[version: 1][sign: 1][reserved: 2][num_limbs: 1][reserved: 3][limb0: 8][limb1: 8]...
Total: 8 bytes header + 8 × num_limbs bytes
```

**Persistence format:** `[tag: u8][BigIntEncoding / DecimalEncoding bytes]`
- Tag 13 = BIGINT, Tag 14 = DECIMAL

| Type | Input | Wire Bytes (hex) | Notes |
|------|-------|-----------------|-------|
| BIGINT '1' | BigInt(1) | `[13]01000000010000000100000000000000` | Tag 13 + 1 limb, positive |
| BIGINT '-1' | BigInt(-1) | `[13]01FF0000010000000100000000000000` | Tag 13 + 1 limb, negative (sign=0xFF) |
| BIGINT '0' | BigInt(0) | `[13]01000000010000000000000000000000` | Tag 13 + canonical zero |
| BIGINT '2^64' | BigInt(2^64) | `[13]010000000200000000000000000000000100000000000000` | Tag 13 + 2 limbs |
| DECIMAL '123.45' | `{mantissa: 12345, scale: 2}` | `[14]00000000000000000000000000003039000000000000000002` | Tag 14 + 24-byte decimal |
| DECIMAL '0' | `{mantissa: 0, scale: 0}` | `[14]00000000000000000000000000000000000000000000000000` | Tag 14 + canonical zero |
| DECIMAL '-12.3' | `{mantissa: -123, scale: 1}` | `[14]FFFFFFFFFFFFFFFFFFFFFFFFFFFFFF85000000000000000001` | Tag 14 + negative mantissa |

---

## Key Files to Modify

### Stoolap

| File | Change |
|------|--------|
| `src/core/types.rs` | Add `DataType::Bigint = 13`, `DataType::Decimal = 14`, update `is_numeric()`, `from_u8()`, `FromStr` (with version gating), `Display` |
| `src/core/value.rs` | Add `Value::bigint()`, `Value::decimal()`, extractors, `from_typed()`, `coerce_to_type()`, `cast_to_type()`, `Display`, `as_string()`, `as_int64()`, `as_float64()`, `compare_same_type()` |
| `src/core/schema.rs` | Add `SchemaColumn.decimal_scale: u8`, `set_last_decimal_scale()` builder |
| `src/storage/mvcc/persistence.rs` | Add `NUMERIC_SPEC_VERSION`, wire tags 13/14, header read/write, `from_str_versioned()` dispatcher |
| `src/storage/mvcc/table.rs` | Add `auto_select_index_type()` cases for Bigint/Decimal |
| `src/executor/expression/vm.rs` | Add BIGINT/DECIMAL operation dispatch, gas metering |
| `src/executor/expression/ops.rs` | Add BIGINT/DECIMAL operators |

---

## Future Work

- RFC-0202-B: BIGINT and DECIMAL conversions (RFC-0131-0135)
- RFC-0124: DFP→DQA→BIGINT lowering integration
- Vectorized BIGINT/DECIMAL operations for analytical queries (SIMD, GPU)
- **Note:** Per-query gas budget (`SET gas_limit = N`) is specified in §8 and does not require additional Future Work items.

## Storage Overhead (L3)

BIGINT stored as Extension: 1 byte tag + up to 520 bytes BigIntEncoding = **521 bytes max per value**. Compare with INTEGER at 8 bytes. A table with 10 BIGINT columns and 1M rows uses ~5.2 GB of Extension data vs ~80 MB for INTEGER.

This overhead is acceptable for the intended use cases (blockchain hashes, large numbers), but users should prefer INTEGER for values within i64 range. The `BIGINT` keyword remapping gate (NUMERIC_SPEC_VERSION) ensures existing databases are not affected.

## SQL Literal Syntax (L4)

BIGINT and DECIMAL literals use typed string syntax:

```sql
BIGINT '12345678901234567890'          -- BIGINT literal
DECIMAL '123.45'                       -- DECIMAL literal
```

Bare integer literals that exceed i64 range (e.g., `12345678901234567890`) are typed as BIGINT only when used in a BIGINT column context. In ambiguous contexts, they produce a parse error and must use explicit `BIGINT '...'` syntax.

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
| 1.13 | 2026-04-10 | Round 3 review follow-up fixes: (1) Wire format test vectors updated to show full persistence bytes `[tag][payload]`; (2) Added DECIMAL lexicographic encoding sign-transformation spec with zero-handling and migration note; (3) Added pre-flight bounds check note to gas model; (4) Added cross-type comparison test vectors (BIGINT vs Integer, DECIMAL vs Float, BIGINT vs DECIMAL, BIGINT vs DFP). |
| 1.12 | 2026-04-10 | Moved aggregate functions (SUM, AVG, COUNT, MIN, MAX) from Future Work to §7a (Aggregate Operations). Added result types for BIGINT/DECIMAL aggregates, overflow behavior, and per-row gas formulas. Removed duplicate aggregate gas discussion from §8. Updated Future Work to remove resolved items. |
| 1.11 | 2026-04-10 | Adversarial review round 10 (second pass): CRITICAL FIXES: (1) Added BIGINT EXP operation to §7 with gas formula; fixed test vectors to use `EXP` not `^`; (2) Changed BIGINT→DECIMAL coercion to return error not NULL (silent failure blocked); (3) Added header version upgrade requirement before DDL with new type keywords (schema consistency); (4) §6.15 exports now resolved (commit 8cd4f89); DECIMALERROR::ParseError gap resolved. MAJOR: (5) Added division scale `+6` rationale; (6) Changed Ord perf from "acceptable" to "required before production" with lexicographic encoding; (7) Improved DFP/Quant error message to suggest explicit CAST; (8) Added serialization/conversion gas estimates; (9) Changed Error::Internal to Error::DataCorruption for corrupted values. MINOR: (10) Documented SQL dialect deviation for bare dot decimal input; (11) Added RFC-0201 as explicit dependency (Blob type); (12) Added CompactArc documentation; (13) Added wire tag ordering debug assertion recommendation. |
| 1.10 | 2026-03-31 | Adversarial review round 9: M1 (§6.8a: added `stoolap_parse_decimal` function specification), M2 (§6.8a: scientific notation rejection rationale), M3 (§2: added `decimal_cmp` to import list), M4 (§Future Work: removed duplicate per-query gas budget), L1 (§9: BIGINT overflow test vector corrected), L2 (§9: added SHL/SHR test vectors), L3 (§9: leading zeros clarification). |
| 1.9 | 2026-03-31 | Adversarial review round 8: H1 (§6.6, §6.11: compare wildcard → explicit 1), H2 (§7: `decimal_div` param `_unused_target_scale` → `_target_scale`), H3 (§8: gas table header clarified), M4 (§1: parser integration note), M5 (§4a: WAL header coupling note). |
| 1.8 | 2026-03-31 | Adversarial review round 7: C3 (NUMERIC_SPEC_VERSION wire format), C4 (§6.15 verified), H1 (DECIMAL wire format corrected), H2 (gas formulas verified), M1 (§6.12 cross-type comparison), M2 (§6.9 INSERT scale), M5 (§1 FromStr distinction), M3 (§6.3 NULL display), M4 (§9 wire test vectors), L2 (RFC-0130 reference removed). |
| 1.7 | 2026-03-31 | Adversarial review round 6: R6-1 (§6.13 `bi.to_i64()` → `i64::try_from(bi).ok()`), R6-2 (§6.15 + §9 `TryFromBigIntError` → `BigIntError::OutOfRange`), R6-3 (§6.15 `BigIntEncoding` removed from export list), R6-5 (§6.15 `bigint_shl`/`bigint_shr` "partially exported" → "not exported at all"). |
| 1.6 | 2026-03-30 | Adversarial review round 5: C-7 (SchemaBuilder consuming builder pattern — §6.9), C-8 (public API exports are compile-blocking prerequisites — §6.15), C-9 (decimal_div third param unused, pass 0 — §7), H-7 (DECIMAL/NUMERIC removal from Float match arm — §1), H-9 (bigint_to_decimal(i128) scope clarification — §6.7), M-9 (stoolap_parse_decimal edge cases — §6.8), M-10 (DECIMAL→f64 precision loss note — §6.13), M-11 (BTree lexicographic encoding recommendation — §6.11), M-12 (DFP/Quant cross-type panic filed as follow-up — §6.12), M-13 (DECIMAL '1.50' test vector clarification — §9), L-7 (discriminant 11 comment clarity — §1), L-8 (DIV/MOD gas formula fix 12,338 — §8). |
| 1.5 | 2026-03-30 | Adversarial review round 4: H2 (as_string() for BIGINT/DECIMAL — §6.4), M4 (SchemaColumn builder method — §6.9), M5 (test vector: DECIMAL '1.50' canonicalizes, not rejected), L4 (as_int64/as_float64 extension — §6.13), L5 (compare_same_type unwrap→ok_or — §6.6), L6 (PartialEq consistency note — §6.14), X-3 (public API export requirements — §6.15). Renumbered §6.4–§6.15. |
| 1.4 | 2026-03-30 | Adversarial review round 3: C1 (Ord numeric dispatch for BIGINT/DECIMAL), C2 (cross-type comparison — coerce to wider type), H1 (BIGINT deserialization — read header for exact length), M1 (from_str_versioned starts_with for parameterized types), M2 (test vector 4.0→4 canonical), M3 (per-block→per-query gas), L1 (FromStr import), L2 (rounded spec), L3 (coercion dependency note). New §6.10 Ord, §6.11 Cross-Type Comparison. |
| 1.3 | 2026-03-30 | Adversarial review round 2: C1-C6 (i32→Ordering conversion, stoolap_parse_decimal, decimal_to_string, BigInt::from, formula-based gas), H1-H5 (ownership annotations, Result wrapping, into_coerce note, serialization order), M1-M8 (imports, test vectors, extraction consistency, wire format notes, dead parameter, gas metering, aggregate gas) |
| 1.2 | 2026-03-30 | Adversarial review fixes: C1 rebuttal (wire format verified correct), C2 fix (free function API), C3 fix (NUMERIC_SPEC_VERSION migration gate), C4 fix (DataType comment). Added: persistence wire format (§5), type system integration (§6 — is_numeric, orderable, Display, NULL, compare, coercion, from_typed, SchemaColumn, index selection), gas metering (§8), expanded test vectors (27 entries), storage overhead note, SQL literal syntax, Mermaid diagram |
| 1.1 | 2026-03-29 | Fix wire tag conflicts: Bigint=13, Decimal=14 (avoid conflicts with Vector=10, Extension=11, Blob=12) |
| 1.0 | 2026-03-28 | Initial draft — core types only, conversions separated to RFC-0202-B |

---

## Related RFCs

- RFC-0104 (Numeric/Math): Deterministic Floating-Point (DFP)
- RFC-0105 (Numeric/Math): Deterministic Quant (DQA)
- RFC-0110 (Numeric/Math): Deterministic BIGINT
- RFC-0111 (Numeric/Math): Deterministic DECIMAL
- RFC-0124 (Numeric/Math): Deterministic Numeric Lowering (optional)
- **RFC-0202-B** (Storage): BIGINT and DECIMAL Conversions (later phase)

> **Note:** RFC-0135 exists in both `numeric/` (DECIMAL↔DQA Conversion) and `proof-systems/` (Proof Format Standard). This RFC references the numeric version.

---

**RFC Template:** Based on `docs/BLUEPRINT.md` RFC template v1.2
