# Plan: RFC-0201 Blob Implementation Missions

## Context

RFC-0201 (Binary BLOB Type for Deterministic Hash Storage) has been moved to Accepted status in the CipherOcto repository. The spec defines native BYTEA/BLOB support for cryptographic hash storage (SHA256, HMAC-SHA256). Implementation must happen in the **stoolap** codebase (external dependency at `github.com:CipherOcto/stoolap`, branch `feat/blockchain-sql`).

Two separate missions are **UNBLOCKED** and can proceed immediately:
- **Mission A**: Phase 2a/2b/2c/2e — Core Blob (wire tag 12)
- **Mission B1**: Phase 2f — DFP Dispatcher Integration (wire tag 13)

Both missions depend only on `octo-determin` crate (already in stoolap) and RFC-0104 (Accepted). Neither depends on RFC-0130.

**BigInt (wire tag 14)** is covered by RFC-0130-A — see "RFC-0130-A and RFC-0130-B Dependency" section below.

---

## Mission A: RFC-0201 Phase 2a/2b/2c/2e — BYTEA Core Blob Type

### 1. DataType Enum (`src/core/types.rs`)

Add `Blob = 10` as the next free variant:

```rust
/// Binary large object for cryptographic hashes and binary data
Blob = 10,
```

Update `FromStr` to parse BYTEA/BINARY/VARBINARY:

```rust
"BYTEA" | "BLOB" | "BINARY" | "VARBINARY" => Ok(DataType::Blob),
```

Update `is_numeric` → no change (Blob is not numeric). Update `is_orderable` → `!matches!(..., DataType::Blob | DataType::Json | DataType::Vector)` — Blob IS orderable via byte comparison.

**Note**: `DataType::as_u8()` and `from_u8()` auto-handle new variants via `#[repr(u8)]`.

### 2. SchemaColumn Extension (`src/core/schema.rs`)

Add `blob_length: Option<u32>` to `SchemaColumn`:

```rust
/// Fixed length for BLOB columns (None = variable length)
pub blob_length: Option<u32>,
```

Initialize to `None` in all constructors. Add builder method:

```rust
pub fn with_blob_length(mut self, len: u32) -> Self {
    self.blob_length = Some(len);
    self
}
```

### 3. Value::Blob Variant (`src/core/value.rs`)

Add first-class Blob variant (NOT Extension):

```rust
/// Binary large object — stored as CompactArc<[u8]> for zero-copy sharing.
/// INVARIANT: The Arc is always heap-allocated; there is no inline/blob case.
Blob(CompactArc<[u8]>),
```

**Remove** the comment at line 68 mentioning "Blob" as a future Extension type.

### 4. Blob Constructors in Value

```rust
impl Value {
    /// Create a Blob from a byte slice (copies into CompactArc)
    pub fn blob(data: &[u8]) -> Self {
        Value::Blob(CompactArc::from(data))
    }

    /// Create a Blob from an owned Vec (no copy — takes ownership of Arc)
    pub fn blob_from_vec(data: Vec<u8>) -> Self {
        Value::Blob(CompactArc::from(data))
    }

    /// Create a Blob from a CompactArc (zero-copy)
    pub fn blob_from_arc(data: CompactArc<[u8]>) -> Self {
        Value::Blob(data)
    }

    /// Extract blob data as byte slice
    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Value::Blob(data) => Some(data),
            _ => None,
        }
    }

    /// Extract blob data as a slice and its length
    pub fn as_blob_len(&self) -> Option<(&[u8], usize)> {
        match self {
            Value::Blob(data) => Some((data, data.len())),
            _ => None,
        }
    }

    /// Extract blob data as a 32-byte array (for SHA256 key_hash columns)
    /// Returns None if the blob is not exactly 32 bytes.
    pub fn as_blob_32(&self) -> Option<[u8; 32]> {
        match self {
            Value::Blob(data) if data.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(data);
                Some(arr)
            }
            _ => None,
        }
    }
}
```

### 4b. ToParam Implementations (`src/api/params.rs`)

Per RFC-0201 §ToParam Implementations — enables `$1` parameter binding for binary data:

```rust
impl ToParam for Vec<u8> {
    fn to_param(&self) -> Value {
        Value::Blob(Blob::from_slice(self))
    }
}

impl<const N: usize> ToParam for [u8; N] {
    fn to_param(&self) -> Value {
        Value::Blob(Blob::from_slice(self))
    }
}

impl ToParam for &[u8] {
    fn to_param(&self) -> Value {
        Value::Blob(Blob::from_slice(self))
    }
}
```

### 5. compare_blob and BlobOrdering (`src/core/value.rs`)

Per RFC-0201 Section on Comparison Semantics:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlobOrdering {
    Less,
    Equal,
    Greater,
}

/// Compare two blobs byte-by-byte in deterministic order
///
/// Algorithm:
/// 1. Compare bytes in ascending index order until difference found
/// 2. If all compared bytes are equal, compare lengths (shorter = less)
///
/// Determinism: This ordering is canonical and reproducible.
fn compare_blob(a: &[u8], b: &[u8]) -> BlobOrdering {
    let min_len = a.len().min(b.len());
    for i in 0..min_len {
        match a[i].cmp(&b[i]) {
            Ordering::Less => return BlobOrdering::Less,
            Ordering::Greater => return BlobOrdering::Greater,
            Ordering::Equal => continue,
        }
    }
    match a.len().cmp(&b.len()) {
        Ordering::Less => BlobOrdering::Less,
        Ordering::Greater => BlobOrdering::Greater,
        Ordering::Equal => BlobOrdering::Equal,
    }
}
```

**Important**: `BlobOrdering` is NOT `Ordering` — the RFC intentionally uses a separate type. The `Ord` impl on `BlobOrdering` is for use in BTree contexts, but `compare_blob` returns `BlobOrdering`.

### 6. Value::compare Integration (`src/core/value.rs`)

Also add `Value::Blob(_) => 10` to the `type_discriminant` function in the `Ord` impl (after the Extension case):
```rust
fn type_discriminant(v: &Value) -> u8 {
    match v {
        Value::Null(_) => 0,
        Value::Boolean(_) => 1,
        Value::Integer(_) | Value::Float(_) => 2,
        Value::Text(_) => 3,
        Value::Timestamp(_) => 4,
        Value::Extension(_) => 5,
        Value::Blob(_) => 10,  // NEW
    }
}
```

In `compare_same_type`, add:

```rust
(Value::Blob(a), Value::Blob(b)) => {
    Ok(match compare_blob(a, b) {
        BlobOrdering::Less => Ordering::Less,
        BlobOrdering::Equal => Ordering::Equal,
        BlobOrdering::Greater => Ordering::Greater,
    })
}
```

In `PartialEq` for Value:

```rust
(Value::Blob(a), Value::Blob(b)) => a == b,
```

In `Ord` for Value:

```rust
(Value::Blob(_), Value::Blob(_)) => Ordering::Equal,
// Blob uses discriminant ordering only (per RFC-0201: "derived Ord on Value uses
// the ordinal position of the Blob variant"). This matches Extension's behavior.
// For BTree: all Blobs sort together by type, not by content.
// For SQL ORDER BY on blobs: use compare_blob via Value::compare, not Ord.
```

In `Hash` for Value:

```rust
Value::Blob(data) => {
    // Include discriminant (10) and blob data in hash
    let mut hasher = state;
    hasher.write_u8(10);
    hasher.write(data);
}
```

### 7. Display and as_string for Blob

In `fmt::Display`:

```rust
Value::Blob(data) => {
    // Display as hex string (first 8 bytes + "..." if long)
    if data.len() <= 16 {
        write!(f, "{}", hex::encode(data))
    } else {
        write!(f, "{}...", hex::encode(&data[..16]))
    }
}
```

In `as_string`:

```rust
Value::Blob(data) => Some(hex::encode(data)),
```

In `as_str` → Blob does NOT implement `as_str` (binary data, not UTF-8).

### 8. Type Coercion for Blob

In `cast_to_type` → `DataType::Blob`: pass through if already Blob, error otherwise.

In `cast_to_type` FROM Blob → Text: hex encoding.

### 9. Serialization (`src/storage/mvcc/persistence.rs`)

**Tag 12** is the next free tag for Blob.

**Wire format** (per RFC-0201 §Serialization): `[u8: 12] [u32_be: length] [u8..len: data]`

**`serialize_blob()`** (standalone DCS function per RFC-0201 §Serialization):
```rust
fn serialize_blob(data: &[u8]) -> Result<Vec<u8>, DcsError> {
    if data.len() > 0xFFFFFFFF {
        return Err(DCS_BLOB_LENGTH_OVERFLOW);
    }
    Ok(serialize_bytes(data))  // u32_be(data.len()) || data
}
```

**`deserialize_blob()`** (standalone DCS function per RFC-0201):
```rust
fn deserialize_blob(input: &[u8]) -> Result<(&[u8], &[u8]), DcsError> {
    const LEN_SIZE: usize = 4;
    if input.len() < LEN_SIZE {
        return Err(DCS_INVALID_BLOB);
    }
    let length = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    if 4 + length > input.len() {
        return Err(DCS_INVALID_BLOB);
    }
    let blob_data = &input[4..4 + length];
    let remaining = &input[4 + length..];
    Ok((blob_data, remaining))
}
```

**`serialize_value` arm** for `Value::Blob`:
```rust
Value::Blob(data) => {
    buf.push(12);
    buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
    buf.extend_from_slice(data);
}
```

**`deserialize_value` arm** for tag 12:

```rust
12 => {
    // Blob — per RFC-0201: u32_be length prefix, DCS_INVALID_BLOB on truncation
    if rest.len() < 4 {
        return Err(Error::internal("missing blob length"));
    }
    let len = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
    if rest.len() < 4 + len {
        return Err(Error::internal("missing blob data"));
    }
    let blob_data = CompactArc::from(&rest[4..4 + len]);
    Ok(Value::Blob(blob_data))
}
```

**Audit requirement:** All `serialize_bytes` call sites in the codebase must be audited to ensure no Blob-typed data bypasses `serialize_blob`. `serialize_bytes` is a low-level primitive; `serialize_blob` is the required typed entry point.

### 10. DDL Parser (`src/executor/ddl.rs`)

Currently at line ~1131: `BLOB | "BINARY" | "VARBINARY" => Ok(DataType::Text)`. Change to:

```rust
"BYTEA" | "BLOB" | "BINARY" | "VARBINARY" => Ok(DataType::Blob),
```

Handle `BYTEA(N)` length constraint via regex in the DDL column parsing path, storing in `SchemaColumn.blob_length`.

### 11. Projection/Selection (Phase 2c)

`Value::Blob` must serialize correctly in result set encoding. The existing `Display` impl for `Value` handles this — Blob displays as hex.

### 12. Equality in Expression Evaluation (Phase 2b)

The `Value::compare` method already handles Blob via the new arm in `compare_same_type`. The expression VM calls `col_val.compare(val)` — no changes needed to the VM, only to Value's comparison logic.

### 13. Phase 2a: Hash Index for Blob Columns

The existing `HashIndex` uses ahash (not SipHash). Per RFC-0201:

- **Acceptable for Phase 2a**: ahash is fine for non-consensus use. SipHash with persistent key is the production requirement for the hash index, but ahash is acceptable for correctness verification first.
- **Implementation**: `HashIndex` already handles arbitrary `Value` types via `Value::hash`. The key insight is that `HashIndex` stores `Value::Blob` as a key — no structural changes needed. Only the hasher would differ (SipHash vs ahash), which is a Phase 2a follow-up.

Acceptance for Phase 2a: `CREATE INDEX ... USING HASH ON blob_column` creates a functional hash index that correctly resolves `WHERE blob_column = $1` lookups.

### 14. Null Handling

Per RFC-0201: `ALTER TABLE ADD COLUMN BYTEA ... NOT NULL` and `ALTER TABLE ADD COLUMN BYTEA ... NULL` are both **rejected** until null bitmap integration is complete. The schema validation layer must reject any `CREATE TABLE` or `ALTER TABLE` that introduces a BYTEA column with a clear error: "BYTEA columns not supported: null bitmap integration required".

### 15. Tests

Per RFC-0201 test vectors, implement:
- Blob round-trip: `Value::Blob(bytes)` → serialize → deserialize → `Value::Blob(same_bytes)`
- `compare_blob` deterministic ordering (bytes-first, length as tiebreaker)
- `BYTEST` in SQL parser
- `CREATE TABLE t(key_hash BYTEA(32) NOT NULL)` rejected
- Hash index creation and lookup for BYTEA column
- `validate_schema` rejects non-ascending and duplicate field_ids

### 16. Schema Validation — `validate_schema()`

Per RFC-0201 §Schema Validation for Dynamic Schemas — called at `CREATE TABLE` / schema registration time (not deserialization time):

```rust
pub fn validate_schema(schema: &[(u32, ColumnType)]) -> Result<(), DcsError> {
    // Check strictly ascending field_ids (uniqueness implied by < ordering)
    if !schema.windows(2).all(|w| w[0].0 < w[1].0) {
        return Err(DCS_INVALID_STRUCT);
    }
    for (_, col_type) in schema {
        validate_col_type(col_type)?;
    }
    Ok(())
}

fn validate_col_type(col_type: &ColumnType) -> Result<(), DcsError> {
    match col_type {
        ColumnType::Struct(inner) => validate_schema(inner)?,
        ColumnType::Enum(variants) => {
            let mut seen_ids: HashSet<u32> = HashSet::new();
            for (variant_id, variant_type) in variants {
                if !seen_ids.insert(variant_id) {
                    return Err(DCS_INVALID_STRUCT); // duplicate variant_id
                }
                validate_col_type(variant_type)?;
            }
        }
        ColumnType::Dvec(elem_type) => validate_col_type(elem_type)?,
        ColumnType::Dmat { rows, cols, elem_type } => {
            if *rows == 0 || *cols == 0 {
                return Err(DCS_INVALID_STRUCT);
            }
            if (*rows as u64) * (*cols as u64) > MAX_CONTAINER_ELEMENTS as u64 {
                return Err(DCS_INVALID_STRUCT);
            }
            validate_col_type(elem_type)?;
        }
        // Primitive types (Text, Bytea, Bool, I128, Dqa, Dfp) — valid as-is
        _ => {}
    }
    Ok(())
}
```

---

## Mission B1: RFC-0201 Phase 2f — DFP Dispatcher Integration

Phase 2f adds explicit DFP serialization/deserialization with wire tag 13 in the RFC-0201 dispatcher.

**Current state:** DFP is stored as `Value::Extension(CompactArc<[u8]>)` with `DataType::DeterministicFloat` tag byte. It serializes via the generic Extension path (tag 6).

**Goal:** Add explicit wire tag 13 for DFP per RFC-0104.

The `octo-determin::Dfp` and `DfpEncoding` types already exist in stoolap. Phase 2f is purely about wire protocol dispatch.

**Note:** BigInt (wire tag 14) is NOT covered by this mission — it is specified by RFC-0130 and depends on RFC-0130 being Accepted and Implemented first.

### Dispatcher Integration

1. **`serialize_value`** — add arm for DFP (wire tag 13):
   ```rust
   Value::Dfp(dfp) => {
       buf.push(13);  // wire tag 13 for DFP
       buf.extend_from_slice(&DfpEncoding::from_dfp(dfp).to_bytes());
   }
   ```

2. **`deserialize_value`** — add arm for tag 13 (24-byte DFP encoding).

**Note:** Phase 2f-A does NOT require a dedicated `Value::Dfp(Dfp)` variant — Extension storage is correct. The change is only in the wire protocol tag.

---

## RFC-0130-A and RFC-0130-B Dependency

BigInt infrastructure in stoolap is split into two RFCs:

**RFC-0130-A** (Stoolap BIGINT and DECIMAL Core Types, Draft):
- Core type infrastructure: `DataType::Bigint`, `DataType::Decimal`, `Value::bigint()`, `Value::decimal()`, SQL parsing, VM dispatch
- Depends ONLY on RFC-0110 and RFC-0111 (both Accepted) — **no conversion dependency**
- **Can be implemented immediately** while RFC-0130-B completes review

**RFC-0130-B** (BIGINT and DECIMAL Conversions, Draft):
- Conversion functions: BIGINT↔DQA, BIGINT↔DECIMAL, DECIMAL↔DQA
- Depends on RFC-0130-A (core types must exist first) AND RFC-0131-0135 (all Draft, with mutual dependencies)
- **Later phase** — conversions come after core types

**RFC-0201 Phase 2f BigInt note:** The BigInt wire tag 14 dispatcher is part of RFC-0130-A's scope. No separate RFC-0201 mission needed.

**Mission sequencing:**
1. Advance RFC-0130-A to Accepted → implement core types in stoolap
2. RFC-0131-0135 advance to Accepted
3. Advance RFC-0130-B to Accepted → implement conversion functions

---

## Dependencies

- **Mission A**: No external RFC dependencies. RFC-0127 (DCS Blob Amendment) is already Accepted and provides the wire format foundation.
- **Mission B1 (DFP)**: RFC-0104 (DFP wire format) is Accepted. `octo-determin::Dfp` already in stoolap. Independent of RFC-0130.
- **BigInt (Phase 2f)**: Covered by RFC-0130-A (core types). RFC-0130-B (conversions) is a later phase.

---

## Verification

After Mission A:
- `cargo test` passes with new Blob tests
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- `CREATE TABLE t(key_hash BYTEA(32))` parses without error
- `SELECT * FROM t WHERE key_hash = $1` uses hash index

After Mission B1 (DFP):
- DFP round-trip through serialize/deserialize with wire tag 13

After RFC-0130-A (BigInt core):
- BigInt available in stoolap via `DataType::Bigint` and `Value::BigInt`
- `NUMERIC_SPEC_VERSION = 2` after BigInt core implementation

After RFC-0130-B (conversions):
- CAST expressions work for BIGINT↔DQA, BIGINT↔DECIMAL, DECIMAL↔DQA
