# Plan: RFC-0201 Blob Implementation Missions

## Context

RFC-0201 (Binary BLOB Type for Deterministic Hash Storage) has been moved to Accepted status in the CipherOcto repository. The spec defines native BYTEA/BLOB support for cryptographic hash storage (SHA256, HMAC-SHA256). Implementation must happen in the **stoolap** codebase (external dependency at `github.com:CipherOcto/stoolap`, branch `feat/blockchain-sql`).

Two separate missions are needed:
- **Mission A**: Phase 2a/2b/2c/2e — Core Blob (parser, DataType, Value, serialization, comparison, projection)
- **Mission B**: Phase 2f — DFP/BigInt wire format integration

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
(Value::Blob(a), Value::Blob(b)) => a.cmp(b),
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

**Tag 12** is the next free tag for Blob:

```rust
Value::Blob(data) => {
    buf.push(12);
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    buf.extend_from_slice(data);
}
```

**Deserialization** for tag 12:

```rust
12 => {
    // Blob
    if rest.len() < 4 {
        return Err(Error::internal("missing blob length"));
    }
    let len = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
    if rest.len() < 4 + len {
        return Err(Error::internal("missing blob data"));
    }
    let blob_data = CompactArc::from(&rest[4..4 + len]);
    Ok(Value::Blob(blob_data))
}
```

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

---

## Mission B: RFC-0201 Phase 2f — DFP and BigInt Dispatcher Integration

Phase 2f implements `serialize_dfp`/`deserialize_dfp` and `serialize_bigint`/`deserialize_bigint` in the RFC-0201 dispatcher, replacing the `Err(DCS_INVALID_STRUCT)` stubs. Both RFC-0104 (DFP, 24-byte canonical format) and RFC-0110 (BigInt, little-endian limb array) are Accepted.

### Prerequisites

- `octo-determin` crate (already a dependency in stoolap — used for `Dfp`, `Dqa`)
- RFC-0104 and RFC-0110 wire format specs must be available

### DFP (RFC-0104)

The `octo-determin::Dfp` type already exists in stoolap (used via `Value::dfp()` etc.). The missing piece is the **dispatcher integration**:

In the RFC-0201 dispatcher pseudocode (implemented in stoolap's query/serialization layer):

```rust
(Value::Dfp(dfp_val), ColumnType::DeterministicFloat) => {
    let encoding = DfpEncoding::from_dfp(dfp_val).to_bytes();
    Ok(serialize_dfp(&encoding))
}
```

The wire format per RFC-0104 is **24 bytes**: sign(1) + exponent(2) + mantissa(21). `octo_determin::DfpEncoding` handles the conversion.

### BigInt (RFC-0110)

The `octo-determin::BigInt` type may not exist yet in stoolap's scope. Per RFC-0110, the wire format is:
- 4-byte little-endian limb count N
- N × 8-byte little-endian limbs, least-significant first

```rust
(Value::BigInt(bigint_val), ColumnType::BigInt) => {
    Ok(serialize_bigint(bigint_val))
}
```

### Dispatcher Integration Points

The "dispatcher" in RFC-0201 terminology maps to stoolap's query/serialization layer. Specifically:

1. **`serialize_value`** (in `src/storage/mvcc/persistence.rs`) — currently has no DFP or BigInt arm. Add:
   ```rust
   Value::Dfp(dfp) => { buf.push(13); buf.extend_from_slice(&DfpEncoding::from_dfp(dfp).to_bytes()); }
   Value::BigInt(bigint) => { /* limb serialization */ }
   ```

2. **`deserialize_value`** — currently returns `Err` for unknown tags. Add deserialization arms for tags 13 (DFP) and 14 (BigInt).

3. **`Value::from_typed`** and **`cast_to_type`** — add DFP and BigInt coercion paths.

### NUMERIC_SPEC_VERSION

Per RFC-0201 Phase 1 item and RFC-0110 governance, after implementing BigInt: bump `NUMERIC_SPEC_VERSION` to 2. This is a configuration constant in the serialization layer.

---

## Dependencies

- **Mission A**: No external RFC dependencies. RFC-0127 (DCS Blob Amendment) is already Accepted and provides the wire format foundation.
- **Mission B**: RFC-0104 (DFP wire format) and RFC-0110 (BigInt wire format) are both Accepted.

---

## Verification

After Mission A:
- `cargo test` passes with new Blob tests
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- `CREATE TABLE t(key_hash BYTEA(32))` parses without error
- `SELECT * FROM t WHERE key_hash = $1` uses hash index

After Mission B:
- DFP and BigInt round-trip through serialize/deserialize
- `NUMERIC_SPEC_VERSION = 2` after BigInt implementation
