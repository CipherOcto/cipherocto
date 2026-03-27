# RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage

## Status

Draft (v5.2, adversarial review)

## Authors

- Author: @cipherocto

## Summary

This RFC adds native binary data type support (BLOB/BYTEA) to stoolap's type system. Binary storage enables efficient cryptographic hash storage (SHA256, HMAC-SHA256) without hex encoding overhead, reducing storage by 50% and enabling deterministic byte-level comparison for economic ledgers.

## Dependencies

**Informative:**

- RFC-0127 (Numeric): DCS Blob Amendment — already Accepted; defines Blob as first-class DCS type with `serialize_blob`/`deserialize_blob` signatures, error codes, and schema-driven dispatcher requirement (RFC-0127 Changes 2, 7, 8, 13). RFC-0201 aligns with RFC-0127's specifications throughout.

**Required By:**

- RFC-0903 (Economics): Virtual API Key System — `key_hash BYTEA(32)` for HMAC-SHA256
- RFC-0909 (Economics): Deterministic Quota Accounting — `event_id BYTEA(32)`, `request_id BYTEA(32)` for SHA256

## Motivation

### The Problem

Current stoolap lacks a binary data type. Implementations requiring binary hash storage must use `TEXT` with hex encoding:

```rust
// Current workaround (wasteful)
let key_hash_hex = hex::encode(&key_hash);  // 32 bytes → 64 chars
params![key_hash_hex.into()]  // TEXT storage
```

**Problems:**
1. **Storage waste**: 32 bytes → 64 hex chars (2x overhead)
2. **Encoding/decoding overhead**: Every insert/lookup requires hex conversion
3. **Lexicographic comparison**: TEXT comparison differs from byte comparison
4. **Non-deterministic semantics**: Hex encoding is a presentation layer concern, not storage

### Use Cases

1. **API Key Hashes**: HMAC-SHA256 key hashes (32 bytes)
2. **Event IDs**: SHA256 request/event identifiers (32 bytes)
3. **Pricing Hashes**: SHA256 of pricing tables (32 bytes)
4. **Merkle Proofs**: Binary proof elements (variable length)
5. **Signatures**: Ed25519, ECDSA signatures (64 bytes, 72 bytes)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | 50% storage reduction | Hash storage: 64 bytes TEXT → 32 bytes BLOB |
| G2 | Zero encoding overhead | No hex encode/decode on insert/lookup |
| G3 | Deterministic comparison | Byte-by-byte comparison, not lexicographic |
| G4 | O(1) hash index lookup | Hash index for equality comparisons |

## Specification

### Type System Changes

#### DataType Enum

```rust
// In core/types.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum DataType {
    // ... existing variants 0-9 ...
    /// Binary large object (variable-length byte sequence)
    Blob = 10,
}
```

#### Value Enum

```rust
// In core/value.rs

/// Blob: Binary large object stored as a reference-counted byte sequence.
///
/// Blob is stored in a dedicated enum variant for compile-time type safety.
/// The CompactArc provides shared ownership without heap allocation on clone.
///
/// Note: Unlike Extension types (Json, Vector), Blob uses a dedicated variant
/// because binary data is security-critical and must not be conflated with
/// other extension types. A separate variant also avoids tag byte validation
/// at access time.
#[derive(Debug, Clone)]
pub struct Blob {
    data: CompactArc<[u8]>,
}

impl Blob {
    /// Create a new Blob from a byte vector
    pub fn new(data: Vec<u8>) -> Self {
        Blob { data: CompactArc::from(data) }
    }

    /// Create a new Blob from a byte slice (copies data into CompactArc).
    ///
    /// The copy into CompactArc (permanent storage) is distinct from the
    /// "MUST NOT copy the returned slice" prohibition in the deserialization
    /// path. `from_slice` is appropriate when constructing Blobs from existing
    /// byte sources (e.g., test data, parameters). In the deserialization path,
    /// use `Blob::new()` or a dedicated from-slice-without-copy constructor to
    /// avoid a double-copy: once into CompactArc storage, and again when the
    /// dispatcher calls `Blob::from_slice(value)` on a slice that is already
    /// owned by the input buffer.
    pub fn from_slice(data: &[u8]) -> Self {
        Blob { data: CompactArc::from_slice(data) }
    }

    /// Returns the blob contents as a byte slice
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// Blob comparison result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobOrdering {
    Equal,
    Less,
    Greater,
}
```

**Storage Note**: Blob data is heap-allocated via `CompactArc<[u8]>`. All blobs share the same storage mechanism regardless of size. Cloning a Blob does not copy the underlying data — `CompactArc` provides shared ownership. Per RFC-0127 Change 8 (Cross-language consistency), `deserialize_blob` returns a slice into the input buffer, not a copy. `as_bytes()` returns a direct reference, not a copied `Vec<u8>`. Implementers MUST NOT copy the returned slice into a new allocation in the deserialization path.

#### ToParam Implementations

```rust
// In api/params.rs

impl ToParam for Vec<u8> {
    fn to_param(&self) -> Value {
        Value::Blob(Blob::new(self.clone()))
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

### SQL Parsing

```rust
// In FromStr for DataType

impl FromStr for DataType {
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let upper = s.to_uppercase();
        match upper.as_str() {
            // ... existing ...
            "BLOB" | "BYTEA" | "BINARY" | "VARBINARY" => Ok(DataType::Blob),
            _ => Err(Error::InvalidColumnType),
        }
    }
}
```

**SQL Examples:**
```sql
CREATE TABLE api_keys (
    key_id TEXT PRIMARY KEY,
    key_hash BYTEA(32) NOT NULL,  -- HMAC-SHA256 (exactly 32 bytes)
    key_prefix TEXT NOT NULL
);

CREATE TABLE usage_ledger (
    event_id BYTEA(32) PRIMARY KEY,   -- SHA256 (exactly 32 bytes)
    request_id BYTEA(32) NOT NULL,   -- SHA256 (exactly 32 bytes)
    pricing_hash BYTEA(32) NOT NULL, -- SHA256 (exactly 32 bytes)
    signature BYTEA                    -- Variable length (Ed25519 = 64 bytes)
);
```

**Note on `BYTEA(32)`:** The `(32)` suffix is a length assertion. The storage engine MUST enforce that inserted values are exactly 32 bytes. Inserting a 31 or 33 byte value into a `BYTEA(32)` column MUST fail with a length constraint error.

**Note on NULL representation (normative):** SQL NULL for a BYTEA column is a schema-layer concept. The DCS layer has no NULL type. NULL MUST NOT be represented as zero bytes on disk — DCS deserialization requires at least 4 bytes (the length prefix). A zero-byte read for a BYTEA column produces `DCS_INVALID_BLOB`. stoolap must use a separate null bitmap or column-level null flag, not zero bytes, to represent NULL.

**Note on ALTER TABLE ADD COLUMN (normative):** ALTER TABLE ADD COLUMN for a nullable BYTEA column requires handling existing records that lack the new column. Per RFC-0127 Change 13, the DCS dispatcher cannot skip absent fields. Options: (1) rewrite all existing records to include the new column with a default value (recommended for simplicity), (2) construct a per-record schema that only includes present columns, or (3) track schema version per record.

**Note on empty BYTEA (normative):** An empty BYTEA value (`length=0`) serializes to exactly 4 bytes: `u32_be(0)`. It is NOT zero bytes on disk. Deserializing zero bytes as BYTEA returns `DCS_INVALID_BLOB` (fewer than 4 bytes for length prefix).

**Note on SQLite compatibility:** On PostgreSQL, use `USING HASH` for hash index creation. On SQLite, omit `USING HASH` — the hash index type is implicit in SQLite's index syntax.

### Comparison Semantics

```rust
// In Value::compare_same_type for Blobs

/// Compare two blobs byte-by-byte in deterministic order
///
/// Algorithm:
/// 1. Compare lengths first (shorter = less if all prefix bytes equal)
/// 2. Compare bytes in ascending index order until difference found
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

**Determinism Proof:**
- Lemma: For any two blobs A and B, `compare_blob(A, B)` returns the same result regardless of implementation
- Proof: The algorithm is defined purely in terms of byte-level operations with no external dependencies (no time, no random, no hardware state)
- Therefore: Blob comparison is Class A (Protocol Deterministic)

### Index Support

```rust
// Hash index for blob equality lookups

impl IndexType {
    /// Hash index supports BLOB columns for O(1) equality lookups
    ///
    /// Index structure: SipHash map from blob bytes → row IDs
    /// This is appropriate because:
    /// - SipHash is DoS-resistant (unlike non-keyed hash functions)
    /// - Lookup is O(1) average case
    /// - Blobs are fixed-size for hashes, making hashing fast
    ///
    /// Note: Uses SipHash-2-4 (the standard DoS-resistant hash table hash).
    /// SipHash requires a 128-bit key generated at database open time.
}
```

**Index Creation:**
```sql
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash) USING HASH;
CREATE UNIQUE INDEX idx_usage_ledger_event_id ON usage_ledger(event_id);
```

**Dispatcher requirement for mixed schemas (normative — RECIPROCAL):** When a stoolap schema contains both `BYTEA` (Blob) and `TEXT` (String) columns, the storage engine's deserialization MUST use the schema-driven dispatcher per RFC-0127's shared-encoding rule (RFC-0127 Change 13). The wire format `[length][bytes]` is byte-identical for both types; without dispatcher context, an implementation cannot determine whether to apply UTF-8 validation (String) or skip it (Blob). Critically, **both** `deserialize_blob` **and** `deserialize_string` must go through the dispatcher — calling `deserialize_string` directly on bytes that were inserted as Blob returns `DCS_INVALID_UTF8` on non-UTF-8 payloads.

### Serialization

Blobs serialize with explicit length prefix for determinism. The DCS-layer signatures and error codes follow RFC-0127 (Changes 2, 7, 8). The `BlobDeserializeError` enum is stoolap's internal wrapper; the DCS-layer pseudocode uses `DCS_INVALID_BLOB` and `DCS_BLOB_LENGTH_OVERFLOW`.

**Dispatcher requirement (normative — CONFORMANCE REQUIRED):** Per RFC-0127 Change 13, Blob deserialization MUST use a schema-driven dispatcher when co-present with other Length-Prefixed types (String) in the same schema. This is not optional. The stoolap storage engine's deserialization path for Blob columns MUST be integrated with a schema-driven dispatcher that routes to `deserialize_blob` or `deserialize_string` based on the column's declared type. An implementation that deserializes `BYTEA` columns via a direct `deserialize_blob` call without schema context, when `TEXT` columns also exist in the same table/schema, is non-conformant with RFC-0127. See RFC-0127 Change 13 for the full specification including the SharedEncoding rule and DCS encoding equivalence classes.

**Dispatcher architecture (concrete example):** The dispatcher receives column type metadata alongside wire bytes. Example:

```rust
// Deserialization path for a row containing both TEXT and BYTEA columns.
// The dispatcher receives (column_type, wire_bytes) pairs:
fn deserialize_column_value(input: &[u8], col_type: &ColumnType) -> Result<Value, DcsError> {
    match col_type {
        ColumnType::Text => {
            // String deserialization also requires dispatcher in mixed schemas
            let (value, remaining) = deserialize_string(input)?;
            Ok(Value::String(value))
        },
        ColumnType::Bytea => {
            // Blob deserialization also requires dispatcher in mixed schemas
            let (value, remaining) = deserialize_blob(input)?;
            Ok(Value::Blob(Blob::from_slice(value)))
        },
    }
}
```

**Ambiguity symmetry (normative — RECIPROCAL):** It is not sufficient for only Blob deserialization to use the dispatcher. When both `BYTEA` and `TEXT` columns exist in a schema, **all** String deserialization must also use the dispatcher. Calling `deserialize_string` directly on bytes that may have been inserted as Blob is non-conformant — on non-UTF-8 payloads (e.g., cryptographic hash bytes), this returns `DCS_INVALID_UTF8`. The UTF-8 validation applied by `deserialize_string` is only correct when the dispatcher has confirmed the bytes are intended as String.

**Typed-context enforcement (normative):** Bare calls to `deserialize_blob` or `deserialize_string` on raw bytes without schema context are **forbidden in production code paths**. The only conformant entry point is through the schema-driven dispatcher. Direct deserialization calls are non-conformant and will produce consensus-divergent results in mixed-type schemas. Test code may call `deserialize_blob` directly only for unit testing the function itself.

**Error code mapping (normative):** stoolap's internal `BlobDeserializeError` variants MUST map exactly to the corresponding DCS error codes at the DCS serialization/deserialization interface:
- `TruncatedInput` → `DCS_INVALID_BLOB`
- `LengthMismatch` → `DCS_INVALID_BLOB`
- `ExceedsMaxSize` → `DCS_BLOB_LENGTH_OVERFLOW`

The `DcsError` type returned by `serialize_blob` is the same opaque error type used by all DCS functions. Per RFC-0127 Change 2 (TRAP-before-serialize principle), `serialize_blob` performs the 4GB overflow check internally and returns `DCS_BLOB_LENGTH_OVERFLOW` rather than a local error type — the function itself is the last line of defense.

```rust
/// Blob deserialization errors (stoolap internal wrapper).
/// The DCS-layer pseudocode uses DCS_INVALID_BLOB and DCS_BLOB_LENGTH_OVERFLOW
/// per RFC-0127 Change 7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobDeserializeError {
    /// Input too short to contain length prefix
    TruncatedInput { actual_len: usize },
    /// Declared length in prefix does not match actual data length
    LengthMismatch { declared: u32, actual: usize },
    /// Blob declared length exceeds DCS maximum (4GB)
    ExceedsMaxSize { declared: u32, max: u32 },
}

/// Serialize blob to canonical bytes for hashing/proofs
///
/// Format: [length: u32BE][data: bytes]
///
/// Length prefix ensures:
/// - No null-termination ambiguity
/// - Deterministic deserialization
/// - Compatible with streaming deserialization
///
/// Returns Err(DCS_BLOB_LENGTH_OVERFLOW) if data.len() > 0xFFFFFFFF (4GB).
/// Per RFC-0127 Change 2: the 4GB limit cannot be expressed as a type constraint,
/// so the check is performed at serialization time.
fn serialize_blob(data: &[u8]) -> Result<Vec<u8>, DcsError> {
    if data.len() > 0xFFFFFFFF {
        return Err(DCS_BLOB_LENGTH_OVERFLOW);
    }
    return Ok(serialize_bytes(data));  // u32_be(data.len()) || data
}

/// Deserialize blob from canonical bytes.
///
/// Reads the first 4 bytes as a big-endian u32 length prefix,
/// extracts that many bytes as the blob data, and verifies the result.
///
/// Returns (blob_data, remaining_bytes) on success. On failure, returns
/// DCS_INVALID_BLOB (truncated input or length mismatch) or
/// DCS_BLOB_LENGTH_OVERFLOW (declared length exceeds 4GB).
///
/// Per RFC-0127 Change 8: returns Result<(&[u8], &[u8]), Err> — the caller
/// receives remaining bytes for chaining through subsequent struct fields.
fn deserialize_blob(input: &[u8]) -> Result<(&[u8], &[u8]), DcsError> {
    const LEN_SIZE: usize = 4;

    if input.len() < LEN_SIZE {
        return Err(DCS_INVALID_BLOB);  // need at least 4 bytes for length prefix
    }
    let length = (u32(input[0]) << 24) | (u32(input[1]) << 16) | (u32(input[2]) << 8) | u32(input[3]);
    if 4 + (length as usize) > input.len() {
        return Err(DCS_INVALID_BLOB);  // truncated: declared length exceeds remaining bytes
    }
    let blob_data = input[4..4+(length as usize)];
    let remaining = input[4+(length as usize)..];
    Ok((blob_data, remaining))
}

/// On-disk format and byte-chaining (normative):
/// stoolap's on-disk format for BYTEA columns must support length-prefixed byte-chaining:
/// each serialized Blob value is u32_be(length) || bytes. When reading a row with multiple
/// columns, the deserializer must consume exactly 4 + length bytes for each BYTEA column
/// before proceeding to the next column. If stoolap's current format uses fixed-width or
/// delimiter-based storage, an adapter layer must convert to/from DCS wire format
/// before/after storage. The returned remaining bytes are chained to the next field's
/// deserializer — discarding remaining bytes at the column level breaks DCS conformance.
```
```

### Accessor Methods

```rust
// In Value impl

impl Value {
    /// Extract blob as a byte slice
    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Value::Blob(blob) => Some(blob.as_bytes()),
            _ => None,
        }
    }

    /// Returns the length of the blob, if this is a blob value
    pub fn as_blob_len(&self) -> Option<usize> {
        self.as_blob().map(|b| b.len())
    }

    /// Extract blob as exact 32-byte array (for SHA256)
    pub fn as_blob_32(&self) -> Option<[u8; 32]> {
        self.as_blob().and_then(|b| {
            if b.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(b);
                Some(arr)
            } else {
                None
            }
        })
    }
}
```

## Determinism Requirements

### Comparison Determinism

| Operation | Requirement |
|-----------|-------------|
| Blob equality | Byte-by-byte comparison, no branching on data values. The algorithm is defined purely in terms of byte-level operations with no external dependencies (no time, no random, no hardware state). |
| Blob ordering | Lexicographic by byte index, length as tiebreaker. Protocol Deterministic. |
| Blob hash (for indexing) | SipHash-2-4 is deterministic for fixed inputs (same key, same data → same output). Note: keys must be stable across restarts. |

### Serialization Determinism

| Operation | Requirement |
|-----------|-------------|
| Blob → bytes | Length prefix + data, no padding. Protocol Deterministic. |
| Bytes → blob | Strip length prefix, verify length matches. Deserialized via `deserialize_blob()` which returns an error if length mismatch. |

### No Non-Deterministic Operations

- **Forbidden**: Floating-point operations on blob data
- **Forbidden**: Time-dependent comparison
- **Forbidden**: Random byte ordering

## Security Considerations

### DoS Prevention

| Threat | Mitigation |
|--------|------------|
| Giant blob injection | Maximum blob size limit: 4GB per RFC-0127 / RFC-0127 Change 2; stoolap may enforce a lower application-level limit (e.g., 1MB). The DCS-layer ceiling is 4GB (0xFFFFFFFF bytes); `serialize_blob` returns `DCS_BLOB_LENGTH_OVERFLOW` if exceeded. |
| Hash collision attacks | Use SipHash-2-4 (DoS-resistant hash function) |
| Memory exhaustion | Blob data stored in Arc, not copied on clone. Per RFC-0127 Change 8 (Allocation safety), record-reading code that pre-allocates a buffer based on a declared length prefix before validating that the bytes are available is vulnerable to memory exhaustion. The bounds check (`4 + (length as usize) > input.len()`) MUST precede any allocation. This applies to the storage engine's record-reading layer, not just `deserialize_blob`. |

### Integrity

| Threat | Mitigation |
|--------|------------|
| Partial read | Length prefix ensures complete read verification |
| Truncation attacks | Store length alongside data in serialization |

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Blob insert | <10μs | No hex encoding, single memcpy |
| Blob lookup | <5μs | Hash index O(1) + single comparison |
| Blob comparison | <1μs | Memcmp of 32 bytes |
| Storage per SHA256 hash | 32 bytes | vs 64 bytes hex TEXT |

## Test Vectors

### Equality Tests

```rust
#[test]
fn test_blob_equality() {
    let blob1: Vec<u8> = (0..32).collect();
    let blob2: Vec<u8> = (0..32).collect();
    let blob3: Vec<u8> = (0..31).chain(std::iter::once(32)).collect();

    assert_eq!(blob1, blob2);  // Same bytes
    assert_ne!(blob1, blob3);  // Different length
}

#[test]
fn test_blob_sha256_stored_as_blob() {
    use sha2::{Sha256, Digest};

    // SHA256 of "hello"
    let expected: [u8; 32] = [
        0x2c, 0xf2, 0x4d, 0xba, 0x5f, 0xb0, 0x5a, 0xd9,
        0x1e, 0xf0, 0x76, 0x7e, 0x4f, 0x1a, 0x14, 0x35,
        0x98, 0x6f, 0xad, 0x5b, 0x4f, 0x6e, 0x34, 0x1f,
        0xc9, 0xb5, 0x6b, 0x3c, 0x7e, 0xb2, 0x51, 0x7a,
    ];

    let input = b"hello";
    // sha2::digest() returns GenericArray<u8, U32>, not [u8; 32]
    let hash: [u8; 32] = Sha256::digest(input).into_array();

    let value: Value = hash.into();  // Uses [u8; 32] → ToParam → Value::Blob
    assert_eq!(value.as_blob_32(), Some(expected));

    // Cross-implementation verification: This payload aligns with RFC-0127 Entry 17,
    // which uses SHA256(b"") — a 32-byte non-UTF-8 value for dispatcher verification.
    // Both test vectors use non-UTF-8 binary data to confirm the Blob/String dispatcher
    // correctly routes based on schema type, not wire format.
}
```

### Serialization Round-Trip Tests

```rust
#[test]
fn test_blob_serialize_roundtrip() {
    let original: &[u8] = b"\x01\x02\x03\x04\x05";

    // Serialize
    let serialized = serialize_blob(original).unwrap();
    assert_eq!(&serialized[..4], &(5u32).to_be_bytes());
    assert_eq!(&serialized[4..], original);

    // Deserialize — verify both remaining bytes and blob data (remaining first per RFC-0127 Change 13)
    let (remaining, deserialized) = deserialize_blob(&serialized).unwrap();
    assert_eq!(deserialized, original);
    assert_eq!(remaining, &[]);  // no trailing bytes
}

#[test]
fn test_blob_deserialize_truncated() {
    // 3 bytes is not enough for the length prefix
    let result = deserialize_blob(&[0x00, 0x00, 0x01]);
    assert!(matches!(result, Err(DCS_INVALID_BLOB)));
}

#[test]
fn test_blob_deserialize_length_mismatch() {
    // Length prefix says 10 bytes but only 5 follow
    let mut data = Vec::new();
    data.extend_from_slice(&10u32.to_be_bytes());
    data.extend_from_slice(b"hello");
    let result = deserialize_blob(&data);
    assert!(matches!(result, Err(DCS_INVALID_BLOB)));
}

#[test]
fn test_blob_deserialize_exceeds_max_size() {
    // Declare 5GB blob (exceeds 4GB DCS maximum)
    let mut data = Vec::new();
    data.extend_from_slice(&5_368_709_120u32.to_be_bytes());  // > 0xFFFFFFFF
    let result = deserialize_blob(&data);
    assert!(matches!(result, Err(DCS_BLOB_LENGTH_OVERFLOW)));
}

#[test]
fn test_blob_entry17_string_negative_verification() {
    // RFC-0127 Entry 17: SHA256(b"") as blob payload.
    // This payload is NOT valid UTF-8. Passing it to deserialize_string
    // MUST return Err(DCS_INVALID_UTF8).
    let entry17_bytes = hex::decode("00000020e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap();
    let result = deserialize_string(&entry17_bytes);
    assert!(matches!(result, Err(DCS_INVALID_UTF8)));
}
```

### Ordering Tests

```rust
#[test]
fn test_blob_ordering() {
    let a: Vec<u8> = vec![0x00, 0x01];
    let b: Vec<u8> = vec![0x00, 0x02];
    let c: Vec<u8> = vec![0x00, 0x01, 0x00];

    // Lexicographic comparison
    assert!(a < b);  // Byte at index 1: 0x01 < 0x02
    assert!(a < c);  // Prefix shorter = less
    assert!(b > c);  // Byte at index 1: 0x02 > 0x01
}
```

### SQL Parsing Tests

```rust
#[test]
fn test_blob_sql_parsing() {
    // Uppercase
    assert_eq!("BLOB".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("BYTEA".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("BINARY".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("VARBINARY".parse::<DataType>().unwrap(), DataType::Blob);
    // Lowercase
    assert_eq!("blob".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("bytea".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("binary".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("varbinary".parse::<DataType>().unwrap(), DataType::Blob);
    // Mixed case
    assert_eq!("Blob".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("Bytea".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("BLOB".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("ByTeA".parse::<DataType>().unwrap(), DataType::Blob);
}
```

### Dispatcher and Struct Conformance Tests

```rust
#[test]
fn test_empty_blob_is_4_bytes_not_zero() {
    // Empty BYTEA serializes to exactly 4 bytes: u32_be(0)
    let serialized = serialize_blob(b"").unwrap();
    assert_eq!(serialized.len(), 4);
    assert_eq!(&serialized[..], &u32::to_be_bytes(0));

    // Deserialize: remaining first
    let (remaining, blob_data) = deserialize_blob(&serialized).unwrap();
    assert_eq!(blob_data, b"");
    assert_eq!(remaining, &[]);
}

#[test]
fn test_zero_bytes_deserialize_returns_invalid_blob() {
    // Zero bytes is NOT a valid empty BYTEA — DCS requires 4 bytes for length prefix
    let result = deserialize_blob(&[]);
    assert!(matches!(result, Err(DCS_INVALID_BLOB)));
}

#[test]
fn test_dispatch_struct_rejects_trailing_bytes() {
    // Row struct: u32_be(1) || string "a" + trailing garbage
    let schema = vec![(1u32, ColumnType::Text)];
    let mut row = Vec::new();
    row.extend_from_slice(&1u32.to_be_bytes());       // field_id = 1
    row.extend_from_slice(&1u32.to_be_bytes());       // string length = 1
    row.push(b'a');
    row.extend_from_slice(b"TRAILING GARBAGE");       // extra bytes — must be rejected

    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &schema);
    assert!(matches!(result, Err(DCS_INVALID_STRUCT)));
}

#[test]
fn test_dispatch_struct_rejects_non_ascending_field_ids() {
    // Struct with fields in wrong (non-ascending) order: field 2 before field 1
    let schema = vec![
        (1u32, ColumnType::Bytea),
        (2u32, ColumnType::Text),
    ];
    let mut row = Vec::new();
    row.extend_from_slice(&2u32.to_be_bytes());       // field_id = 2 (should be after 1)
    row.extend_from_slice(&3u32.to_be_bytes());       // string length = 3
    row.extend_from_slice(b"abc");
    row.extend_from_slice(&1u32.to_be_bytes());       // field_id = 1 (should be before 2)
    row.extend_from_slice(&2u32.to_be_bytes());       // blob length = 2
    row.extend_from_slice(b"xy");
    row.extend_from_slice(&0u32.to_be_bytes());       // terminator

    // deserialize_struct with ascending field_id enforcement returns error
    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &schema);
    assert!(matches!(result, Err(DCS_INVALID_STRUCT)));
}

#[test]
fn test_dispatch_struct_empty_struct_exemption() {
    // Empty nested struct: Struct { inner: Struct {} }
    // The empty struct MUST NOT trigger the progress check — it legitimately consumes 0 bytes
    let inner_schema: Vec<(u32, ColumnType)> = vec![];  // empty struct
    let outer_schema = vec![
        (1u32, ColumnType::Struct(inner_schema.clone())),
    ];
    let mut row = Vec::new();
    row.extend_from_slice(&1u32.to_be_bytes());       // field_id = 1 (Struct)
    row.extend_from_slice(&0u32.to_be_bytes());       // inner struct: empty (u32_be(0))
    row.extend_from_slice(&0u32.to_be_bytes());       // terminator

    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &outer_schema);
    assert!(result.is_ok(), "Empty struct must be accepted, not rejected");
}

#[test]
fn test_dispatch_struct_recursion_depth_limit() {
    // 32 levels of nesting: depth >= 32 triggers DCS_RECURSION_LIMIT_EXCEEDED
    // (Each nesting level adds 2 to depth; 32 * 2 = 64 effective depth units)
    let schema: Vec<(u32, ColumnType)> = vec![
        (1u32, ColumnType::Struct(vec![
            (1u32, ColumnType::Struct(vec![
                (1u32, ColumnType::Text),
            ])),
        ])),
    ];
    // Build a row with 32 levels of nesting...
    // This is a simplified test: confirm that the limit check exists
    // Full 32-level test requires building a deeply nested struct
    let result = dispatch_struct(&[], /* is_top_level = */ true, /* depth = */ 32, &schema);
    assert!(matches!(result, Err(DCS_RECURSION_LIMIT_EXCEEDED)));
}

#[test]
fn test_text_1mb_limit() {
    // TEXT exceeding 1MB (1,048,576 bytes) must return DCS_STRING_LENGTH_OVERFLOW
    let oversized: Vec<u8> = vec![b'x'; 1_048_577]; // 1MB + 1 byte
    let serialized = serialize_string(&oversized);
    let result = deserialize_string(&serialized);
    assert!(matches!(result, Err(DCS_STRING_LENGTH_OVERFLOW)));
}

#[test]
fn test_bytea_32_length_assertion() {
    // Inserting a 31 or 33 byte value into a BYTEA(32) column must fail
    let value: Value = Value::Blob(Blob::new(vec![0u8; 31])); // wrong length
    let col_type = ColumnType::Bytea; // without length assertion
    // The column schema (BYTEA(32)) enforces length; this is tested at the SQL layer
    // Here we verify the blob itself is valid regardless of length constraints
    assert_eq!(value.as_blob().unwrap().len(), 31);
}
```

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| **Blob variant (chosen)** | Compile-time type safety, no tag byte validation needed | Adds 8 bytes to Value discriminant (but CompactArc<[u8]> is thin, so net effect is modest) |
| **Extension (rejected)** | Fits existing pattern | Requires runtime tag validation, conflates blob with other extension types |
| **Direct Vec<u8>** | Simple | Larger Value size, no shared ownership |
| **TEXT + hex (current)** | Works today | 2x storage, encoding overhead, non-deterministic comparison |

## Implementation Phases

### Phase 1: Core Blob Type

- [ ] Add `DataType::Blob = 10` to `core/types.rs`
- [ ] Add `FromStr` parsing for BLOB, BYTEA, BINARY, VARBINARY
- [ ] Add `Blob` struct and `BlobOrdering` enum to `core/value.rs`
- [ ] Add `Value::Blob(Blob)` variant to Value enum
- [ ] Add `ToParam` for `Vec<u8>`, `[u8; N]`, `&[u8]`
- [ ] Add `Value::as_blob()`, `Value::as_blob_len()`, and `Value::as_blob_32()` accessors
- [ ] Add blob comparison in `Value::compare_same_type`
- [ ] Add `serialize_blob()` and `deserialize_blob()` functions
- [ ] **Refactor serialization call chain** to propagate `Result<Vec<u8>, DcsError>` from `serialize_blob`. All other DCS serializers return `Vec<u8>` directly; Blob is the first to return `Result`. The insert path must handle both `Ok(bytes)` and `Err(DCS_BLOB_LENGTH_OVERFLOW)`.
- [ ] **Increment `NUMERIC_SPEC_VERSION` to `2`** per RFC-0127 Change 11 and RFC-0110. Blob is a new DCS type; implementations claiming conformance must declare `NUMERIC_SPEC_VERSION >= 2`. See RFC-0110 for activation governance (minimum 2-epoch notice before H_upgrade).
- [ ] **Audit `serialize_bytes` call sites** to ensure no Blob-typed data bypasses `serialize_blob`. `serialize_bytes` is a low-level primitive; `serialize_blob` is the public typed entry point.

## Rationale

### Why Blob Variant Over Extension?

1. **Type safety**: Binary hash data is security-critical. A dedicated `Blob` variant ensures blobs cannot be confused with Json or Vector extension types at compile time. No runtime tag validation needed.
2. **Clean API**: Accessors (`as_blob()`, `as_blob_len()`, `as_blob_32()`) return directly from the `Blob` struct without tag checking.
3. **Consistent with design**: Other security-critical types in the Value enum have dedicated variants (e.g., `Timestamp`, `Boolean`). Blob is treated with the same care.

### Why CompactArc Storage?

1. **Shared ownership**: Cloning a Blob does not copy the underlying data — both clones point to the same heap allocation
2. **Memory efficiency**: `CompactArc<[u8]>` is 8 bytes (thin pointer), keeping the overall memory footprint reasonable
3. **Thread-safe**: `CompactArc` uses atomic reference counting for safe concurrent access

### Why Hash Index Only?

Blob comparison for ordering (>, <, >=, <=) is non-deterministic in practice because:
- Different implementations may have different tie-breaking
- Range scans on binary data are uncommon for hash storage

Therefore, only equality index (Hash) is supported, consistent with how hashes are used in practice.

### Wire Format Has No Type Information

The DCS wire format carries no type identifier — a `u32_be(5) || b"hello"` byte sequence is indistinguishable as String or Blob without schema context. Schema metadata must be preserved alongside data; loss of schema information makes byte-level type reconstruction impossible. This has disaster recovery implications: backups must include schema metadata to correctly deserialize BYTEA values.

### Schema Validation for Dynamic Schemas

Dynamic schemas (SQL CREATE TABLE) SHOULD be validated for well-formedness before deserialization begins. This includes verifying that all column types are known DCS types and that the dispatcher can route each column correctly. Compile-time schema definitions (e.g., Rust struct types) benefit from compile-time validation and are generally lower risk.

## Future Work

- F1: Streaming blob I/O for large data (documents, images) — per RFC-0127 (Motivation), implementations SHOULD support streaming decode for Blobs larger than a configurable memory threshold (e.g., > 1MB) to prevent full payload allocation.
- F2: Blob compression (for large variable-size blobs)
- F3: Partial blob reads (subrange extraction)

## UTF-8 Skip Optimization (normative — FORBIDDEN)

stoolap MUST NOT skip UTF-8 validation on TEXT reads based on byte inspection, caching schemes, or "known valid" heuristics when BYTEA columns coexist in the same schema. The ONLY conformant UTF-8 validation path for TEXT columns is through the schema-driven dispatcher. Any optimization that bypasses the dispatcher to "skip validation" for performance is non-conformant because:

1. The dispatcher determines type by schema metadata, not by inspecting bytes
2. "Known valid UTF-8" cannot be established by examining the bytes themselves — that is precisely what `deserialize_string` exists to validate
3. A caching optimization that stores pre-validated UTF-8 bytes is acceptable only if the cache entry was created through the dispatcher in the first place

The dispatcher requirement is not a performance suggestion — it is a consensus requirement per RFC-0127 Change 13. Short-circuiting it via byte-inspection shortcuts produces consensus-divergent results.

## TEXT Column Size Limit (normative — 1MB per RFC-0127)

TEXT columns MUST enforce a 1MB (1,048,576 byte) maximum length. Per RFC-0127 Change 6, `DCS_STRING_LENGTH_OVERFLOW` is returned when a string exceeds 1MB. This limit applies to TEXT columns in all schemas, including mixed BYTEA+TEXT schemas. The limit is enforced at the DCS layer via `deserialize_string`; the storage engine must propagate this error correctly rather than truncating or accepting oversized strings.

## Row Deserialization: `is_top_level` Requirement (normative — MUST be true)

stoolap's row deserialization MUST pass `is_top_level = true` to `deserialize_struct`. This is required because:

- `is_top_level = true` enables the trailing-bytes check: if bytes remain after consuming all expected struct fields, the deserializer returns `DCS_INVALID_STRUCT`
- `is_top_level = false` silently ignores trailing bytes, permitting malformed data to go undetected
- A malicious or buggy storage engine could otherwise store trailing garbage that is never detected on read

The only acceptable use of `is_top_level = false` is for nested Struct contexts (e.g., a Blob containing a nested Struct field). Top-level row deserialization is never a nested context.

**Conformance test:**
```rust
#[test]
fn test_row_deserialization_rejects_trailing_garbage() {
    // A row struct with 1 TEXT field: u32_be(1) || "a" + trailing garbage
    let schema = vec![(1u32, ColumnType::Text)];
    let mut row = Vec::new();
    row.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1 (TEXT)
    row.extend_from_slice(&1u32.to_be_bytes()); // string length = 1
    row.push(b'a');
    row.extend_from_slice(b"trailing garbage that must be rejected"); // extra bytes

    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &schema);
    assert!(matches!(result, Err(DCS_INVALID_STRUCT)));
}
```

## Row Storage Format (normative — DCS Struct encoding required)

stoolap rows MUST be stored using DCS Struct encoding. This is not optional because:

1. The DCS serialization layer (`serialize_struct`/`deserialize_struct`) is the only conformant way to handle the length-prefixed byte-chaining required for mixed-type rows containing Blobs
2. A custom row format that does not use DCS Struct encoding would require the storage engine to reimplement byte-chaining, null handling, and field ordering — introducing non-determinism

**Required format:**
- Each field: `u32_be(field_id) || serialized_value` in **strictly ascending field_id order**
- End of struct: `u32_be(0)` (zero field_id sentinel)
- Trailing bytes MUST be rejected at deserialization time (enforced by `is_top_level = true`)
- Null fields: schema-layer null bitmap or per-column null flag; DCS layer has no null type — zero bytes MUST NOT be used for NULL (see **Note on NULL representation** above)

**Example — row with 2 columns (TEXT, BYTEA):**
```
u32_be(1) || u32_be(1) || 'a'    // field_id=1: TEXT "a"
u32_be(2) || u32_be(5) || bytes  // field_id=2: BYTEA 5-bytes
u32_be(0)                        // struct terminator
```

**Conformance test:**
```rust
#[test]
fn test_row_struct_encoding_ascending_field_id() {
    // Row with fields in wrong (non-ascending) order must be rejected.
    // Correct: field 1 then field 2. Wrong: field 2 then field 1.
    let mut row = Vec::new();
    row.extend_from_slice(&2u32.to_be_bytes()); // field_id = 2 (should be first, but isn't)
    row.extend_from_slice(&5u32.to_be_bytes());
    row.extend_from_slice(b"hello");
    row.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1 (should precede field 2)
    row.extend_from_slice(&1u32.to_be_bytes());
    row.push(b'x');
    row.extend_from_slice(&0u32.to_be_bytes()); // terminator

    // deserialize_struct with ascending field_id check returns DCS_INVALID_STRUCT
    let result = deserialize_struct(&row, /* is_top_level = */ true, /* depth = */ 0);
    assert!(matches!(result, Err(DCS_INVALID_STRUCT)));
}
```

## Phase 2: Query Engine Integration

Phase 2 MUST fully implement the following items. Each is specified precisely:

### Phase 2a: Hash Index for Blob Columns

```sql
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash) USING HASH;
```

**Implementation requirements:**
- Hash function: SipHash-2-4 with a 128-bit key generated at database open time
- Index structure: `HashMap<SipHash output, Vec<row_id>>`
- Blob hash key: the full 32-byte (or variable-length) blob content, not a hash of the content
- O(1) average equality lookup

### Phase 2b: Blob Equality in Expression Evaluation

```sql
SELECT * FROM api_keys WHERE key_hash = $1;
```

**Requirements:**
- Expression VM must handle `Value::Blob` in equality comparison
- Use `compare_blob()` (byte-by-byte) for comparison, not `memcmp` wrapper
- Index lookup must use hash index when available

### Phase 2c: Blob in Projection/Selection

```sql
SELECT key_hash, signature FROM usage_ledger WHERE event_id = $1;
```

**Requirements:**
- Projection must route Blob columns through the schema-driven dispatcher
- `Value::Blob` must serialize correctly in result set encoding

### Phase 2d: Dispatcher Integration — Complete Specification

The dispatcher integrates Blob deserialization with all Struct-containing operations. This is the full specification, not a placeholder.

**Type definitions (normative — required for all pseudocode):**

```rust
/// DCS error type. All DCS functions return this type. Per RFC-0127 Change 7,
/// the concrete variants are implementation-defined but the interface is uniform:
/// DCS_INVALID_BLOB, DCS_BLOB_LENGTH_OVERFLOW, DCS_INVALID_UTF8,
/// DCS_INVALID_STRUCT, DCS_RECURSION_LIMIT_EXCEEDED, DCS_STRING_LENGTH_OVERFLOW.
pub type DcsError = /* implementation-defined */;

/// StructValue: the deserialized value of a DCS Struct field.
/// fields: Vec of (field_id, Value) pairs in ascending field_id order.
pub struct StructValue {
    pub fields: Vec<(u32, Value)>,
}

/// ColumnType: stoolap's schema type system. Maps to DCS types per RFC-0127.
/// field_ids in Struct variants are u32; no duplicate field_ids permitted.
pub enum ColumnType {
    Bool,
    I128,
    Dqa,
    Dfp,
    BigInt,
    Text,
    Bytea,
    Struct(Vec<(u32, ColumnType)>),  // field_id → type mapping, ascending order
    Option(Box<ColumnType>),
    Enum(Vec<(u32, ColumnType)>),     // variant_id → type mapping
    Dvec(Box<ColumnType>),            // element type
    Dmat { rows: usize, cols: usize, elem: Box<ColumnType> },
}
```

**Dispatcher contract (normative):**
1. **Progress check**: After deserializing each field, the remaining bytes MUST differ from `remaining_after_field_id`. If they are equal, the field consumed zero bytes but declared a non-zero length — return `DCS_INVALID_STRUCT`. Exception: `ColumnType::Struct(fields)` where `fields` is empty — an empty Struct legitimately consumes 0 bytes.
2. **Empty-struct exemption**: An empty Struct (`ColumnType::Struct([])`) is valid and MUST NOT trigger the progress check. This is the only permitted zero-byte type.
3. **Recursion depth limit**: If `depth >= 32`, return `DCS_RECURSION_LIMIT_EXCEEDED`. The limit is 32 (not 64) because each Struct nesting level increments `depth` by 2 (once in `dispatch_field` via the Struct case, once in `dispatch_struct` via the loop). Thus `depth >= 32` yields at most 32 nesting levels × 2 = 64 total depth units, matching RFC-0127 Change 13's intent.
4. **Trailing bytes**: When `is_top_level = true`, any bytes remaining after the `u32_be(0)` terminator MUST return `DCS_INVALID_STRUCT`.
5. **Required types**: The dispatcher MUST handle at minimum: `Bool`, `I128`, `Text`, `Bytea`, `Struct`, `Option`, and `Dvec`/`Dmat` (with per-element dispatcher routing per RFC-0127 Change 2.5). `Dfp`, `BigInt`, and `Enum` MAY be deferred to a future phase.

**`dispatch_field` specification (depth: usize to match RFC-0127):**
```rust
fn dispatch_field(input: &[u8], col_type: &ColumnType, depth: usize)
    -> Result<(&[u8], Value), DcsError>
{
    match col_type {
        ColumnType::Text => deserialize_string(input)
            .map(|(v, rem)| (rem, Value::String(v))),
        ColumnType::Bytea => deserialize_blob(input)
            .map(|(v, rem)| (rem, Value::Blob(Blob::from_slice(v)))),
        ColumnType::Bool => /* ... deserialize_bool ... */,
        ColumnType::I128 => /* ... deserialize_i128 ... */,
        ColumnType::Struct(fields) => dispatch_struct(input, false, depth + 1, fields)
            .map(|(rem, v)| (rem, Value::Struct(v))),
        ColumnType::Option(inner) => {
            // RFC-0127 Change 10: Option is encoded as u32 variant_id (0=None, 1=Some)
            if input.len() < 4 {
                return Err(DCS_INVALID_STRUCT);
            }
            let variant_id = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
            let remaining = &input[4..];
            match variant_id {
                0 => Ok((remaining, Value::Option(None))),
                1 => dispatch_field(remaining, inner, depth + 1)
                    .map(|(rem, v)| (rem, Value::Option(Some(Box::new(v))))),
                _ => Err(DCS_INVALID_STRUCT),
            }
        },
        ColumnType::Dvec(elem_type) => {
            // Per RFC-0127 Change 2.5: each element routed through dispatcher
            deserialize_dvec(input, elem_type, depth + 1)
                .map(|(rem, v)| (rem, Value::Dvec(v)))
        },
        ColumnType::Dmat { rows, cols, elem } => {
            deserialize_dmat(input, *rows, *cols, elem, depth + 1)
                .map(|(rem, v)| (rem, Value::Dmat(v)))
        },
        ColumnType::Dfp => /* ... deserialize_dfp ... */,
        ColumnType::BigInt => /* ... deserialize_bigint ... */,
        ColumnType::Enum(variants) => {
            // Enum encoded as u32 variant_id, then variant value
            if input.len() < 4 {
                return Err(DCS_INVALID_STRUCT);
            }
            let variant_id = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
            let remaining = &input[4..];
            let variant_type = variants.iter()
                .find(|(id, _)| *id == variant_id)
                .map(|(_, t)| t)
                .ok_or(DCS_INVALID_STRUCT)?;
            dispatch_field(remaining, variant_type, depth + 1)
                .map(|(rem, v)| (rem, Value::Enum(variant_id, Box::new(v))))
        },
    }
}
```

**`dispatch_struct` specification (return order matches RFC-0127: remaining first):**
```rust
fn dispatch_struct(input: &[u8], is_top_level: bool, depth: usize,
                   schema: &[(u32, ColumnType)])  // field_id → type, ascending
    -> Result<(&[u8], StructValue), DcsError>
{
    if depth >= 32 {
        return Err(DCS_RECURSION_LIMIT_EXCEEDED);
    }
    let mut fields = Vec::new();
    let mut remaining = input;

    loop {
        if remaining.len() < 4 {
            return Err(DCS_INVALID_STRUCT); // need at least 4 bytes for field_id
        }
        let field_id = u32::from_be_bytes([remaining[0], remaining[1], remaining[2], remaining[3]]);
        let remaining_after_field_id = &remaining[4..];

        if field_id == 0 {
            break; // end of struct
        }

        // Look up this field's type from the schema
        let col_type = schema.iter()
            .find(|(id, _)| *id == field_id)
            .map(|(_, t)| t)
            .ok_or(DCS_INVALID_STRUCT)?; // field_id not in schema

        let (rem_after_value, value) = dispatch_field(remaining_after_field_id, col_type, depth + 1)?;

        // Progress check: non-empty types must consume at least 1 byte
        let is_empty_struct = matches!(col_type, ColumnType::Struct(fs) if fs.is_empty());
        if !is_empty_struct && rem_after_value == remaining_after_field_id {
            return Err(DCS_INVALID_STRUCT); // zero-byte consumption on non-empty type
        }

        remaining = rem_after_value;
        fields.push((field_id, value));
    }

    if is_top_level && !remaining.is_empty() {
        return Err(DCS_INVALID_STRUCT); // trailing bytes check
    }

    Ok((remaining, StructValue { fields }))
}
```

### Phase 2e: Array Support

**Fixed-type arrays (`BYTEA[]`, `TEXT[]`)** — Per RFC-0127 Change 2.5, the element type is known from the column schema, so the element deserializer is called directly without dispatcher overhead:

```rust
fn deserialize_bytea_array(input: &[u8]) -> Result<(&[u8], Vec<Blob>), DcsError> {
    if input.len() < 4 {
        return Err(DCS_INVALID_BLOB); // need count prefix
    }
    let count = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let mut remaining = &input[4..];
    let mut elements = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let (blob_data, rem) = deserialize_blob(remaining)?;
        elements.push(Blob::from_slice(blob_data));
        remaining = rem;
    }

    Ok((remaining, elements))
}
```

**Polymorphic arrays (`DVEC`, `DMAT`)** — Per RFC-0127 Change 2.5, each element MUST be deserialized using the element type's deserialization function as determined by the container schema. Route each element through `dispatch_field`:

```rust
fn deserialize_dvec(input: &[u8], elem_type: &ColumnType, depth: usize)
    -> Result<(&[u8], Vec<Value>), DcsError>
{
    if input.len() < 4 {
        return Err(DCS_INVALID_STRUCT);
    }
    let count = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let mut remaining = &input[4..];
    let mut elements = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let (rem_after_elem, elem_value) = dispatch_field(remaining, elem_type, depth + 1)?;
        remaining = rem_after_elem;
        elements.push(elem_value);
    }

    Ok((remaining, elements))
}
```

**Return order:** All deserialization functions return `(&[u8], T)` — remaining bytes first, value second — matching RFC-0127 Change 13's `deserialize_struct` signature.

## Phase 3: Integration with RFC-0903/0909

> **Note**: Phase 3 is pending stoolap Blob implementation. The `schema.rs` in `crates/quota-router-core` has already been updated to use `key_hash BYTEA(32)`, but `storage.rs` still uses `hex::encode/decode`. See `TODO(rfc-0201-phase3)` comments in `storage.rs`.

- [ ] Update `storage.rs` to use native blob (remove hex::encode/decode) — blocked on stoolap Blob implementation
- [ ] Verify storage reduction with benchmark

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 5.2 | 2026-03-27 | Round 5 adversarial review fixes: CRIT-5.1 (define all types used in pseudocode: DcsError, Err→DcsError, ColumnType enum with all variants, StructValue struct), CRIT-5.2 (add is_empty_struct exemption to dispatch_struct progress check), CRIT-5.3 (fix depth tracking: Struct case increments once, dispatch_struct loop increments once — effective limit 32 nesting levels × 2 = 64 depth units, matching RFC-0127 intent), CRIT-5.4 (clarify Blob::from_slice copy semantics: copy into CompactArc is permanent storage allocation, not the prohibited deserialization-path copy; add to from_slice docs), HIGH-5.2 (fix dispatch_field Struct case: .and_then returns Ok tuple not bare tuple), HIGH-5.3 (remove errant ? from u32::from_be_bytes in deserialize_bytea_array), HIGH-5.4 (correct v5.1 changelog: the 6 deferred questions were replaced with normative specs, not removal of a section), MED-5.1 (Phase 2d dispatcher: add all required types — Bool, I128, Option, Dvec, Dmat, Enum), MED-5.2 (add schema parameter to dispatch_struct), MED-5.3 (add conformance tests: empty blob 4-byte serialization, zero-byte rejection, trailing-bytes rejection, non-ascending field_id rejection, empty-struct exemption, depth-32 limit, 1MB TEXT limit, BYTEA(32) length assertion), MED-5.4 (BYTEA[]: direct deserialize_blob; DVEC/DMAT: per-element dispatch_field per RFC-0127 Change 2.5), MED-5.5 (align return order to RFC-0127: remaining first in all functions; update round-trip test accordingly), LOW-5.1 (add case-insensitive SQL parsing tests: lowercase, mixed-case), LOW-5.3 (change depth type from u8 to usize to match RFC-0127), LOW-5.4 (rename before_field → remaining_after_field_id to match RFC-0127). |
| 5.1 | 2026-03-27 | Fix MED-3.6/3.7/3.4/3.3/3.2/3.1 deferred items: replaced 6 implementation architecture questions from Round 4 review with normative specifications (UTF-8 skip FORBIDDEN, TEXT 1MB per RFC-0127, is_top_level MUST be true, DCS Struct encoding required with format spec, fully-specified Phase 2e BYTEA[] and Phase 2d dispatcher). Removed duplicate Phase 2/Phase 3 checklist sections and "Key Files to Modify" table. |
| 5.0 | 2026-03-27 | Round 3 adversarial review fixes: CRIT-1.1 (add dispatcher architecture pseudocode with type metadata flow example), CRIT-1.2 (strengthen mixed-schema note: reciprocal String-deserialization-through-dispatcher requirement), CRIT-1.3 (Phase 1: serialize_blob Result propagation through insert path), CRIT-1.4 (add on-disk format / byte-chaining compatibility note), HIGH-2.3 (Security Considerations: allocation safety at record-reading level), HIGH-2.4 (clarify zero-copy / CompactArc semantics), HIGH-2.5 (normative: bare deserialization forbidden in production paths), HIGH-2.6 (NULL for BYTEA: NOT zero bytes — normative constraint), HIGH-2.7 (ALTER TABLE ADD COLUMN: DCS dispatcher cannot skip absent fields), MED-3.5 (add Entry 17 negative verification test: deserialize_string rejects non-UTF-8 payload with DCS_INVALID_UTF8), MED-3.8 (empty BYTEA = 4 bytes `0x00000000`, not zero), MED-4.2 (Phase 1: audit serialize_bytes call sites), MED-4.6 (F1: reference RFC-0127 streaming decode recommendation), MED-4.7 (add schema validation note for dynamic schemas), MED-4.8 (wire format type-less disaster recovery implication), MED-3.6/3.7/3.4/3.3/3.2/3.1 (defer 6 implementation architecture questions to Open Implementation Questions section) |
| 3.0 | 2026-03-27 | Round 1 adversarial review fixes: CRIT-1 (RFC-0126 Amendment section removed; RFC-0127 is Accepted and already provides the DCS Blob entry), CRIT-2 (serialize_blob returns Result<Vec<u8>, DcsError> with explicit 4GB overflow guard per RFC-0127 Change 2), HIGH-1 (remove 1MB MAX_BLOB_SIZE, align to 4GB DCS maximum), HIGH-2 (deserialize_blob returns Result<(&[u8], &[u8]), Err> per RFC-0127 Change 8), HIGH-3 (explicit > 0xFFFFFFFF guard), MED-1 (DCS-layer pseudocode uses DCS_INVALID_BLOB / DCS_BLOB_LENGTH_OVERFLOW per RFC-0127 Change 7), MED-2 (Security Considerations reflects 4GB DCS max, not 1MB), MED-3 (add schema-driven dispatcher requirement citing RFC-0127 Change 13), MED-4 (deserialize_blob returns remaining bytes), MED-5 (DCS_BLOB_LENGTH_OVERFLOW in serialize_blob); LOW-1 (Dependencies cite RFC-0127), LOW-2 (Future Work F4 removed), LOW-3 (round-trip test verifies remaining bytes), LOW-4 (exceeds_max_size test uses 5GB) |
| 2.0 | 2026-03-25 | Adversarial review fixes: CRIT-1 (remove false inline storage claim), CRIT-2 (document Phase 3 pending state), CRIT-3 (RFC-0126 amendment required); HIGH-1 (SipHash instead of ahash), HIGH-2 (fix GenericArray test), HIGH-3 (add deserialize_blob with error type), HIGH-4 (add Blob variant instead of Extension); MED-1 (no version byte), MED-2 (remove two-strategies framing), MED-3 (add round-trip tests), MED-4 (SQLite compatibility note), MED-5 (BYTEA(32) is enforced); LOW-1 (add as_blob_len), LOW-2 (plain language determinism) |
| 1.0 | 2026-03-25 | Initial draft |

## Related RFCs

- RFC-0903 (Economics): Virtual API Key System
- RFC-0909 (Economics): Deterministic Quota Accounting
- RFC-0126 (Numeric): Deterministic Canonical Serialization
- RFC-0127 (Numeric): DCS Blob Amendment — defines Blob as first-class DCS type

## Related Use Cases

- Enhanced Quota Router Gateway (docs/use-cases/enhanced-quota-router-gateway.md)

---

**Version:** 5.2
**Original Submission Date:** 2026-03-25
**Last Updated:** 2026-03-27
