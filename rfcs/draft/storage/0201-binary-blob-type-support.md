# RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage

## Status

Draft (v2, adversarial review)

## Authors

- Author: @cipherocto

## Summary

This RFC adds native binary data type support (BLOB/BYTEA) to stoolap's type system. Binary storage enables efficient cryptographic hash storage (SHA256, HMAC-SHA256) without hex encoding overhead, reducing storage by 50% and enabling deterministic byte-level comparison for economic ledgers.

## Dependencies

**Informative:**

- RFC-0126 (Numeric): Deterministic Canonical Serialization — Blob serialization follows RFC-0126's methodology (length prefix, big-endian, no padding). See RFC-0126 SectionPart 3 for the framework. A future RFC-0126 amendment should add Blob to the DCS type table (see SectionRFC-0126 Amendment below).

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

    /// Create a new Blob from a byte slice (copies data)
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

**Storage Note**: Blob data is heap-allocated via `CompactArc<[u8]>`. All blobs share the same storage mechanism regardless of size. Cloning a Blob does not copy the underlying data — `CompactArc` provides shared ownership.

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

### Serialization

Blobs serialize with explicit length prefix for determinism:

```rust
/// Blob serialization error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobDeserializeError {
    /// Input too short to contain length prefix
    TruncatedInput { actual_len: usize },
    /// Declared length in prefix does not match actual data length
    LengthMismatch { declared: u32, actual: usize },
    /// Blob exceeds maximum allowed size (1MB)
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
fn serialize_blob(blob: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(4 + blob.len());
    result.extend_from_slice(&(blob.len() as u32).to_be_bytes());
    result.extend_from_slice(blob);
    result
}

/// Deserialize blob from canonical bytes.
///
/// Reads the first 4 bytes as a big-endian u32 length prefix,
/// extracts that many bytes as the blob data, and verifies the result.
///
/// Returns the blob data on success. On failure, returns BlobDeserializeError.
fn deserialize_blob(bytes: &[u8]) -> Result<&[u8], BlobDeserializeError> {
    const LEN_SIZE: usize = 4;

    if bytes.len() < LEN_SIZE {
        return Err(BlobDeserializeError::TruncatedInput { actual_len: bytes.len() });
    }

    let declared_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let data = &bytes[LEN_SIZE..];

    const MAX_BLOB_SIZE: u32 = 1_048_576; // 1MB
    if declared_len > MAX_BLOB_SIZE {
        return Err(BlobDeserializeError::ExceedsMaxSize { declared: declared_len as u32, max: MAX_BLOB_SIZE });
    }

    if data.len() != declared_len {
        return Err(BlobDeserializeError::LengthMismatch { declared: declared_len as u32, actual: data.len() });
    }

    Ok(data)
}
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
| Giant blob injection | Maximum blob size limit: 1MB |
| Hash collision attacks | Use SipHash-2-4 (DoS-resistant hash function) |
| Memory exhaustion | Blob data stored in Arc, not copied on clone |

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
}
```

### Serialization Round-Trip Tests

```rust
#[test]
fn test_blob_serialize_roundtrip() {
    let original: &[u8] = b"\x01\x02\x03\x04\x05";

    // Serialize
    let serialized = serialize_blob(original);
    assert_eq!(&serialized[..4], &(5u32).to_be_bytes());
    assert_eq!(&serialized[4..], original);

    // Deserialize
    let deserialized = deserialize_blob(&serialized).unwrap();
    assert_eq!(deserialized, original);
}

#[test]
fn test_blob_deserialize_truncated() {
    // 3 bytes is not enough for the length prefix
    let result = deserialize_blob(&[0x00, 0x00, 0x01]);
    assert!(matches!(result, Err(BlobDeserializeError::TruncatedInput { .. })));
}

#[test]
fn test_blob_deserialize_length_mismatch() {
    // Length prefix says 10 bytes but only 5 follow
    let mut data = Vec::new();
    data.extend_from_slice(&10u32.to_be_bytes());
    data.extend_from_slice(b"hello");
    let result = deserialize_blob(&data);
    assert!(matches!(result, Err(BlobDeserializeError::LengthMismatch { .. })));
}

#[test]
fn test_blob_deserialize_exceeds_max_size() {
    // Declare 2MB blob
    let mut data = Vec::new();
    data.extend_from_slice(&2_097_152u32.to_be_bytes());
    let result = deserialize_blob(&data);
    assert!(matches!(result, Err(BlobDeserializeError::ExceedsMaxSize { .. })));
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
    assert_eq!("BLOB".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("BYTEA".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("BINARY".parse::<DataType>().unwrap(), DataType::Blob);
    assert_eq!("VARBINARY".parse::<DataType>().unwrap(), DataType::Blob);
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

### Phase 2: Query Engine Integration

- [ ] Hash index support for Blob columns using SipHash-2-4
- [ ] Blob equality in expression evaluation
- [ ] Blob in projection/selection

### Phase 3: Integration with RFC-0903/0909

> **Note**: Phase 3 is pending stoolap Blob implementation. The `schema.rs` in `crates/quota-router-core` has already been updated to use `key_hash BYTEA(32)`, but `storage.rs` still uses `hex::encode/decode`. See `TODO(rfc-0201-phase3)` comments in `storage.rs`.

- [ ] Update `storage.rs` to use native blob (remove hex::encode/decode) — blocked on stoolap Blob implementation
- [ ] Verify storage reduction with benchmark

## Key Files to Modify

| File | Change |
|------|--------|
| `stoolap/src/core/types.rs` | Add `DataType::Blob = 10` |
| `stoolap/src/core/value.rs` | Add `Blob` struct, `BlobOrdering` enum, `Value::Blob` variant, accessors, comparison |
| `stoolap/src/api/params.rs` | Add `ToParam` impls for blob types |
| `stoolap/src/executor/expression/` | Blob comparison in VM |
| `stoolap/src/storage/index/hash.rs` | Hash index for Blob columns (SipHash-2-4) |
| `crates/quota-router-core/src/storage.rs` | TODO(rfc-0201-phase3): remove hex::encode/decode when Blob is available |

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

## Future Work

- F1: Streaming blob I/O for large data (documents, images)
- F2: Blob compression (for large variable-size blobs)
- F3: Partial blob reads (subrange extraction)
- F4: RFC-0126 amendment to add Blob/Bytes to the DCS type table (see SectionRFC-0126 Amendment below)

## RFC-0126 Amendment

RFC-0201 depends on RFC-0126's serialization framework (length prefix, big-endian, no padding). RFC-0126 Part 3's DCS type table currently does not include Blob/Bytes as a primitive type. Before RFC-0201 can advance to Accepted, a **companion RFC** should be created to amend RFC-0126, adding the following entry to the DCS Primitive Type table:

| Type | Format | Size |
|------|--------|------|
| `Blob` | `[length: u32BE][data: bytes]` | variable |

This amendment ensures the Blob serialization format is formally part of the DCS type system. The companion RFC should reference RFC-0201 as the authoritative specification for Blob semantics.

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 2.0 | 2026-03-25 | Adversarial review fixes: CRIT-1 (remove false inline storage claim), CRIT-2 (document Phase 3 pending state), CRIT-3 (RFC-0126 amendment required); HIGH-1 (SipHash instead of ahash), HIGH-2 (fix GenericArray test), HIGH-3 (add deserialize_blob with error type), HIGH-4 (add Blob variant instead of Extension); MED-1 (no version byte), MED-2 (remove two-strategies framing), MED-3 (add round-trip tests), MED-4 (SQLite compatibility note), MED-5 (BYTEA(32) is enforced); LOW-1 (add as_blob_len), LOW-2 (plain language determinism) |
| 1.0 | 2026-03-25 | Initial draft |

## Related RFCs

- RFC-0903 (Economics): Virtual API Key System
- RFC-0909 (Economics): Deterministic Quota Accounting
- RFC-0126 (Numeric): Deterministic Canonical Serialization

## Related Use Cases

- Enhanced Quota Router Gateway (docs/use-cases/enhanced-quota-router-gateway.md)

---

**Version:** 2.0
**Submission Date:** 2026-03-25
**Last Updated:** 2026-03-25
