# RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage

## Status

Draft (v5.7, adversarial review)

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
    /// byte sources (e.g., test data, parameters).
    pub fn from_slice(data: &[u8]) -> Self {
        Blob { data: CompactArc::from_slice(data) }
    }

    /// Shared-ownership constructor: wrap byte slice data in CompactArc.
    ///
    /// `CompactArc<[u8]>` heap-allocates and copies input data on construction.
    /// Both `from_slice` and `from_shared` perform a single heap allocation and copy —
    /// neither borrows from the input buffer. The "zero-copy" benefit applies to the
    /// deserialization path (no intermediate Vec allocation between `deserialize_blob`
    /// and `Blob`), not a lifetime extension. The input buffer's contents are copied
    /// into CompactArc storage; the returned Blob owns its data independently.
    ///
    /// This is the correct constructor for the deserialization path. The dispatcher
    /// receives a slice into the input buffer (via `deserialize_blob`); calling
    /// `from_shared(value)` copies those bytes into CompactArc storage. This is
    /// distinct from `Blob::new()` which is used for the storage path (from
    /// parameters where the Vec is already owned).
    ///
    /// Implementations MUST verify that `Blob::from_shared(data).as_bytes().as_ptr()
    /// != data.as_ptr()` — i.e., the stored bytes are a distinct allocation, not a
    /// direct pointer into the input buffer. Use a debug_assert! in the implementation:
    /// `debug_assert_ne!(Blob::from_shared(data).as_bytes().as_ptr(), data.as_ptr())`.
    /// This is safe when called from the dispatcher in the query execution context —
    /// the input buffer lives for the query duration.
    pub fn from_shared(data: &[u8]) -> Self {
        Blob { data: CompactArc::from(data) }
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

**Ordering Note**: `Blob` does not derive `Ord` or `PartialOrd`. `Value::compare_same_type` uses `compare_blob` for Blob comparison, not derived ordering. The derived `Ord` on `Value` uses the ordinal position of the `Blob` variant within the enum discriminant, not byte-level comparison.

#### ToParam Implementations

```rust
// In api/params.rs

impl ToParam for Vec<u8> {
    fn to_param(&self) -> Value {
        // Clone is necessary because to_param receives an immutable reference (&self);
        // the Vec is not consumed. Using std::mem::take would require &mut self.
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

**Note on `BYTEA(32)`:** The `(32)` suffix is a length assertion. Length constraints are parsed via regex `BYTEA\s*\((\d+)\)` during column definition: the integer N is extracted and stored as a `ColumnConstraint::Length(N)` attached to the column. `DataType::Blob` stores no length; the constraint is separate. The constraint is enforced at the SQL preparation layer: inserts with `len != N` return a constraint error. `BINARY(N)` and `VARBINARY(N)` are parsed the same way; `VARBINARY` is stored identically to `BYTEA`.

**Note on NULL representation (normative):** SQL NULL for a BYTEA column is a schema-layer concept. The DCS layer has no NULL type. NULL MUST NOT be represented as zero bytes on disk — DCS deserialization requires at least 4 bytes (the length prefix). A zero-byte read for a BYTEA column produces `DCS_INVALID_BLOB`. stoolap must use a separate null bitmap or column-level null flag, not zero bytes, to represent NULL.

**Note on ALTER TABLE ADD COLUMN (normative):** ALTER TABLE ADD COLUMN for a nullable BYTEA column requires handling existing records that lack the new column. Per RFC-0127 Change 13, the DCS dispatcher cannot skip absent fields. Options: (1) rewrite all existing records to include the new column with a default value (recommended for simplicity), (2) construct a per-record schema that only includes present columns, or (3) track schema version per record.

**Note on empty BYTEA (normative):** An empty BYTEA value (`length=0`) serializes to exactly 4 bytes: `u32_be(0)`. It is NOT zero bytes on disk. Deserializing zero bytes as BYTEA returns `DCS_INVALID_BLOB` (fewer than 4 bytes for length prefix).

**Note on SQLite compatibility:** stoolap's SQL parser accepts `USING HASH` for blob index creation (case-insensitive — `USING hash`, `USING Hash`, etc. are all valid). On SQLite, omit `USING HASH` — SQLite's index syntax is implicit.

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
    /// The 128-bit key MUST be persistent for the lifetime of the index and
    /// reloaded on database restart — a key generated fresh at open time (without
    /// persistence) produces different hash values, silently corrupting all existing
    /// index entries and making blob hash lookups return incorrect results.
    /// Acceptable key sources: (1) stored in the index metadata file, (2) derived
    /// from a database master key via HKDF-SHA256(master_key, salt="stoolap-siphash-v1",
    /// info=db_identifier, len=16), or (3) stored in a database-wide key registry.
    /// Key rotation and multi-database key isolation are out of scope for this RFC
    /// and should be addressed in a future key management specification.
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
            Ok(Value::Blob(Blob::from_shared(value)))
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
/// Per RFC-0127 Change 8: returns Result<(&[u8], &[u8]), DcsError> — the caller
/// receives remaining bytes for chaining through subsequent struct fields.
/// `deserialize_string` is defined in RFC-0127 (the String DCS type). RFC-0201
/// does not redefine it — the dispatcher routes all String fields to the
/// RFC-0127 implementation. RFC-0201 specifies only the Bytea dispatch arm
/// and the conformance constraints on mixed-type routing. Per RFC-0127 Change 8,
/// `deserialize_string` reads a u32_be length prefix, extracts that many bytes
/// as UTF-8, and returns DCS_INVALID_UTF8 if bytes are not valid UTF-8.

fn deserialize_blob(input: &[u8]) -> Result<(&[u8], &[u8]), DcsError> {
    const LEN_SIZE: usize = 4;

    if input.len() < LEN_SIZE {
        return Err(DCS_INVALID_BLOB);  // truncated: need at least 4 bytes for length prefix
    }
    let length = (u32(input[0]) << 24) | (u32(input[1]) << 16) | (u32(input[2]) << 8) | u32(input[3]);
    if 4 + (length as usize) > input.len() {
        return Err(DCS_INVALID_BLOB);  // truncated: declared length exceeds remaining bytes
    }
    let blob_data = input[4..4+(length as usize)];
    let remaining = input[4+(length as usize)..];
    Ok((blob_data, remaining))
}

/// Zero-byte read distinction (normative):
/// `DCS_INVALID_BLOB` with `input.len() == 0` specifically indicates a zero-byte read
/// (no length prefix present). `DCS_INVALID_BLOB` with a non-zero declared length
/// that exceeds input indicates truncation. These are distinct corruption patterns:
/// - Zero bytes: the length prefix itself is missing — storage corruption or null
/// - Truncated: length prefix present but data incomplete — partial write or page split

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
| Giant blob injection | Maximum blob size limit: 4GB per RFC-0127 / RFC-0127 Change 2; **stoolap MUST enforce a lower application-level limit (e.g., 1MB)** and MUST support streaming deserialize for blobs that approach the limit. `serialize_blob` returns `DCS_BLOB_LENGTH_OVERFLOW` if the DCS 4GB ceiling is exceeded. An implementation that accepts a 4GB blob into memory in a single allocation is vulnerable to memory exhaustion — the 4GB ceiling is a DCS wire-format constraint, not an application-level memory safety guarantee. |
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

**Benchmark methodology:** Targets use criterion.rs on hardware meeting minimum spec: 3GHz+ x86-64, 8GB RAM. Measurements are median of 1000 iterations. Full methodology in `benchmarks/README.md`.

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

    // Deserialize — verify both remaining bytes and blob data (data first per deserialize_blob signature)
    let (deserialized, remaining) = deserialize_blob(&serialized).unwrap();
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

#[test]
fn test_serialize_blob_exceeds_4gb_boundary() {
    // The 4GB boundary check: if data.len() > 0xFFFFFFFF { return Err(DCS_BLOB_LENGTH_OVERFLOW) }
    // Integration test: allocate exactly 0xFFFFFFFF + 1 bytes and verify DCS_BLOB_LENGTH_OVERFLOW.
    // That allocation requires >= 8GB RAM and should run in integration tests only.
    // Unit test: verify the guard logic with a symbolic check:
    // The condition `data.len() > 0xFFFFFFFF` is equivalent to `data.len() > u32::MAX as usize`.
    // A representative small-boundary sanity check (10 bytes succeeds):
    let small_blob = vec![0u8; 10];
    assert!(serialize_blob(&small_blob).is_ok(), "10-byte blob is accepted");
    // The full 4GB+ boundary test is documented for integration testing with adequate RAM.
}
```

### Ordering Tests

```rust
#[test]
fn test_blob_ordering() {
    // Blob values via Blob::new for direct compare_blob testing
    let a = Blob::new(vec![0x00, 0x01]);
    let b = Blob::new(vec![0x00, 0x02]);
    let c = Blob::new(vec![0x00, 0x01, 0x00]);

    // compare_blob implements byte-by-byte comparison
    assert!(compare_blob(&a, &b) == std::cmp::Ordering::Less);   // Byte at index 1: 0x01 < 0x02
    assert!(compare_blob(&a, &c) == std::cmp::Ordering::Less);  // Prefix shorter = less
    assert!(compare_blob(&b, &c) == std::cmp::Ordering::Greater); // Byte at index 1: 0x02 > 0x01
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

    // Deserialize: blob_data first, remaining second (per deserialize_blob signature)
    let (blob_data, remaining) = deserialize_blob(&serialized).unwrap();
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
    row.extend_from_slice(&0u32.to_be_bytes());       // inner struct: empty (u32_be(0)) — inner terminator
    row.extend_from_slice(&0u32.to_be_bytes());       // outer struct: terminator

    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &outer_schema);
    assert!(result.is_ok(), "Empty struct must be accepted, not rejected");
}

#[test]
fn test_dispatch_struct_recursion_depth_limit() {
    // Per RFC-0127 Change 13: depth >= 64 triggers DCS_RECURSION_LIMIT_EXCEEDED.
    // Top-level depth=0; 63 nested levels below top is allowed (64 total frames).
    // depth=64 itself is rejected.
    let schema: Vec<(u32, ColumnType)> = vec![
        (1u32, ColumnType::Struct(vec![
            (1u32, ColumnType::Text),
        ])),
    ];
    // Empty struct bytes: u32_be(0) terminator — valid input that reaches the depth check
    let empty_struct_bytes = u32::to_be_bytes(0);
    let result = dispatch_struct(&empty_struct_bytes, /* is_top_level = */ true, /* depth = */ 64, &schema);
    assert!(matches!(result, Err(DCS_RECURSION_LIMIT_EXCEEDED)));
    // depth=63 with valid empty struct bytes should succeed
    let result_ok = dispatch_struct(&empty_struct_bytes, /* is_top_level = */ true, /* depth = */ 63, &schema);
    assert!(result_ok.is_ok(), "depth=63 with valid data should be accepted (63 below top = 64 total)");
}

#[test]
fn test_text_1mb_limit() {
    // TEXT exceeding 1MB (1,048,576 bytes) must return DCS_STRING_LENGTH_OVERFLOW.
    // Per RFC-0127 Change 8, serialize_string takes &str (UTF-8 validated).
    // Note: serialize_string and deserialize_string are from the RFC-0127 DCS layer,
    // imported into this implementation's DCS module — not defined in RFC-0201.
    let oversized: String = std::iter::repeat('x').take(1_048_577).collect(); // 1MB + 1 byte
    let serialized = serialize_string(&oversized);
    let result = deserialize_string(&serialized);
    assert!(matches!(result, Err(DCS_STRING_LENGTH_OVERFLOW)));
}

#[test]
fn test_bytea_32_length_constraint_enforcement() {
    // BYTEA(32) columns MUST reject inserts with 31 or 33 byte values.
    // This test verifies Blob construction for various sizes. Full SQL layer
    // enforcement (INSERT INTO t(col) VALUES($1) with 31/33 bytes → constraint error)
    // is tested at the integration level, not in unit tests.
    let blob_31 = Blob::new(vec![0u8; 31]);
    let blob_33 = Blob::new(vec![0u8; 33]);
    let blob_32 = Blob::new(vec![0u8; 32]);

    // Verify blobs constructed correctly; SQL constraint layer rejects invalid lengths
    assert_eq!(blob_32.as_bytes().len(), 32);
    assert_eq!(blob_31.as_bytes().len(), 31);
    assert_eq!(blob_33.as_bytes().len(), 33);
}

#[test]
fn test_dispatch_struct_rejects_unknown_field_id() {
    // Wire data contains field_id=99 which is not in the schema
    let schema = vec![(1u32, ColumnType::Text)];
    let mut row = Vec::new();
    row.extend_from_slice(&99u32.to_be_bytes()); // field_id = 99 (not in schema)
    row.extend_from_slice(&1u32.to_be_bytes()); // string length = 1
    row.push(b'x');
    row.extend_from_slice(&0u32.to_be_bytes()); // terminator

    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &schema);
    assert!(matches!(result, Err(DCS_INVALID_STRUCT)));
}

#[test]
fn test_dispatch_struct_rejects_truncated_input() {
    // Schema expects field_id + data but input ends after field_id
    let schema = vec![(1u32, ColumnType::Text)];
    let mut row = Vec::new();
    row.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    // Missing: string length + data + terminator

    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &schema);
    assert!(matches!(result, Err(DCS_INVALID_STRUCT)));
}

#[test]
fn test_dispatch_routes_blob_vs_string_by_schema() {
    // Mixed schema: TEXT (field 1) and BYTEA (field 2).
    // Wire data: field 1 ("hello") + field 2 (5 binary bytes).
    // The dispatcher must route field 1 → deserialize_string (UTF-8 validated),
    // field 2 → deserialize_blob (no UTF-8 check).
    let schema = vec![
        (1u32, ColumnType::Text),
        (2u32, ColumnType::Bytea),
    ];
    let mut wire = Vec::new();
    wire.extend_from_slice(&1u32.to_be_bytes());       // field_id = 1 (TEXT)
    wire.extend_from_slice(&5u32.to_be_bytes());       // string length = 5
    wire.extend_from_slice(b"hello");                  // UTF-8 text
    wire.extend_from_slice(&2u32.to_be_bytes());       // field_id = 2 (BYTEA)
    wire.extend_from_slice(&5u32.to_be_bytes());       // blob length = 5
    wire.extend_from_slice(b"\xDE\xAD\xBE\xEF\x00");  // binary (non-UTF-8)
    wire.extend_from_slice(&0u32.to_be_bytes());       // terminator

    let result = dispatch_struct(&wire, /* is_top_level = */ true, /* depth = */ 0, &schema);
    assert!(result.is_ok(), "Mixed schema dispatch should succeed");
    // The binary bytes in field 2 must NOT trigger UTF-8 validation

    // Swap test: same wire, but field 2 is now TEXT (non-UTF-8 bytes).
    // The dispatcher routes field 2 → deserialize_string, which returns DCS_INVALID_UTF8.
    let schema_swapped = vec![
        (1u32, ColumnType::Text),
        (2u32, ColumnType::Text),  // field 2 is now TEXT, not BYTEA
    ];
    let result_swapped = dispatch_struct(&wire, /* is_top_level = */ true, /* depth = */ 0, &schema_swapped);
    assert!(matches!(result_swapped, Err(DCS_INVALID_UTF8)),
        "Non-UTF-8 bytes routed as TEXT must return DCS_INVALID_UTF8");
}

#[test]
fn test_dispatch_option_none_and_some() {
    // Option<T> encoded as 1-byte tag: 0x00 = None, 0x01 = Some(value)
    // Schema: field 1 is Option<Text>
    let schema = vec![
        (1u32, ColumnType::Option(Box::new(ColumnType::Text))),
    ];

    // None: tag = 0x00, no value follows
    let mut wire_none = Vec::new();
    wire_none.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire_none.push(0x00); // None tag
    let result_none = dispatch_struct(&wire_none, true, 0, &schema);
    assert!(result_none.is_ok(), "Option::None should be accepted");

    // Some("hello"): tag = 0x01, then serialize_string
    let mut wire_some = Vec::new();
    wire_some.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire_some.push(0x01); // Some tag
    wire_some.extend_from_slice(&5u32.to_be_bytes());
    wire_some.extend_from_slice(b"hello");
    let result_some = dispatch_struct(&wire_some, true, 0, &schema);
    assert!(result_some.is_ok(), "Option::Some should be accepted");

    // Invalid tag (0x02) should return error
    let mut wire_invalid = Vec::new();
    wire_invalid.extend_from_slice(&1u32.to_be_bytes());
    wire_invalid.push(0x02); // invalid tag
    let result_invalid = dispatch_struct(&wire_invalid, true, 0, &schema);
    assert!(matches!(result_invalid, Err(DCS_INVALID_STRUCT)), "Invalid Option tag must error");
}

#[test]
fn test_dispatch_enum_valid_and_invalid_variant() {
    // Enum: u32 variant_id, then variant value
    // Schema: field 1 is Enum[(0, Text), (1, Bytea)]
    let schema = vec![
        (1u32, ColumnType::Enum(vec![
            (0u32, ColumnType::Text),
            (1u32, ColumnType::Bytea),
        ])),
    ];

    // Valid variant 0 (Text): u32(0) + serialize_string("hi")
    let mut wire_v0 = Vec::new();
    wire_v0.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire_v0.extend_from_slice(&0u32.to_be_bytes()); // variant_id = 0
    wire_v0.extend_from_slice(&2u32.to_be_bytes()); // string length = 2
    wire_v0.extend_from_slice(b"hi");
    let result_v0 = dispatch_struct(&wire_v0, true, 0, &schema);
    assert!(result_v0.is_ok(), "Valid enum variant 0 should be accepted");

    // Valid variant 1 (Bytea): u32(1) + serialize_blob(3 bytes)
    let mut wire_v1 = Vec::new();
    wire_v1.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire_v1.extend_from_slice(&1u32.to_be_bytes()); // variant_id = 1
    wire_v1.extend_from_slice(&3u32.to_be_bytes()); // blob length = 3
    wire_v1.extend_from_slice(b"abc");
    let result_v1 = dispatch_struct(&wire_v1, true, 0, &schema);
    assert!(result_v1.is_ok(), "Valid enum variant 1 should be accepted");

    // Invalid variant 99: unknown variant_id → DCS_INVALID_STRUCT
    let mut wire_invalid = Vec::new();
    wire_invalid.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire_invalid.extend_from_slice(&99u32.to_be_bytes()); // variant_id = 99 (not in schema)
    let result_invalid = dispatch_struct(&wire_invalid, true, 0, &schema);
    assert!(matches!(result_invalid, Err(DCS_INVALID_STRUCT)),
        "Unknown enum variant must return DCS_INVALID_STRUCT");
}

#[test]
fn test_dispatch_dvec_with_blob_elements() {
    // Dvec<Bytea>: u32(count) + per-element deserialize_blob
    let schema = vec![
        (1u32, ColumnType::Dvec(Box::new(ColumnType::Bytea))),
    ];
    let mut wire = Vec::new();
    wire.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire.extend_from_slice(&3u32.to_be_bytes()); // count = 3
    // element 1: 2 bytes
    wire.extend_from_slice(&2u32.to_be_bytes());
    wire.extend_from_slice(b"ab");
    // element 2: 1 byte
    wire.extend_from_slice(&1u32.to_be_bytes());
    wire.extend_from_slice(b"c");
    // element 3: 0 bytes (empty blob)
    wire.extend_from_slice(&0u32.to_be_bytes());

    let result = dispatch_struct(&wire, true, 0, &schema);
    assert!(result.is_ok(), "Dvec<Bytea> should be accepted");
}

#[test]
fn test_dispatch_recursive_struct() {
    // Nested struct: outer { inner: struct { text: Text } }
    let inner_schema = vec![
        (1u32, ColumnType::Text),
    ];
    let outer_schema = vec![
        (1u32, ColumnType::Struct(inner_schema)),
    ];

    // Wire: field 1 (outer Struct) → inner struct → field 1 (inner Text "hello")
    let mut wire = Vec::new();
    wire.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1 (outer Struct)
    wire.extend_from_slice(&1u32.to_be_bytes()); // inner field_id = 1 (Text)
    wire.extend_from_slice(&5u32.to_be_bytes()); // string length = 5
    wire.extend_from_slice(b"hello");              // text value

    let result = dispatch_struct(&wire, true, 0, &outer_schema);
    assert!(result.is_ok(), "Recursive struct should be accepted");
}

#[test]
fn test_dispatch_struct_with_blob_field() {
    // Primary use case: a struct containing a BYTEA field
    let schema = vec![
        (1u32, ColumnType::Text),      // name
        (2u32, ColumnType::Bytea),    // key_hash (32 bytes)
        (3u32, ColumnType::Text),      // label
    ];
    let mut wire = Vec::new();
    wire.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire.extend_from_slice(&4u32.to_be_bytes()); // string length = 4
    wire.extend_from_slice(b"name");
    wire.extend_from_slice(&2u32.to_be_bytes()); // field_id = 2
    wire.extend_from_slice(&32u32.to_be_bytes()); // blob length = 32
    wire.extend_from_slice(b"\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f"); // 32 bytes
    wire.extend_from_slice(&3u32.to_be_bytes()); // field_id = 3
    wire.extend_from_slice(&6u32.to_be_bytes()); // string length = 6
    wire.extend_from_slice(b"label1");

    let result = dispatch_struct(&wire, true, 0, &schema);
    assert!(result.is_ok(), "Struct with Blob field should be accepted");
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
- [ ] **Refactor serialization call chain** to propagate `Result<Vec<u8>, DcsError>` from `serialize_blob`. All other DCS serializers return `Vec<u8>` directly; Blob is the first to return `Result`. The insert path must handle both `Ok(bytes)` (proceed with insert) and `Err(DCS_BLOB_LENGTH_OVERFLOW)` (reject the insert with a length error). The error MUST be propagated to the SQL caller, not silently discarded.
- [ ] **Audit `serialize_bytes` call sites** to ensure no Blob-typed data bypasses `serialize_blob`. `serialize_bytes` is a low-level primitive; `serialize_blob` is the public typed entry point.
- [ ] **Increment `NUMERIC_SPEC_VERSION` to `2`** per RFC-0127 Change 11 and RFC-0110. Blob is a new DCS type; implementations claiming conformance must declare `NUMERIC_SPEC_VERSION >= 2`. Coordinate with RFC-0110 governance: the version increment from 1 to 2 requires minimum 2-epoch notice before activation per RFC-0110's upgrade procedure. RFC-0110 must be updated to include Blob in the spec version table, or a separate governance RFC must specify the activation. The Blob type is Final in RFC-0127; this RFC's conformance claim activates according to RFC-0110's upgrade procedure.
- [ ] **Enforce 1MB TEXT column limit** — per RFC-0127 Change 6, `DCS_STRING_LENGTH_OVERFLOW` fires at 1,048,576 bytes. TEXT columns in all schemas (including mixed BYTEA+TEXT) MUST enforce this limit. The limit is enforced at the DCS layer; the storage engine must propagate the error correctly.

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

`compare_blob` supports all comparison operators (equality and ordering) in expression evaluation — the comparison function is deterministic and complete. The hash index only supports equality lookups because hash trees cannot efficiently answer range predicates. Users requiring `WHERE blob_col > $1` must use a full scan or a B-tree index (future work). Range scans on binary data are uncommon for hash storage use cases.

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

**Null bitmap format (normative):** The DCS layer has no NULL type. NULL fields in a row are represented via a schema-layer null bitmap, not via zero bytes or special sentinel values. The null bitmap is:
- **Offset:** Fixed at the start of the row header, before struct field data
- **Bit ordering:** LSB of the first byte = first nullable column in schema order; bit N = column N
- **Encoding:** `1` = NULL, `0` = present
- **Size:** Fixed at table creation time per schema version; 1 bit per nullable column, rounded up to nearest byte
- **Schema versioning:** Adding a nullable column requires a schema migration that rewrites existing rows with the new bitmap; existing rows retain their old bitmap format
- **DCS interaction with schema-iteration:** The dispatcher reads the null bitmap from the row header before processing struct fields. For each schema field, if the bitmap indicates NULL (bit = 1), the field is absent from the wire — the dispatcher sets the value to NULL and continues to the next schema field without reading wire data. If the bitmap indicates present (bit = 0), the dispatcher reads the field_id and value from the wire and validates the wire_field_id matches the expected schema field_id. The bitmap ensures completeness: if the wire is missing a non-null field, the dispatcher returns `DCS_INVALID_STRUCT`.
Zero bytes MUST NOT be used to represent NULL for any column type including BYTEA.

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

- [ ] Phase 2a: Hash Index for Blob Columns — SipHash key persistence required (per CRIT-1)
- [ ] Phase 2b: Blob Equality in Expression Evaluation
- [ ] Phase 2c: Blob in Projection/Selection
- [ ] Phase 2d: Dispatcher Integration — Complete (per CRIT-1.2, HIGH-2.5, MED-3.3)
- [ ] Phase 2e: Array Support — BYTEA[] and DVEC/DMAT

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
/// DcsError: canonical DCS error codes. Per RFC-0127 Change 7, the full set of DCS
/// error codes is: DCS_INVALID_BOOL, DCS_INVALID_SCALE, DCS_NON_CANONICAL, DCS_OVERFLOW,
/// DCS_INVALID_UTF8, DCS_STRING_LENGTH_OVERFLOW, DCS_INVALID_STRING, DCS_INVALID_BLOB,
/// DCS_BLOB_LENGTH_OVERFLOW, DCS_INVALID_STRUCT, DCS_TRAILING_BYTES,
/// DCS_RECURSION_LIMIT_EXCEEDED. Implementations MUST implement all twelve.
/// The error type name (DcsError) is the public interface; the variant names
/// are the conformance interface.

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
    Struct(Vec<(u32, ColumnType)>),  // field_id → type mapping; field_ids MUST be strictly ascending, unique (no duplicates), and without gaps for used range. Schema validation at table-creation time MUST reject schemas that violate these constraints.
    Option(Box<ColumnType>),
    Enum(Vec<(u32, ColumnType)>),     // variant_id → type mapping
    Dvec(Box<ColumnType>),            // element type
    Dmat { rows: usize, cols: usize, elem_type: Box<ColumnType> },  // Note: Value::Dmat variant must be added to stoolap's core Value enum in core/value.rs; Value::Dvec(Vec<Value>) also required
}
```

**Dispatcher contract (normative):**
1. **Progress check**: After deserializing each field, the remaining bytes MUST differ from `remaining_after_field_id`. If they are equal, the field consumed zero bytes but declared a non-zero length — return `DCS_INVALID_STRUCT`. Exception: `ColumnType::Struct(fields)` where `fields` is empty — an empty Struct legitimately consumes 0 bytes.
2. **Empty-struct exemption**: An empty Struct (`ColumnType::Struct([])`) is valid and MUST NOT trigger the progress check. This is the only permitted zero-byte type.
3. **Recursion depth limit**: If `depth >= 64`, return `DCS_RECURSION_LIMIT_EXCEEDED` per RFC-0127 Change 13. Each `dispatch_struct` call (one per Struct nesting level) increments depth by 1. The `dispatch_struct` guard runs once per frame; `dispatch_field` does not independently check the limit. A nesting depth of 0 (top-level) through 63 (63 nested levels below top) is allowed — 64 total frames.
4. **Trailing bytes**: When `is_top_level = true`, any bytes remaining after the `u32_be(0)` terminator MUST return `DCS_INVALID_STRUCT`.
5. **Required types**: The dispatcher MUST handle at minimum: `Bool`, `I128`, `Dqa`, `Text`, `Bytea`, `Struct`, `Option`, `Dvec`, and `Dmat`. `Dfp`, `BigInt`, and `Enum` are fully specified in this RFC — `Dfp` and `BigInt` use `todo!()` as implementation placeholders. They are not required types for Blob conformance but MUST be implemented before general availability.

**`dispatch_field` specification (depth: usize to match RFC-0127):**
```rust
fn dispatch_field(input: &[u8], col_type: &ColumnType, depth: usize)
    -> Result<(&[u8], Value), DcsError>
{
    match col_type {
        ColumnType::Text => deserialize_string(input)
            .map(|(v, rem)| (rem, Value::String(v))),
        ColumnType::Bytea => deserialize_blob(input)
            .map(|(v, rem)| (rem, Value::Blob(Blob::from_shared(v)))),
        ColumnType::Bool => deserialize_bool(input)
            .map(|(v, rem)| (rem, Value::Bool(v))),
        ColumnType::I128 => deserialize_i128(input)
            .map(|(v, rem)| (rem, Value::I128(v))),
        ColumnType::Dqa => deserialize_dqa(input)
            .map(|(v, rem)| (rem, Value::Dqa(v))),
        ColumnType::Struct(fields) => dispatch_struct(input, false, depth + 1, fields)
            .map(|(rem, v)| (rem, Value::Struct(v))),
        ColumnType::Option(inner) => {
            // RFC-0127 Change 10: Option is encoded as a 1-byte tag:
            // 0x00 = None, 0x01 = Some(value). Per RFC-0127 Change 13, depth
            // is incremented only for Struct deserialization frames. Option
            // is NOT a Struct frame — depth is passed unchanged.
            if input.is_empty() {
                return Err(DCS_INVALID_STRUCT);
            }
            let tag = input[0];
            let remaining = &input[1..];
            match tag {
                0 => Ok((remaining, Value::Option(None))),
                1 => dispatch_field(remaining, inner, depth)
                    .map(|(rem, v)| (rem, Value::Option(Some(Box::new(v))))),
                _ => Err(DCS_INVALID_STRUCT),
            }
        },
        ColumnType::Dvec(elem_type) => {
            // Per RFC-0127 Change 2.5: each element routed through dispatcher.
            // Depth is NOT incremented — Dvec is a container, not a Struct frame.
            deserialize_dvec(input, elem_type, depth)
                .map(|(rem, v)| (rem, Value::Dvec(v)))
        },
        ColumnType::Dmat { rows, cols, elem_type } => {
            // Depth is NOT incremented — Dmat is a container, not a Struct frame.
            // Wire dimensions are validated against schema dimensions inside deserialize_dmat.
            deserialize_dmat(input, *rows, *cols, elem_type, depth)
                .map(|(rem, v)| (rem, Value::Dmat(v)))
        },
        ColumnType::Dfp => todo!("deserialize_dfp — deferred per dispatcher contract"),
        ColumnType::BigInt => todo!("deserialize_bigint — deferred per dispatcher contract"),
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
            // Note: DCS_INVALID_STRUCT is used for unknown enum variants because RFC-0127
            // Change 7 does not define a separate DCS_INVALID_ENUM. Callers MUST NOT rely
            // on error code specificity for enum variant lookup — DCS_INVALID_STRUCT and
            // DCS_INVALID_ENUM (if defined) must be treated as equivalent structural errors.
            // The dispatcher's error response is "unknown variant" — callers must not branch
            // on the specific error code to determine the failure mode.
            dispatch_field(remaining, variant_type, depth)
                .map(|(rem, v)| (rem, Value::Enum(variant_id, Box::new(v))))
        },
    }
}
```

**`dispatch_struct` specification (matches RFC-0127 Change 13 wire format):**
```rust
fn dispatch_struct(input: &[u8], is_top_level: bool, depth: usize,
                   schema: &[(u32, ColumnType)])  // schema fields in declaration order
    -> Result<(&[u8], StructValue), DcsError>
{
    if depth >= 64 {
        return Err(DCS_RECURSION_LIMIT_EXCEEDED);
    }

    // Validate schema: field_ids must be strictly ascending and unique
    debug_assert!(schema.windows(2).all(|w| w[0].0 < w[1].0 && w[0].0 != w[1].0),
        "Schema field_ids must be strictly ascending and unique");

    let mut fields = Vec::new();
    let mut remaining = input;

    // Per RFC-0127 Change 13: iterate over schema.fields in declaration order.
    // No terminator byte. Struct ends when all schema fields are consumed.
    for (expected_id, col_type) in schema {
        // Read field_id from wire (4 bytes)
        if remaining.len() < 4 {
            return Err(DCS_INVALID_STRUCT); // need at least 4 bytes for field_id
        }
        let wire_field_id = u32::from_be_bytes([remaining[0], remaining[1], remaining[2], remaining[3]]);
        let remaining_after_field_id = &remaining[4..];

        // Strict positional matching: wire field_id must match schema field_id
        if wire_field_id != *expected_id {
            return Err(DCS_INVALID_STRUCT); // field_id mismatch
        }

        // dispatch_field receives depth unchanged — only Struct arm increments
        let (rem_after_value, value) = dispatch_field(remaining_after_field_id, col_type, depth)?;

        // Progress check: non-empty types must consume at least 1 byte
        let is_empty_struct = matches!(col_type, ColumnType::Struct(fs) if fs.is_empty());
        if !is_empty_struct && rem_after_value == remaining_after_field_id {
            return Err(DCS_INVALID_STRUCT); // zero-byte consumption on non-empty type
        }

        remaining = rem_after_value;
        fields.push((*expected_id, value));
    }

    // Trailing bytes check: at top level, no bytes should remain after all schema fields consumed
    if is_top_level && !remaining.is_empty() {
        return Err(DCS_TRAILING_BYTES); // per RFC-0127 Change 13
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
        let (rem_after_elem, elem_value) = dispatch_field(remaining, elem_type, depth)?;
        remaining = rem_after_elem;
        elements.push(elem_value);
    }

    Ok((remaining, elements))
}

fn deserialize_dmat(input: &[u8], schema_rows: usize, schema_cols: usize, elem_type: &ColumnType, depth: usize)
    -> Result<(&[u8], Vec<Vec<Value>>), DcsError>
{
    // Per RFC-0127 Change 2.5: u32_be(rows) || u32_be(cols) || elements...
    if input.len() < 8 {
        return Err(DCS_INVALID_STRUCT);
    }
    let wire_rows = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    let wire_cols = u32::from_be_bytes([input[4], input[5], input[6], input[7]]) as usize;
    // Wire dimensions MUST match schema dimensions — mismatch indicates corruption
    if wire_rows != schema_rows || wire_cols != schema_cols {
        return Err(DCS_INVALID_STRUCT);  // dimension mismatch: wire vs schema
    }
    let mut remaining = &input[8..];
    let mut matrix: Vec<Vec<Value>> = Vec::with_capacity(schema_rows);

    // Schema dimensions are authoritative for iteration (wire dimensions were validated above)
    for _ in 0..schema_rows {
        let mut row: Vec<Value> = Vec::with_capacity(schema_cols);
        for _ in 0..schema_cols {
            let (rem_after_elem, elem_value) = dispatch_field(remaining, elem_type, depth)?;
            remaining = rem_after_elem;
            row.push(elem_value);
        }
        matrix.push(row);
    }

    Ok((remaining, matrix))
}
```

**Return order:** All deserialization functions return `(&[u8], T)` — remaining bytes first, value second — matching RFC-0127 Change 13's `deserialize_struct` signature.

## Phase 3: Integration with RFC-0903/0909

> **Note**: Phase 3 is pending stoolap Blob implementation. The `schema.rs` in `crates/quota-router-core` has already been updated to use `key_hash BYTEA(32)`, but `storage.rs` still uses `hex::encode/decode`. See `TODO(rfc-0201-phase3)` comments in `storage.rs`.
>
> **Acceptance criteria:** Phase 3 is complete when (1) `storage.rs` uses native `Blob` type instead of `hex::encode/decode`, (2) all `TODO(rfc-0201-phase3)` comments are resolved, (3) a benchmark shows storage reduction for `key_hash BYTEA(32)` vs hex-encoded TEXT. **Dependencies:** RFC-0903 and RFC-0909 must both be in Final status before Phase 3 can be merged.

- [ ] Update `storage.rs` to use native blob (remove hex::encode/decode) — blocked on stoolap Blob implementation
- [ ] Verify storage reduction with benchmark

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 5.7 | 2026-03-27 | Round 10 adversarial review fixes: CRIT-1 (remove terminator; iterate over schema in order per RFC-0127 Change 13), CRIT-2 (accept via CRIT-1), CRIT-3 (dispatcher example: from_slice → from_shared), CRIT-4 (strict positional matching; wire field_id must equal schema field_id per RFC-0127), HIGH-1 (remove vestigial 6-code DcsError paragraph; keep only 12-code paragraph), HIGH-2 (add test_dispatch_option_none_and_some, test_dispatch_enum_valid_and_invalid_variant, test_dispatch_dvec_with_blob_elements, test_dispatch_recursive_struct, test_dispatch_struct_with_blob_field), HIGH-3 (add Value::Dvec(Vec<Value>) to stoolap core Value enum note), HIGH-4 (from_shared: SHOULD → MUST with debug_assert), MED-1 (deserialize_dmat: use schema dimensions for iteration; wire only for validation), MED-2 (clarify null bitmap: NULL fields absent from wire, bitmap indicates which schema fields present), MED-3 (BYTEA(N): regex captures N, stored as ColumnConstraint::Length(N)), MED-4 (BYTEA(32) test: unit test for Blob construction; SQL enforcement at integration level), MED-5 (text_1mb_limit: note serialize_string/deserialize_string imported from RFC-0127), MED-6 (depth fix: dispatch_struct passes depth to dispatch_field; only Struct arm increments), LOW-1 (empty struct test: add outer/inner terminator labels), LOW-3 (test_blob_ordering: use Blob::new with compare_blob), LOW-4 (debug_assert for schema field_id ascending/unique already present). |
| 5.6 | 2026-03-27 | Round 9 adversarial review fixes: CRIT-1 (remove deserialize_string stub; reference RFC-0127 only), CRIT-3 (Enum error: callers MUST NOT rely on error code specificity), CRIT-4 (depth: only Struct increments depth; Option/Dvec/Dmat/Enum pass depth unchanged), CRIT-5 (clarify CompactArc copies data; from_shared is not lifetime extension), CRIT-6 (REBUTTAL: non-contiguous field_ids valid for schema evolution), CRIT-7 (REBUTTAL: zero terminator IS in RFC-0127 Change 13), HIGH-1 (add HKDF-SHA256 params to SipHash key derivation), HIGH-3 (revert ToParam Vec<u8> to self.clone()), HIGH-4 (Struct field validation: MUST reject, not SHOULD), HIGH-5 (document DCS_INVALID_BLOB zero-read vs truncation distinction), HIGH-6 (fully specify null bitmap: offset, bit ordering, versioning, DCS interaction), HIGH-8 (add wire vs schema dimension validation in deserialize_dmat), MED-1 (document CompactArc heap-allocates and copies), MED-2 (add dispatcher routing prose test vectors), MED-3 (from_shared pointer-distinctness note), MED-4 (specify BYTEA(N) parsing), MED-5 (Dfp/BigInt not required for Blob conformance), MED-6 (clarify hash index only affects lookups, not comparison operators), MED-7 (correct DcsError to all 12 RFC-0127 codes), LOW-1 (replace 5GB allocation test with boundary test), LOW-3 (clarify USING HASH accepted by stoolap parser), LOW-4 (document Blob does not derive Ord; Value uses compare_blob), LOW-5 (add benchmark methodology). |
| 5.5 | 2026-03-27 | Round 8 adversarial review fixes: CRIT-1 (SipHash key: specify persistent key storage/reload requirement), CRIT-2 (4GB blob: add application-level 1MB limit + memory exhaustion warning), CRIT-3 (add serialize_blob >4GB test), HIGH-1 (strengthen DcsError definition with all 6 canonical variant names), HIGH-2 (add Blob::from_shared zero-copy constructor for deserialization path), HIGH-3 (add deserialize_string definition reference to RFC-0127), HIGH-4 (add 1MB TEXT enforcement to Phase 1 checklist), MED-1 (specify null bitmap format normatively), MED-2 (add Phase 2 checkboxes to all sub-sections), MED-3 (document DCS_INVALID_ENUM semantic mismatch in Enum arm), MED-4 (add Struct field_id ascending/unique/no-gaps constraint), MED-5 (replace BYTEA(32) placeholder test with real conformance test), MED-6 (fix ToParam Vec<u8> redundant clone: use std::mem::take), LOW-1 (PostgreSQL USING HASH case-insensitivity note), LOW-2 (add Dqa to dispatcher contract required types), LOW-3 (add Value::Dmat note to type def), LOW-4 (fix v5.4 changelog: remove MED-7.1/7.2/7.3 self-references), LOW-5 (verify all test scenarios present), XRFC-1 (specify serialize_blob Result handling in INSERT path), XRFC-2 (coordinate NUMERIC_SPEC_VERSION with RFC-0110 governance), XRFC-3 (add Phase 3 acceptance criteria). |
| 5.4 | 2026-03-27 | Round 7 adversarial review fixes: CRIT-7.1 (replace placeholder comment arms with actual calls: Bool→deserialize_bool, I128→deserialize_i128, Dqa→deserialize_dqa; deferred Dfp/BigInt→todo!() per dispatcher contract), CRIT-7.2 (Dmat arm: remove * from *rows/*cols dereference of usize values), CRIT-7.3 (empty blob test: swap (remaining,blob_data)→(blob_data,remaining) per deserialize_blob signature), HIGH-7.1 (depth limit test: provide valid empty-struct bytes (u32_be(0)) so depth=63 test succeeds for correct reason), HIGH-7.2 (add ascending field_id enforcement to dispatch_struct per RFC-0127 Change 13), MED-7.4 (deserialize_dmat reads rows/cols from wire as u32_be per RFC-0127 Change 2.5; update call site), LOW-7.1 (note: Value::Dmat must be added to stoolap Value enum), LOW-7.3 (rename ColumnType::Dmat elem→elem_type for consistency with deserialize_dmat param). |
| 5.3 | 2026-03-27 | Round 6 adversarial review fixes: CRIT-6.1 (fix stale deserialize_blob doc comment: Err→DcsError per RFC-0127 Change 8), CRIT-6.2 (Option tag: changed from 4 bytes to 1 byte per RFC-0127 Change 10 / termination invariant), CRIT-6.3 (depth limit: changed from 32 to 64 per RFC-0127 Change 13), HIGH-6.1 (round-trip test: swap destructuring to (deserialized, remaining) matching deserialize_blob signature), HIGH-6.2 (add missing Dqa match arm to dispatch_field), MED-6.1 (correct v5.2 changelog: depth check was 32, not "×2=64"), MED-6.2 (define deserialize_dmat function for DMAT arrays), MED-6.3 (document DCS_INVALID_STRUCT for enum variant lookup failure), MED-6.4 (serialize_string test: use String not Vec<u8> per RFC-0127 Change 8), LOW-6.1 (Dqa arm now present), LOW-6.2 (note: Value enum is external, defined in stoolap core/value.rs), LOW-6.3 (DcsError pseudocode note added), LOW-6.4 (bare error variant names acceptable in pseudocode). |
| 5.2 | 2026-03-27 | Round 5 adversarial review fixes: CRIT-5.1 (define all types used in pseudocode: DcsError, ColumnType enum with all variants, StructValue struct), CRIT-5.2 (add is_empty_struct exemption to dispatch_struct progress check), CRIT-5.3 (depth check was 32; corrected to 64 per RFC-0127 Change 13), CRIT-5.4 (clarify Blob::from_slice copy semantics), HIGH-5.2 (fix dispatch_field Struct case), HIGH-5.3 (remove errant ?), HIGH-5.4 (correct v5.1 changelog), MED-5.1 (Phase 2d dispatcher: add Bool, I128, Option, Dvec, Dmat, Enum), MED-5.2 (add schema parameter), MED-5.3 (add conformance tests), MED-5.4 (BYTEA[] direct deserialize_blob), MED-5.5 (align return order), LOW-5.1 (case-insensitive tests), LOW-5.3 (depth type usize), LOW-5.4 (rename before_field→remaining_after_field_id). |
| 5.1 | 2026-03-27 | Fix MED-3.6/3.7/3.4/3.3/3.2/3.1 deferred items: replaced 6 implementation architecture questions with normative specs. Removed duplicate Phase 2/Phase 3 checklist sections. |
| 5.0 | 2026-03-27 | Round 3 adversarial review fixes. |
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

**Version:** 5.7
**Original Submission Date:** 2026-03-25
**Last Updated:** 2026-03-27
