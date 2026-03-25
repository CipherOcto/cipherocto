# RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage

## Status

Draft (v1)

## Authors

- Author: @cipherocto

## Summary

This RFC adds native binary data type support (BLOB/BYTEA) to stoolap's type system. Binary storage enables efficient cryptographic hash storage (SHA256, HMAC-SHA256) without hex encoding overhead, reducing storage by 50% and enabling deterministic byte-level comparison for economic ledgers.

## Dependencies

**Requires:**

- RFC-0126 (Numeric): Deterministic Canonical Serialization (for blob serialization determinism)

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

Two storage strategies based on blob size:

```rust
// In core/value.rs

/// Blob storage using Extension pattern for memory efficiency
///
/// Extension layout: byte[0] = DataType::Blob as u8, byte[1..] = payload
/// For small blobs (≤30 bytes): inline in CompactArc
/// For larger blobs: heap-allocated Arc<[u8]>
pub type BlobData = crate::common::CompactArc<[u8]>;

/// Blob comparison result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobOrdering {
    Equal,
    Less,
    Greater,
}
```

**Storage Note**: Small blobs (≤30 bytes, including tag byte) fit inline in `CompactArc`'s inline representation, avoiding heap allocation. This covers all common hash sizes (SHA256 = 32 bytes = 33 bytes with tag).

#### ToParam Implementations

```rust
// In api/params.rs

impl ToParam for Vec<u8> {
    fn to_param(&self) -> Value {
        let mut bytes = Vec::with_capacity(1 + self.len());
        bytes.push(DataType::Blob as u8);
        bytes.extend_from_slice(self);
        Value::Extension(CompactArc::from(bytes))
    }
}

impl<const N: usize> ToParam for [u8; N] {
    fn to_param(&self) -> Value {
        let mut bytes = Vec::with_capacity(1 + N);
        bytes.push(DataType::Blob as u8);
        bytes.extend_from_slice(self);
        Value::Extension(CompactArc::from(bytes))
    }
}

impl ToParam for &[u8] {
    fn to_param(&self) -> Value {
        let mut bytes = Vec::with_capacity(1 + self.len());
        bytes.push(DataType::Blob as u8);
        bytes.extend_from_slice(self);
        Value::Extension(CompactArc::from(bytes))
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
    key_hash BLOB NOT NULL,        -- HMAC-SHA256 (32 bytes)
    key_prefix TEXT NOT NULL
);

CREATE TABLE usage_ledger (
    event_id BLOB PRIMARY KEY,     -- SHA256 (32 bytes)
    request_id BLOB NOT NULL,     -- SHA256 (32 bytes)
    pricing_hash BLOB NOT NULL,   -- SHA256 (32 bytes)
    signature BLOB                 -- Variable (Ed25519 = 64 bytes)
);
```

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
    /// Index structure: ahash map from blob bytes → row IDs
    /// This is appropriate because:
    /// - Hash comparison is deterministic (no ordering needed)
    /// - Lookup is O(1) average case
    /// - Blobs are fixed-size for hashes, making hashing fast
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
```

### Accessor Methods

```rust
// In Value impl

impl Value {
    /// Extract blob as Vec<u8>
    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Value::Extension(data) if data.first() == Some(&(DataType::Blob as u8)) => {
                Some(&data[1..])
            }
            _ => None,
        }
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

| Operation | Class | Requirement |
|-----------|-------|--------------|
| Blob equality | A (Protocol Deterministic) | Byte-by-byte comparison, no branching on data values |
| Blob ordering | A (Protocol Deterministic) | Lexicographic by byte index, length as tiebreaker |
| Blob hash (for indexing) | A (Protocol Deterministic) | ahash is deterministic for fixed inputs |

### Serialization Determinism

| Operation | Class | Requirement |
|-----------|-------|--------------|
| Blob → bytes | A (Protocol Deterministic) | Length prefix + data, no padding |
| Bytes → blob | A (Protocol Deterministic) | Strip length prefix, verify length matches |

### No Non-Deterministic Operations

- **Forbidden**: Floating-point operations on blob data
- **Forbidden**: Time-dependent comparison
- **Forbidden**: Random byte ordering

## Security Considerations

### DoS Prevention

| Threat | Mitigation |
|--------|------------|
| Giant blob injection | Maximum blob size limit: 1MB |
| Hash collision attacks | Use ahash with keyed output (future) |
| Memory exhaustion | Blob data stored in Arc, not copied on clone |

### Integrity

| Threat | Mitigation |
|--------|------------|
| Partial read | Length prefix ensures complete read verification |
| Truncation attacks | Store length alongside data in serialization |

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Blob insert | <10µs | No hex encoding, single memcpy |
| Blob lookup | <5µs | Hash index O(1) + single comparison |
| Blob comparison | <1µs | Memcmp of 32 bytes |
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
    let hash = Sha256::digest(input);

    let value: Value = hash.into();  // Uses [u8; 32] → ToParam → Value::Extension
    assert_eq!(value.as_blob_32(), Some(expected));
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
| **Extension (chosen)** | Memory efficient, fits existing pattern | Requires tag byte overhead |
| **Direct Vec<u8>** | Simple | Not niche-optimized, larger Value size |
| **Separate Blob variant** | Explicit | Duplicates Extension logic, breaks Value enum compactness |
| **TEXT + hex (current)** | Works today | 2x storage, encoding overhead, non-deterministic comparison |

## Implementation Phases

### Phase 1: Core Blob Type

- [ ] Add `DataType::Blob = 10` to `core/types.rs`
- [ ] Add `FromStr` parsing for BLOB, BYTEA, BINARY, VARBINARY
- [ ] Add `Value::Extension` serialization for BlobData
- [ ] Add `ToParam` for `Vec<u8>`, `[u8; N]`, `&[u8]`
- [ ] Add `Value::as_blob()` and `Value::as_blob_32()` accessors
- [ ] Add blob comparison in `Value::compare_same_type`

### Phase 2: Query Engine Integration

- [ ] Hash index support for Blob columns
- [ ] Blob equality in expression evaluation
- [ ] Blob in projection/selection

### Phase 3: Integration with RFC-0903/0909

- [ ] Update `schema.rs` for api_keys key_hash → BYTEA(32)
- [ ] Update `storage.rs` to use native blob (remove hex::encode/decode)
- [ ] Verify storage reduction with benchmark

## Key Files to Modify

| File | Change |
|------|--------|
| `stoolap/src/core/types.rs` | Add `DataType::Blob = 10` |
| `stoolap/src/core/value.rs` | Add blob accessors, comparison |
| `stoolap/src/api/params.rs` | Add `ToParam` impls for blob types |
| `stoolap/src/executor/expression/` | Blob comparison in VM |
| `stoolap/src/storage/index/hash.rs` | Hash index for Blob columns |
| `crates/quota-router-core/src/schema.rs` | Update key_hash → BYTEA(32) |
| `crates/quota-router-core/src/storage.rs` | Remove hex::encode/decode |

## Rationale

### Why Extension Pattern?

1. **Memory efficiency**: `CompactArc<[u8]>` is 8 bytes (thin pointer), allowing Value to remain 16 bytes via niche optimization
2. **No Value enum growth**: Adding a separate `Blob` variant would add another 8 bytes for the discriminant, breaking the 16-byte guarantee
3. **Consistent with Vector/Json**: Extension is already used for complex types with tag-byte structure

### Why Not Direct Vec<u8>?

1. `Vec<u8>` would require separate heap allocation even for small blobs
2. `CompactArc<[u8]>` supports inline storage for blobs ≤30 bytes (including tag)
3. SHA256 hashes (32 bytes + 1 tag = 33 bytes) fit inline

### Why Hash Index Only?

Blob comparison for ordering (>, <, >=, <=) is non-deterministic in practice because:
- Different implementations may have different tie-breaking
- Range scans on binary data are uncommon for hash storage

Therefore, only equality index (Hash) is supported, consistent with how hashes are used in practice.

## Future Work

- F1: Streaming blob I/O for large data (documents, images)
- F2: Blob compression (for large variable-size blobs)
- F3: Partial blob reads (subrange extraction)
- F4: Keyed hashing for DoS-resistant blob indexing

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-25 | Initial draft |

## Related RFCs

- RFC-0903 (Economics): Virtual API Key System
- RFC-0909 (Economics): Deterministic Quota Accounting
- RFC-0126 (Numeric): Deterministic Canonical Serialization

## Related Use Cases

- Enhanced Quota Router Gateway (docs/use-cases/enhanced-quota-router-gateway.md)

---

**Version:** 1.0
**Submission Date:** 2026-03-25
**Last Updated:** 2026-03-25
