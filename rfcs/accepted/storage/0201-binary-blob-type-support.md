# RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage

## Status

Accepted (v5.24)

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

    /// Create a Blob from deserialized wire bytes.
    ///
    /// `CompactArc<[u8]>` heap-allocates and copies input data on construction.
    /// Both `from_slice` and `from_deserialized` perform a single heap allocation
    /// and copy — neither borrows from the input buffer. The benefit of the
    /// deserialization path is a single-copy (no intermediate Vec allocation between
    /// `deserialize_blob` and `Blob`), not zero-copy lifetime extension. The input buffer's
    /// contents are copied into CompactArc storage; the returned Blob owns its data
    /// independently.
    ///
    /// This is the correct constructor for the deserialization path. The dispatcher
    /// receives a slice into the wire buffer (via `deserialize_blob`); calling
    /// `from_deserialized(value)` copies those bytes into CompactArc storage. This is
    /// distinct from `Blob::new()` which is used for the storage path (from
    /// parameters where the Vec is already owned).
    ///
    /// `CompactArc::from(data)` always heap-allocates and copies the input bytes,
    /// guaranteeing the stored data is independent from the wire buffer lifetime.
    /// Implementations MUST NOT store a raw pointer into the wire buffer — doing so
    /// would cause use-after-free when the wire buffer is released.
    /// Alternative ownership types (e.g., Rc, Arc without Copy-on-write) MUST ensure
    /// their allocation is independent from the wire buffer lifetime.
    pub fn from_deserialized(data: &[u8]) -> Self {
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

**Storage Note**: `deserialize_blob` returns a slice into the input buffer — this is the zero-copy DCS layer. At the stoolap application layer, `Blob::from_deserialized` copies the slice into `CompactArc<[u8]>` storage for owned lifetime management. This single copy is unavoidable in Rust's ownership model. The prohibition is against *additional* copies — implementers MUST NOT introduce a second allocation (e.g., an intermediate `Vec<u8>`) between `deserialize_blob`'s returned slice and `Blob::from_deserialized`. `as_bytes()` returns a direct reference to the `CompactArc` data, not a copied `Vec<u8>`. Cloning a Blob does not copy the underlying data — `CompactArc` provides shared ownership.

**Ordering Note**: `Blob` does not derive `Ord` or `PartialOrd`. `Value::compare_blob_same_type` uses `compare_blob` for Blob comparison, not derived ordering. The derived `Ord` on `Value` uses the ordinal position of the `Blob` variant within the enum discriminant, not byte-level comparison.

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

**Note on `BYTEA(32)`:** The `(32)` suffix is a length assertion. Length constraints are parsed via regex `BYTEA\s*\((\d+)\)` during column definition: the integer N is extracted and stored as a `ColumnConstraint::Length(N)` attached to the column. `DataType::Blob` stores no length; the constraint is separate. The constraint is enforced at the SQL preparation layer: INSERT and UPDATE operations with `len != N` MUST return a constraint error before reaching the storage layer. `BINARY(N)` and `VARBINARY(N)` are parsed the same way; `VARBINARY` is stored identically to `BYTEA`.

**Note on NULL representation (normative):** SQL NULL for a BYTEA column is a schema-layer concept. The DCS layer has no NULL type. NULL MUST NOT be represented as zero bytes on disk — DCS deserialization requires at least 4 bytes (the length prefix). stoolap must use a separate null bitmap or column-level null flag, not zero bytes, to represent NULL.

Error code assignment depends on where the zero-byte condition is detected:
- If the struct layer reads 0 bytes at the field boundary (remaining < 4 before field_id): `DCS_INVALID_STRUCT` — the struct cannot even read the field_id
- If the blob layer reads 0 bytes after the field_id is consumed (remaining < 4 for length prefix): `DCS_INVALID_BLOB` — the field is present but truncated

The null bitmap (schema-layer) indicates which fields are NULL; absent fields (no bitmap entry) are non-NULL and MUST be present in the wire. See **Null bitmap format** above.

**Note on ALTER TABLE ADD COLUMN (normative):** ALTER TABLE ADD COLUMN for any BYTEA column (nullable or NOT NULL) is **not supported** until null bitmap integration is complete. The schema validation layer MUST reject `ALTER TABLE ADD COLUMN` with a BYTEA column type with a clear error (e.g., "BYTEA columns not supported in ALTER TABLE: null bitmap integration is required"). Without null bitmap integration, there is no conformant representation for existing rows lacking the new column's bytes on the wire — attempting to deserialize existing rows after adding a BYTEA column would produce `DCS_INVALID_STRUCT` (missing non-null field). For NOT NULL BYTEA columns, even if existing rows were rewritten with default values, the null bitmap integration is still required to distinguish NOT NULL (present, non-null) from the absent-field representation.

**Note on empty BYTEA (normative):** An empty BYTEA value (`length=0`) serializes to exactly 4 bytes: `u32_be(0)`. It is NOT zero bytes on disk. Deserializing zero bytes as BYTEA returns `DCS_INVALID_BLOB` (fewer than 4 bytes for length prefix).

**Note on SQLite compatibility:** stoolap's SQL parser accepts `USING HASH` for blob index creation (case-insensitive — `USING hash`, `USING Hash`, etc. are all valid). On SQLite, omit `USING HASH` — SQLite's index syntax is implicit.

### Comparison Semantics

```rust
// In Value::compare_blob_same_type

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
    /// from a database master key via HKDF-SHA256 per RFC 5869:
    ///   `DK = HKDF-Extract(salt=db_identifier, IKM=master_key) || HKDF-Expand(info="stoolap-siphash-v1", DK=DK, len=16)`,
    ///   where `db_identifier` is a unique per-database instance identifier (UUID, database
    ///   pathname, or registry key) providing MITM-resistance entropy, and the info string
    ///   "stoolap-siphash-v1" binds the derived key to this protocol, or (3) stored in a
    ///   database-wide key registry.
    /// Key rotation and multi-database key isolation are out of scope for this RFC
    /// and should be addressed in a future key management specification.
    ///
    /// **Key loss/corruption recovery:** If the SipHash key cannot be loaded (e.g.,
    /// metadata file deleted or corrupted, HKDF derivation fails, key registry unavailable),
    /// the hash index MUST be marked as corrupted and rebuilt from the underlying data.
    /// The database MUST NOT silently generate a new key — doing so produces different hash
    /// values, silently corrupting all existing index entries and causing blob hash lookups
    /// to return incorrect results. Recovery procedure: (1) detect key load failure on open,
    /// (2) mark hash index as corrupted, (3) rebuild index by scanning all rows and
    /// recomputing hash values with the new key, or (4) refuse to open if rebuild is
    /// not possible (e.g., insufficient space). Automatic key generation without
    /// detection is a critical integrity failure.
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
// For multi-field structs, use dispatch_struct — this single-field example
// is for illustration only and does not handle byte-chaining across fields.
fn deserialize_column_value(input: &[u8], col_type: &ColumnType) -> Result<Value, DcsError> {
    match col_type {
        ColumnType::Text => {
            // String deserialization also requires dispatcher in mixed schemas
            let (value, _remaining) = deserialize_string(input)?;
            Ok(Value::String(value))
        },
        ColumnType::Bytea => {
            // Blob deserialization also requires dispatcher in mixed schemas
            let (value, _remaining) = deserialize_blob(input)?;
            Ok(Value::Blob(Blob::from_deserialized(value)))
        },
    }
}
```

**Ambiguity symmetry (normative — RECIPROCAL):** It is not sufficient for only Blob deserialization to use the dispatcher. When both `BYTEA` and `TEXT` columns exist in a schema, **all** String deserialization must also use the dispatcher. Calling `deserialize_string` directly on bytes that may have been inserted as Blob is non-conformant — on non-UTF-8 payloads (e.g., cryptographic hash bytes), this returns `DCS_INVALID_UTF8`. The UTF-8 validation applied by `deserialize_string` is only correct when the dispatcher has confirmed the bytes are intended as String.

**Typed-context enforcement (normative):** Bare calls to `deserialize_blob` or `deserialize_string` on raw bytes without schema context are **forbidden in production code paths**. The only conformant entry point is through the schema-driven dispatcher. Direct deserialization calls are non-conformant and will produce consensus-divergent results in mixed-type schemas. Test code may call `deserialize_blob` or `deserialize_string` directly only for unit testing the function itself.

**Error code mapping (normative):** stoolap's internal `BlobDeserializeError` variants MUST map exactly to the corresponding DCS error codes at the DCS serialization/deserialization interface:
- `TruncatedInput` → `DCS_INVALID_BLOB`
- `ExceedsMaxSize` → `DCS_BLOB_LENGTH_OVERFLOW`

The `DcsError` type returned by `serialize_blob` is the same opaque error type used by all DCS functions. Per RFC-0127 Change 2 (TRAP-before-serialize principle), `serialize_blob` performs the 4GB overflow check internally and returns `DCS_BLOB_LENGTH_OVERFLOW` rather than a local error type — the function itself is the last line of defense.

```rust
/// Blob deserialization errors (stoolap internal wrapper).
/// The DCS-layer pseudocode uses DCS_INVALID_BLOB and DCS_BLOB_LENGTH_OVERFLOW
/// per RFC-0127 Change 7.
///
/// **Note (normative):** This enum is documentation-only. Per Round 18 CRIT-4 finding,
/// all `BlobDeserializeError` variants are unreachable — the only concrete error path
/// at the DCS layer is `DCS_INVALID_BLOB` (mapped from `TruncatedInput` in practice).
/// Implementations MAY use this enum internally but MUST NOT rely on variant-specific
/// behavior; all variants collapse to `DCS_INVALID_BLOB` at the DCS interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobDeserializeError {
    /// Input too short to contain length prefix (maps to DCS_INVALID_BLOB)
    TruncatedInput { actual_len: usize },
    /// Blob declared length exceeds DCS maximum (4GB) — maps to DCS_BLOB_LENGTH_OVERFLOW
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

/// Serialize a dynamic array (Dvec). Per RFC-0127 Change 2.5:
/// u32_be(count) || [serialize_elem(elem) for each element]
fn serialize_dvec(elems: &[Value], elem_type: &ColumnType) -> Result<Vec<u8>, DcsError> {
    // Guard against count overflow when casting usize → u32.
    // Note: The element count is not bounded at serialize time (only this u32::MAX guard).
    // Dvec is variable-length; the operative bound is MAX_CONTAINER_ELEMENTS (10M),
    // enforced at deserialization time in deserialize_dvec. A validated schema can produce
    // wire data that deserialize_dvec would reject if count exceeds MAX_CONTAINER_ELEMENTS.
    // RFC-0127 defines no DCS_DVEC_LENGTH_OVERFLOW; DCS_INVALID_STRUCT is used.
    if elems.len() > u32::MAX as usize {
        return Err(DCS_INVALID_STRUCT);  // count exceeds u32::MAX
    }
    // Element-type validation: each element is passed to serialize_value(elem, elem_type),
    // which returns DCS_INVALID_STRUCT on type mismatch. No partial serialization occurs —
    // Vec allocation happens before any serialize_value calls, and serialize_value returns
    // an error (not Ok) on the first mismatched element, leaving the result Vec unchanged.
    let mut result = Vec::new();
    result.extend_from_slice(&(elems.len() as u32).to_be_bytes());
    for elem in elems {
        let serialized = serialize_value(elem, elem_type)?;
        result.extend_from_slice(&serialized);
    }
    Ok(result)
}

/// Serialize a dynamic matrix (Dmat). Per RFC-0127 Change 2.5:
/// u32_be(rows) || u32_be(cols) || [serialize_elem(elem) for row-major order]
fn serialize_dmat(matrix: &[Vec<Value>], rows: u32, cols: u32, elem_type: &ColumnType) -> Result<Vec<u8>, DcsError> {
    // RFC-0127 defines no DCS_DMAT_DIMENSION_OVERFLOW; DCS_INVALID_STRUCT is used because
    // the wire format cannot represent dimensions that exceed u32::MAX. ColumnType::Dmat
    // stores rows/cols as u32, so the schema cannot hold values > u32::MAX.
    // Validate: actual matrix dimensions must match the schema parameters.
    // Without this check, a mismatched matrix (e.g., 3 rows with rows=2) would write
    // a header claiming 2 rows but 3 rows of data — corrupting the next field on deserialization.
    if matrix.len() != rows as usize {
        return Err(DCS_INVALID_STRUCT);  // matrix row count mismatch
    }
    for row in matrix {
        if row.len() != cols as usize {
            return Err(DCS_INVALID_STRUCT);  // matrix column count mismatch
        }
    }
    let mut result = Vec::new();
    result.extend_from_slice(&rows.to_be_bytes());
    result.extend_from_slice(&cols.to_be_bytes());
    for row in matrix {
        for elem in row {
            let serialized = serialize_value(elem, elem_type)?;
            result.extend_from_slice(&serialized);
        }
    }
    Ok(result)
}

/// Serialize an Enum variant. Per RFC-0127 Change 11:
/// u32_be(variant_id) || serialize_variant_value(value, variant_type)
/// Note: variant_id must match one of the schema's defined variants.
fn serialize_enum(variant_id: u32, value: &Value, variants: &[(u32, ColumnType)]) -> Result<Vec<u8>, DcsError> {
    let variant_type = variants.iter()
        .find(|(id, _)| *id == variant_id)
        .map(|(_, t)| t)
        .ok_or(DCS_INVALID_STRUCT)?;
    let mut result = Vec::new();
    result.extend_from_slice(&variant_id.to_be_bytes());
    result.extend_from_slice(&serialize_value(value, variant_type)?);
    Ok(result)
}

/// Serialize a Value given its ColumnType. Dispatches to the appropriate serializer.
fn serialize_value(value: &Value, col_type: &ColumnType) -> Result<Vec<u8>, DcsError> {
    match (value, col_type) {
        (Value::Blob(blob), ColumnType::Bytea) => serialize_blob(blob.as_bytes()),
        (Value::String(s), ColumnType::Text) => serialize_string(s),
        (Value::Bool(b), ColumnType::Bool) => serialize_bool(*b),
        (Value::I128(i), ColumnType::I128) => serialize_i128(*i),
        (Value::Dqa(s), ColumnType::Dqa) => serialize_dqa(s),
        (Value::Struct(sv), ColumnType::Struct(fields)) => serialize_struct(&sv.fields, fields),
        (Value::Option(None), ColumnType::Option(_)) => Ok(vec![0x00]),
        (Value::Option(Some(v)), ColumnType::Option(inner)) => {
            let mut result = vec![0x01];
            result.extend_from_slice(&serialize_value(v, inner)?);
            Ok(result)
        },
        (Value::Enum(variant_id, v), ColumnType::Enum(variants)) => {
            serialize_enum(*variant_id, v, variants)
        },
        (Value::Dvec(elems), ColumnType::Dvec(elem_type)) => {
            serialize_dvec(elems, elem_type)
        },
        (Value::Dmat(matrix), ColumnType::Dmat { rows, cols, elem_type }) => {
            serialize_dmat(matrix, rows, cols, elem_type)
        },
        // Type mismatch: RFC-0127 does not define DCS_TYPE_MISMATCH.
        // DCS_INVALID_STRUCT is used for structural/conformance errors.
        // A Blob value paired with a non-Bytea column is a type mismatch (not
        // a blob-specific error), so DCS_INVALID_STRUCT is the correct code.
        // Size is irrelevant for type mismatch — even a 1-byte Blob with a Text column is wrong.
        (Value::Blob(_), _) => Err(DCS_INVALID_STRUCT),
        // Deferred types: Dfp and BigInt return explicit errors rather than panicking.
        // todo!() is not used because panicking in production on a DFP/BigInt column crashes
        // the process — a recoverable DCS_INVALID_STRUCT is correct for deferred types.
        (Value::Dfp(_), ColumnType::Dfp) => Err(DCS_INVALID_STRUCT),  // TODO(rfc-0201-phase2e): implement serialize_dfp
        (Value::BigInt(_), ColumnType::BigInt) => Err(DCS_INVALID_STRUCT),  // TODO(rfc-0201-phase2e): implement serialize_bigint
        // Other type mismatches: use DCS_INVALID_STRUCT
        _ => Err(DCS_INVALID_STRUCT),
    }
}

/// Serialize a DCS Struct: field_id || serialized_value for each schema field in order.
/// Per RFC-0127 Change 13: no terminator; struct ends when all schema fields consumed.
fn serialize_struct(fields: &[(u32, Value)], schema: &[(u32, ColumnType)]) -> Result<Vec<u8>, DcsError> {
    // Validate: fields and schema must have the same length
    if fields.len() != schema.len() {
        return Err(DCS_INVALID_STRUCT);  // length mismatch: cannot serialize partial struct
    }
    let mut result = Vec::new();
    for ((schema_id, col_type), (field_id, value)) in schema.iter().zip(fields.iter()) {
        // Validate: field_id in fields must match schema field_id at this position
        if schema_id != field_id {
            return Err(DCS_INVALID_STRUCT);  // field_id mismatch: wire would misalign values
        }
        result.extend_from_slice(&field_id.to_be_bytes());
        result.extend_from_slice(&serialize_value(value, col_type)?);
    }
    Ok(result)
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
/// as UTF-8, validates the bytes are valid UTF-8, and returns `DCS_INVALID_UTF8`
/// if validation fails. It returns `(&str, &[u8])` — decoded string first, remaining
/// bytes second — following the value-first convention used by all underlying
/// deserializers in RFC-0201.

fn deserialize_blob(input: &[u8]) -> Result<(&[u8], &[u8]), DcsError> {
    const LEN_SIZE: usize = 4;

    if input.len() < LEN_SIZE {
        return Err(DCS_INVALID_BLOB);  // truncated: need at least 4 bytes for length prefix
    }
    let length = (u32(input[0]) << 24) | (u32(input[1]) << 16) | (u32(input[2]) << 8) | u32(input[3]);
    // Both insufficient-bytes-for-prefix and length-mismatch return DCS_INVALID_BLOB.
    // RFC-0127 does not distinguish these cases at the error-code level. A caller
    // needing to distinguish "no prefix readable" from "prefix readable but data truncated"
    // must inspect the remaining bytes at the call site. The 4GB overflow check is
    // performed at serialization time (serialize_blob), not here — a wire declaring
    // length=0xFFFFFFFF on a shorter buffer returns DCS_INVALID_BLOB (truncated), not
    // DCS_BLOB_LENGTH_OVERFLOW.
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
    // sha2 0.10+ (via digest::Output_size::U32) — confirm sha2 crate is a direct
    // dependency (not transitive) so version is controllable.
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

    let value: Value = hash.to_param();  // Uses [u8; 32] → ToParam → Value::Blob
    assert_eq!(value.as_blob_32(), Some(expected));

    // Cross-implementation verification: This test uses a SHA256 hash of "hello" as a
    // representative 32-byte non-UTF-8 binary value to confirm the Blob/String dispatcher
    // correctly routes based on schema type, not wire format. Any 32-byte non-UTF-8 value
    // (e.g., SHA256 of any input) serves the same verification purpose.
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
fn test_blob_entry17_string_negative_verification() {
    // RFC-0127 Entry 17: SHA256(b"") as blob payload.
    // This payload is NOT valid UTF-8. Passing it to deserialize_string
    // MUST return Err(DCS_INVALID_UTF8).
    let entry17_bytes = hex::decode("00000020e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap();
    let result = deserialize_string(&entry17_bytes);
    assert!(matches!(result, Err(DCS_INVALID_UTF8)));
}

#[test]
fn test_serialize_struct_rejects_field_id_mismatch() {
    // serialize_struct validates that each field_id in fields matches the schema field_id
    // at the corresponding position. Mismatch at any position → DCS_INVALID_STRUCT.
    let schema = vec![
        (1u32, ColumnType::Text),
        (2u32, ColumnType::Bytea),
    ];
    // field_ids are swapped: (2, "wrong") at position 0, (1, Blob) at position 1
    let fields = vec![
        (2u32, Value::String("wrong".to_string())),
        (1u32, Value::Blob(Blob::new(vec![0xAB]))),
    ];
    let result = serialize_struct(&fields, &schema);
    assert!(matches!(result, Err(DCS_INVALID_STRUCT)), "field_id mismatch must be rejected");
}

#[test]
fn test_serialize_blob_small_blob_accepted() {
    // Unit test: verify a representative small blob succeeds.
    // The 4GB boundary (data.len() > 0xFFFFFFFF → DCS_BLOB_LENGTH_OVERFLOW) requires
    // >= 8GB RAM for integration testing and is documented as a deferred integration test.
    let small_blob = vec![0u8; 10];
    assert!(serialize_blob(&small_blob).is_ok(), "10-byte blob is accepted");
}

#[test]
fn test_struct_serialize_roundtrip() {
    // Round-trip: serialize_struct → dispatch_struct produces same values
    let schema = vec![
        (1u32, ColumnType::Text),
        (2u32, ColumnType::Bytea),
    ];
    let fields = vec![
        (1u32, Value::String("hello".to_string())),
        (2u32, Value::Blob(Blob::new(vec![0xDE, 0xAD]))),
    ];
    let serialized = serialize_struct(&fields, &schema).unwrap();
    let (remaining, sv) = dispatch_struct(&serialized, /* is_top_level = */ true, /* depth = */ 0, &schema).unwrap();
    assert!(remaining.is_empty(), "round-trip must consume all bytes");
    assert_eq!(sv.fields.len(), 2);
    assert_eq!(sv.fields[0].1, Value::String("hello".to_string()));
    assert_eq!(sv.fields[1].1, Value::Blob(Blob::new(vec![0xDE, 0xAD])));
}

#[test]
fn test_option_blob_serialize_roundtrip() {
    // Round-trip: Option<Blob> — None and Some cases
    let schema = vec![
        (1u32, ColumnType::Option(Box::new(ColumnType::Bytea))),
    ];

    // None case
    let fields_none = vec![(1u32, Value::Option(None))];
    let serialized_none = serialize_struct(&fields_none, &schema).unwrap();
    let (remaining_none, sv_none) = dispatch_struct(&serialized_none, true, 0, &schema).unwrap();
    assert!(remaining_none.is_empty());
    assert_eq!(sv_none.fields[0].1, Value::Option(None));

    // Some case
    let fields_some = vec![(1u32, Value::Option(Some(Box::new(Value::Blob(Blob::new(vec![0xAB, 0xCD]))))))];
    let serialized_some = serialize_struct(&fields_some, &schema).unwrap();
    let (remaining_some, sv_some) = dispatch_struct(&serialized_some, true, 0, &schema).unwrap();
    assert!(remaining_some.is_empty());
    match &sv_some.fields[0].1 {
        Value::Option(Some(v)) => {
            match v.as_ref() {
                Value::Blob(b) => assert_eq!(b.as_bytes(), &[0xAB, 0xCD]),
                _ => panic!("expected Blob inside Option"),
            }
        },
        _ => panic!("expected Option::Some"),
    }
}

#[test]
fn test_dvec_blob_serialize_roundtrip() {
    // Round-trip: Dvec<Blob>
    let schema = vec![
        (1u32, ColumnType::Dvec(Box::new(ColumnType::Bytea))),
    ];
    let fields = vec![(
        1u32,
        Value::Dvec(vec![
            Value::Blob(Blob::new(vec![0x11])),
            Value::Blob(Blob::new(vec![0x22, 0x33])),
            Value::Blob(Blob::new(vec![0x44, 0x55, 0x66])),
        ]),
    )];
    let serialized = serialize_struct(&fields, &schema).unwrap();
    let (remaining, sv) = dispatch_struct(&serialized, true, 0, &schema).unwrap();
    assert!(remaining.is_empty());
    match &sv.fields[0].1 {
        Value::Dvec(elems) => {
            assert_eq!(elems.len(), 3);
            assert_eq!(elems[0], Value::Blob(Blob::new(vec![0x11])));
            assert_eq!(elems[1], Value::Blob(Blob::new(vec![0x22, 0x33])));
            assert_eq!(elems[2], Value::Blob(Blob::new(vec![0x44, 0x55, 0x66])));
        },
        _ => panic!("expected Dvec"),
    }
}

#[test]
fn test_dvec_empty_serialize_roundtrip() {
    // Round-trip: empty Dvec<Blob> (count=0, no elements)
    let schema = vec![
        (1u32, ColumnType::Dvec(Box::new(ColumnType::Bytea))),
    ];
    let fields = vec![(
        1u32,
        Value::Dvec(vec![]),  // empty
    )];
    let serialized = serialize_struct(&fields, &schema).unwrap();
    let (remaining, sv) = dispatch_struct(&serialized, true, 0, &schema).unwrap();
    assert!(remaining.is_empty());
    match &sv.fields[0].1 {
        Value::Dvec(elems) => assert_eq!(elems.len(), 0),
        _ => panic!("expected Dvec"),
    }
}
```

### Ordering Tests

```rust
#[test]
fn test_blob_ordering() {
    // Blob values via Blob::new for compare_blob testing
    let a = Blob::new(vec![0x00, 0x01]);
    let b = Blob::new(vec![0x00, 0x02]);
    let c = Blob::new(vec![0x00, 0x01, 0x00]);

    // compare_blob implements byte-by-byte comparison
    assert_eq!(compare_blob(a.as_bytes(), b.as_bytes()), BlobOrdering::Less);   // Byte at index 1: 0x01 < 0x02
    assert_eq!(compare_blob(a.as_bytes(), c.as_bytes()), BlobOrdering::Less);  // Prefix shorter = less
    assert_eq!(compare_blob(b.as_bytes(), c.as_bytes()), BlobOrdering::Greater); // Byte at index 1: 0x02 > 0x01

    // Value::compare_blob_same_type delegates to compare_blob — this is what the query engine uses
    let va = Value::Blob(a.clone());
    let vb = Value::Blob(b.clone());
    let vc = Value::Blob(c.clone());
    assert!(va.compare_blob_same_type(&vb) == BlobOrdering::Less);
    assert!(va.compare_blob_same_type(&vc) == BlobOrdering::Less);
    assert!(vb.compare_blob_same_type(&vc) == BlobOrdering::Greater);
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
fn test_validate_schema_rejects_non_ascending_field_ids() {
    // Non-ascending field_ids must be rejected at schema registration time
    let bad_schema = vec![
        (2u32, ColumnType::Text),  // field_id=2 before field_id=1 — invalid
        (1u32, ColumnType::Bytea),
    ];
    assert!(matches!(validate_schema(&bad_schema), Err(DCS_INVALID_STRUCT)),
        "Non-ascending field_ids must return DCS_INVALID_STRUCT");

    // Duplicate field_ids must also be rejected (not strictly ascending)
    let dup_schema = vec![
        (1u32, ColumnType::Text),
        (1u32, ColumnType::Bytea),  // duplicate field_id=1
    ];
    assert!(matches!(validate_schema(&dup_schema), Err(DCS_INVALID_STRUCT)),
        "Duplicate field_ids must return DCS_INVALID_STRUCT");

    // Valid schema must be accepted
    let good_schema = vec![
        (1u32, ColumnType::Text),
        (3u32, ColumnType::Bytea),  // non-sequential but ascending — valid
        (5u32, ColumnType::Bool),
    ];
    assert!(validate_schema(&good_schema).is_ok(),
        "Non-sequential but ascending field_ids must be accepted");

    // Nested invalid Struct must be caught recursively
    let nested_bad = vec![
        (1u32, ColumnType::Struct(vec![
            (2u32, ColumnType::Text),
            (1u32, ColumnType::Bytea),  // non-ascending inside nested Struct
        ])),
    ];
    assert!(matches!(validate_schema(&nested_bad), Err(DCS_INVALID_STRUCT)),
        "Non-ascending field_ids inside nested Struct must be caught");

    // Enum variants with invalid inner Struct must be caught
    let enum_bad = vec![
        (1u32, ColumnType::Enum(vec![
            (0, ColumnType::Struct(vec![
                (2u32, ColumnType::Text),
                (1u32, ColumnType::Bytea),  // non-ascending
            ])),
        ])),
    ];
    assert!(matches!(validate_schema(&enum_bad), Err(DCS_INVALID_STRUCT)),
        "Non-ascending field_ids inside Enum variant Struct must be caught");

    // Dmat zero-dimension must be rejected at schema registration time
    let zero_row_schema = vec![
        (1u32, ColumnType::Dmat { rows: 0, cols: 4, elem_type: Box::new(ColumnType::Bytea) }),
    ];
    assert!(matches!(validate_schema(&zero_row_schema), Err(DCS_INVALID_STRUCT)),
        "Dmat with rows=0 must be rejected at schema registration time");

    let zero_col_schema = vec![
        (1u32, ColumnType::Dmat { rows: 4, cols: 0, elem_type: Box::new(ColumnType::Bytea) }),
    ];
    assert!(matches!(validate_schema(&zero_col_schema), Err(DCS_INVALID_STRUCT)),
        "Dmat with cols=0 must be rejected at schema registration time");

    // Dmat total element count exceeding MAX_CONTAINER_ELEMENTS must be rejected
    let oversized_schema = vec![
        (1u32, ColumnType::Dmat { rows: MAX_CONTAINER_ELEMENTS / 2 + 1, cols: 2,
                                   elem_type: Box::new(ColumnType::Bytea) }),
    ];
    assert!(matches!(validate_schema(&oversized_schema), Err(DCS_INVALID_STRUCT)),
        "Dmat with rows*cols > MAX_CONTAINER_ELEMENTS must be rejected");
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
    assert!(matches!(result, Err(DCS_TRAILING_BYTES)));
}

#[test]
fn test_dispatch_struct_rejects_field_id_mismatch() {
    // Struct: wire field_id=2 but schema expects field_id=1 at position 0.
    // Per v5.7 for-loop-over-schema: wire_field_id must match expected_schema_field_id
    // at each iteration — ascending/descending order in the wire is not the issue;
    // positional mismatch against the schema is.
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
    // No terminator — v5.7 for-loop ends when schema fields exhausted

    // dispatch_struct with ascending field_id enforcement returns error
    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &schema);
    assert!(matches!(result, Err(DCS_INVALID_STRUCT)));
}

#[test]
fn test_dispatch_struct_empty_struct_exemption() {
    // Empty nested struct: Struct { inner: Struct {} }
    // The empty struct MUST NOT trigger the progress check — it legitimately consumes 0 bytes.
    // With v5.7 for-loop-over-schema: wire is just field_id=1, no terminator, no inner content.
    let inner_schema: Vec<(u32, ColumnType)> = vec![];  // empty struct
    let outer_schema = vec![
        (1u32, ColumnType::Struct(inner_schema.clone())),
    ];
    let mut row = Vec::new();
    row.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1 (Struct)
    // Inner schema is empty [] — dispatch_struct for inner consumes 0 bytes.
    // No inner terminator, no outer terminator — v5.7 for-loop ends when schema fields exhausted.

    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &outer_schema);
    assert!(result.is_ok(), "Empty struct must be accepted, not rejected");
}

#[test]
fn test_dispatch_struct_recursion_depth_limit() {
    // Per RFC-0127 Change 13: depth >= 64 triggers DCS_RECURSION_LIMIT_EXCEEDED.
    // Top-level depth=0; each Struct nesting level adds 1 to depth.
    // For a schema with a Struct field, the inner dispatch_struct call is at depth+1.
    // So: top-level depth=62 → dispatch_struct depth=62 (check 62>=64? No) → dispatch_field → inner dispatch_struct depth=63 (check 63>=64? No) → dispatch_field for Text → deserialize_string at depth=63 → deepest=63, accepted.
    //     top-level depth=63 → dispatch_struct depth=63 (check 63>=64? No) → dispatch_field → inner dispatch_struct depth=64 (check 64>=64? Yes) → REJECTED.
    let schema: Vec<(u32, ColumnType)> = vec![
        (1u32, ColumnType::Struct(vec![
            (1u32, ColumnType::Text),
        ])),
    ];
    // Valid v5.7 wire: outer field_id=1 + serialized inner Struct (field_id=1 + "x")
    let mut inner_wire = Vec::new();
    inner_wire.extend_from_slice(&1u32.to_be_bytes()); // inner field_id = 1
    inner_wire.extend_from_slice(&1u32.to_be_bytes()); // string length = 1
    inner_wire.push(b'x');
    let mut wire = Vec::new();
    wire.extend_from_slice(&1u32.to_be_bytes()); // outer field_id = 1
    wire.extend_from_slice(&inner_wire[..]);     // serialized inner struct

    // depth=64: rejected immediately (64 >= 64)
    let result = dispatch_struct(&wire, /* is_top_level = */ true, /* depth = */ 64, &schema);
    assert!(matches!(result, Err(DCS_RECURSION_LIMIT_EXCEEDED)),
        "depth=64 must be rejected");

    // depth=62: inner Struct call is at depth=63 (62+1=63), below limit — accepted.
    // The deepest frame is depth=63, which is < 64, so this succeeds.
    let result_62 = dispatch_struct(&wire, /* is_top_level = */ true, /* depth = */ 62, &schema);
    assert!(result_62.is_ok(), "depth=62 with Struct field: inner reaches depth=63 (< 64), must be accepted");

    // depth=63: outer Struct call is at depth=63 (< 64, OK) but the inner Struct
    // call (triggered by dispatch_field) reaches depth=64 (63+1=64), rejected.
    let result_63 = dispatch_struct(&wire, /* is_top_level = */ true, /* depth = */ 63, &schema);
    assert!(matches!(result_63, Err(DCS_RECURSION_LIMIT_EXCEEDED)),
        "depth=63 with Struct field: inner Struct call reaches depth=64, must be rejected");
}

#[test]
fn test_text_1mb_limit() {
    // TEXT exceeding 1MB (1,048,576 bytes) must return DCS_STRING_LENGTH_OVERFLOW.
    // Per RFC-0127 Change 8, serialize_string takes &str (UTF-8 validated).
    //
    // INTEGRATION TEST: Requires importing serialize_string and deserialize_string
    // from the RFC-0127 DCS layer. This is not a pure RFC-0201 unit test — it is
    // included here as a conformance marker. Compile and run in the DCS integration
    // test suite, not in the RFC-0201 pseudocode compilation context.
    let oversized: String = std::iter::repeat('x').take(1_048_577).collect(); // 1MB + 1 byte
    let serialized = serialize_string(&oversized);
    let result = deserialize_string(&serialized);
    assert!(matches!(result, Err(DCS_STRING_LENGTH_OVERFLOW)));
}

#[test]
fn test_blob_construction_all_lengths_accepted() {
    // Blob::new accepts any Vec<u8> regardless of length — no length constraint at DCS layer.
    // BYTEA(N) length enforcement is a SQL schema-layer concern: the schema stores
    // N via ColumnConstraint::Length(N), and the SQL INSERT layer must validate that
    // blob.len() == N before calling serialize_blob. This unit test verifies only that
    // Blob construction itself (which copies bytes into CompactArc) works for any length.
    let blob_31 = Blob::new(vec![0u8; 31]);
    let blob_33 = Blob::new(vec![0u8; 33]);
    let blob_32 = Blob::new(vec![0u8; 32]);
    assert_eq!(blob_32.as_bytes().len(), 32);
    assert_eq!(blob_31.as_bytes().len(), 31);
    assert_eq!(blob_33.as_bytes().len(), 33);
    // Serialization succeeds for any length — constraint checked at SQL layer
    assert!(serialize_blob(blob_32.as_bytes()).is_ok());
    assert!(serialize_blob(blob_31.as_bytes()).is_ok());
    assert!(serialize_blob(blob_33.as_bytes()).is_ok());
}

#[test]
fn test_dispatch_struct_rejects_unknown_field_id() {
    // Wire data contains field_id=99 which is not in the schema
    let schema = vec![(1u32, ColumnType::Text)];
    let mut row = Vec::new();
    row.extend_from_slice(&99u32.to_be_bytes()); // field_id = 99 (not in schema)
    row.extend_from_slice(&1u32.to_be_bytes()); // string length = 1
    row.push(b'x');
    // No terminator — v5.7 for-loop ends when schema fields exhausted

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
    // No terminator — v5.7 for-loop ends when all 2 schema fields consumed

    let result = dispatch_struct(&wire, /* is_top_level = */ true, /* depth = */ 0, &schema);
    assert!(result.is_ok(), "Mixed schema dispatch should succeed");
    // The binary bytes in field 2 must NOT trigger UTF-8 validation

    // Swap test: same wire, but field 2 is now TEXT (non-UTF-8 bytes).
    // This mirrors test_blob_entry17_string_negative_verification but exercised
    // through the full dispatch path. The dispatcher MUST route field 2 → deserialize_string,
    // which returns DCS_INVALID_UTF8. This verifies the UTF-8 skip optimization is forbidden:
    // an implementation that bypasses the dispatcher and calls deserialize_string directly
    // on non-UTF-8 bytes would get the same result, but only because the dispatcher correctly
    // identified field 2 as TEXT. A UTF-8 skip optimization that inspects bytes instead of
    // consulting the schema would misroute field 2 as Bytea and succeed incorrectly.
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
fn test_enum_blob_serialize_roundtrip() {
    // Round-trip: Enum variant with Blob value — serialize_enum → dispatch_struct
    let schema = vec![
        (1u32, ColumnType::Enum(vec![
            (0u32, ColumnType::Text),
            (1u32, ColumnType::Bytea),  // Blob variant
        ])),
    ];
    // Serialize: variant 1 (Bytea) with 3-byte blob
    let fields = vec![(
        1u32,
        Value::Enum(1, Box::new(Value::Blob(Blob::new(vec![0xAB, 0xCD, 0xEF])))),
    )];
    let serialized = serialize_struct(&fields, &schema).unwrap();
    // Deserialize
    let (remaining, sv) = dispatch_struct(&serialized, true, 0, &schema).unwrap();
    assert!(remaining.is_empty());
    match &sv.fields[0].1 {
        Value::Enum(variant, v) => {
            assert_eq!(*variant, 1);
            match v.as_ref() {
                Value::Blob(b) => assert_eq!(b.as_bytes(), &[0xAB, 0xCD, 0xEF]),
                _ => panic!("expected Blob inside Enum"),
            }
        },
        _ => panic!("expected Enum"),
    }
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
fn test_nested_struct_with_blob_serialize_roundtrip() {
    // Round-trip: nested struct { inner: { hash: BYTEA } }
    let inner_schema = vec![
        (1u32, ColumnType::Bytea),  // hash field — primary use case
    ];
    let outer_schema = vec![
        (1u32, ColumnType::Struct(inner_schema)),
    ];
    let fields = vec![(
        1u32,
        Value::Struct(StructValue { fields: vec![
            (1u32, Value::Blob(Blob::new(vec![0xDE, 0xAD, 0xBE, 0xEF]))),
        ]}),
    )];
    let serialized = serialize_struct(&fields, &outer_schema).unwrap();
    let (remaining, sv) = dispatch_struct(&serialized, true, 0, &outer_schema).unwrap();
    assert!(remaining.is_empty());
    match &sv.fields[0].1 {
        Value::Struct(sv_outer) => {
            match &sv_outer.fields[0].1 {
                Value::Blob(b) => assert_eq!(b.as_bytes(), &[0xDE, 0xAD, 0xBE, 0xEF]),
                _ => panic!("expected Blob in inner struct"),
            }
        },
        _ => panic!("expected outer Struct"),
    }
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

    // Verify deserialized values match inputs
    let (remaining, sv) = result.unwrap();
    assert!(remaining.is_empty(), "All bytes must be consumed");
    assert_eq!(sv.fields.len(), 3, "Must have exactly 3 fields");

    // Field 1: "name"
    let (_, v1) = &sv.fields[0];
    assert_eq!(v1, &Value::String("name".to_string()));

    // Field 2: 32-byte Blob
    let (_, v2) = &sv.fields[1];
    let expected_blob = Blob::new((0u8..32).collect());
    assert_eq!(v2, &Value::Blob(expected_blob));

    // Field 3: "label1"
    let (_, v3) = &sv.fields[2];
    assert_eq!(v3, &Value::String("label1".to_string()));
}

#[test]
fn test_dispatch_dmat_deserialization() {
    // Dmat: u32(rows) || u32(cols) || [element...], each element routed through dispatcher
    // Test 1: 2x2 matrix of Text values
    let schema = vec![
        (1u32, ColumnType::Dmat { rows: 2, cols: 2, elem_type: Box::new(ColumnType::Text) }),
    ];
    let mut wire = Vec::new();
    wire.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1 (Dmat field)
    wire.extend_from_slice(&2u32.to_be_bytes()); // rows = 2
    wire.extend_from_slice(&2u32.to_be_bytes()); // cols = 2
    // Row 0: "a", "b"
    wire.extend_from_slice(&1u32.to_be_bytes()); // string length = 1
    wire.push(b'a');
    wire.extend_from_slice(&1u32.to_be_bytes()); // string length = 1
    wire.push(b'b');
    // Row 1: "c", "d"
    wire.extend_from_slice(&1u32.to_be_bytes()); // string length = 1
    wire.push(b'c');
    wire.extend_from_slice(&1u32.to_be_bytes()); // string length = 1
    wire.push(b'd');

    let result = dispatch_struct(&wire, true, 0, &schema);
    assert!(result.is_ok(), "2x2 Dmat of Text should deserialize");
    let (remaining, sv) = result.unwrap();
    assert!(remaining.is_empty());
    let (_, v) = &sv.fields[0];
    assert_eq!(v, &Value::Dmat(vec![
        vec![Value::String("a".to_string()), Value::String("b".to_string())],
        vec![Value::String("c".to_string()), Value::String("d".to_string())],
    ]));

    // Test 2: Dimension mismatch — wire says 3x3, schema says 2x2
    let schema_mismatch = vec![
        (1u32, ColumnType::Dmat { rows: 2, cols: 2, elem_type: Box::new(ColumnType::Text) }),
    ];
    let mut wire_mismatch = Vec::new();
    wire_mismatch.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire_mismatch.extend_from_slice(&3u32.to_be_bytes()); // rows = 3 (mismatch)
    wire_mismatch.extend_from_slice(&3u32.to_be_bytes()); // cols = 3 (mismatch)
    // Not providing full 3x3 data is fine — we only check dimension header mismatch
    let result_mismatch = dispatch_struct(&wire_mismatch, true, 0, &schema_mismatch);
    assert!(matches!(result_mismatch, Err(DCS_INVALID_STRUCT)),
        "Dimension mismatch (wire 3x3 vs schema 2x2) must be rejected");

    // Test 3: 2x2 matrix of Blob elements
    let schema_blob = vec![
        (1u32, ColumnType::Dmat { rows: 2, cols: 1, elem_type: Box::new(ColumnType::Bytea) }),
    ];
    let mut wire_blob = Vec::new();
    wire_blob.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire_blob.extend_from_slice(&2u32.to_be_bytes()); // rows = 2
    wire_blob.extend_from_slice(&1u32.to_be_bytes()); // cols = 1
    // Row 0: 3-byte blob
    wire_blob.extend_from_slice(&3u32.to_be_bytes());
    wire_blob.extend_from_slice(b"abc");
    // Row 1: 2-byte blob
    wire_blob.extend_from_slice(&2u32.to_be_bytes());
    wire_blob.extend_from_slice(b"xy");

    let result_blob = dispatch_struct(&wire_blob, true, 0, &schema_blob);
    assert!(result_blob.is_ok(), "2x1 Dmat of Blob should deserialize");
    let (remaining_blob, sv_blob) = result_blob.unwrap();
    assert!(remaining_blob.is_empty(), "all wire bytes must be consumed");
    let (_, v_blob) = &sv_blob.fields[0];
    assert_eq!(v_blob, &Value::Dmat(vec![
        vec![Value::Blob(Blob::new(b"abc".to_vec()))],
        vec![Value::Blob(Blob::new(b"xy".to_vec()))],
    ]));
}

#[test]
fn test_dispatch_dmat_depth_propagation() {
    // Depth is NOT incremented for Dmat container (not a Struct frame).
    // Elements inside the Dmat receive the same depth as the Dmat itself.
    // This test verifies depth propagation by using a Dmat<Struct> at depth=0
    // and confirming it succeeds (inner Struct is at depth=1, well below limit).
    let schema = vec![
        (1u32, ColumnType::Dmat { rows: 1, cols: 1, elem_type: Box::new(ColumnType::Struct(vec![
            (1u32, ColumnType::Text),
        ])) }),
    ];
    // Wire: field_id=1 (Dmat field) + Dmat header (1 row × 1 col) + inner Struct (field_id=1 + "x")
    let mut inner_struct = Vec::new();
    inner_struct.extend_from_slice(&1u32.to_be_bytes()); // inner field_id = 1
    inner_struct.extend_from_slice(&1u32.to_be_bytes()); // string length = 1
    inner_struct.push(b'x');
    let mut wire = Vec::new();
    wire.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1 (outer Dmat field)
    wire.extend_from_slice(&1u32.to_be_bytes()); // rows = 1
    wire.extend_from_slice(&1u32.to_be_bytes()); // cols = 1
    wire.extend_from_slice(&inner_struct[..]);   // 1×1 Struct element

    let result = dispatch_struct(&wire, /* is_top_level = */ true, /* depth = */ 0, &schema);
    assert!(result.is_ok(), "Dmat<Struct> at depth=0: inner Struct at depth=1 should be accepted");
    let (_, sv) = result.unwrap();
    let (_, v) = &sv.fields[0];
    match v {
        Value::Dmat(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].len(), 1);
            match &rows[0][0] {
                Value::Struct(sv_inner) => assert_eq!(sv_inner.fields[0].1, Value::String("x".into())),
                _ => panic!("expected Struct inside Dmat"),
            }
        },
        _ => panic!("expected Dmat"),
    }
}

#[test]
fn test_dispatch_null_bitmap_interaction() {
    // The null bitmap is a schema-layer construct. The DCS dispatcher itself has no
    // null type — NULL fields are absent from the wire. The schema-layer bitmap
    // tells the dispatcher which schema fields are present vs NULL (absent).
    // This test verifies the dispatcher's behavior with wire data that omits fields.
    //
    // Schema: (1, Text), (2, Bytea), (3, Text) — fields 1 and 3 required, field 2 nullable
    // Wire with field 2 NULL (absent from wire): field 1 data + field 3 data (no field 2)
    let schema = vec![
        (1u32, ColumnType::Text),
        (2u32, ColumnType::Bytea),  // nullable
        (3u32, ColumnType::Text),
    ];
    // Wire: field 1 ("hello") + field 3 ("world") — field 2 is absent (NULL)
    let mut wire = Vec::new();
    wire.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire.extend_from_slice(&5u32.to_be_bytes()); // string length = 5
    wire.extend_from_slice(b"hello");
    wire.extend_from_slice(&3u32.to_be_bytes()); // field_id = 3
    wire.extend_from_slice(&5u32.to_be_bytes()); // string length = 5
    wire.extend_from_slice(b"world");
    // field_id=2 (Bytea) is absent — represents NULL. This test documents an
    // **unsupported** scenario: at the DCS layer, non-null fields MUST NOT be omitted
    // from the wire without a null bitmap. The schema-layer bitmap handles null tracking;
    // omitting a non-null field at the DCS layer is a structural error (DCS_INVALID_STRUCT).
    // DCS-layer behavior: if a non-null field is missing from wire (no null bitmap),
    // the dispatcher returns DCS_INVALID_STRUCT (field_id mismatch).
    let result = dispatch_struct(&wire, true, 0, &schema);
    // Unsupported: DCS layer has no null bitmap — non-null field omission → DCS_INVALID_STRUCT
    assert!(matches!(result, Err(DCS_INVALID_STRUCT)),
        "Missing non-null field (no bitmap) must return DCS_INVALID_STRUCT");

    // Test: NULL field correctly absent from wire (simulate with null bitmap awareness)
    // For this test, we use a schema where all fields are present in wire.
    // The null bitmap interaction is at the row-storage layer, not the DCS dispatcher.
    // Verify: valid wire with all fields present succeeds
    let mut wire_all_present = Vec::new();
    wire_all_present.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1
    wire_all_present.extend_from_slice(&5u32.to_be_bytes()); // string length = 5
    wire_all_present.extend_from_slice(b"hello");
    wire_all_present.extend_from_slice(&2u32.to_be_bytes()); // field_id = 2
    wire_all_present.extend_from_slice(&3u32.to_be_bytes()); // blob length = 3
    wire_all_present.extend_from_slice(b"xyz");
    wire_all_present.extend_from_slice(&3u32.to_be_bytes()); // field_id = 3
    wire_all_present.extend_from_slice(&5u32.to_be_bytes()); // string length = 5
    wire_all_present.extend_from_slice(b"world");

    let result_all = dispatch_struct(&wire_all_present, true, 0, &schema);
    assert!(result_all.is_ok(), "All fields present should deserialize successfully");
    let (_, sv) = result_all.unwrap().1;
    assert_eq!(sv.fields.len(), 3);
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
- [ ] Add blob comparison in `Value::compare_blob_same_type`
- [ ] Add `serialize_blob()` and `deserialize_blob()` functions
- [ ] Add `validate_schema()` function called at `CREATE TABLE` time; returns `DCS_INVALID_STRUCT` for non-ascending field_ids. Must be called for all schemas (static and dynamic) before use with `dispatch_struct`.
- [ ] **4GB boundary integration test** — verify `serialize_blob` returns `DCS_BLOB_LENGTH_OVERFLOW` when `data.len() > 0xFFFFFFFF`. This requires a system with >= 8GB RAM or a mock-based test that simulates the boundary without allocating 8GB. The test is a Phase 1 acceptance criterion: it must pass before Blob conformance is claimed. All other DCS serializers return `Vec<u8>` directly; Blob is the first to return `Result`. The insert path must handle both `Ok(bytes)` (proceed with insert) and `Err(DCS_BLOB_LENGTH_OVERFLOW)` (reject the insert with a length error). The error MUST be propagated to the SQL caller, not silently discarded.
- [ ] **Audit `serialize_bytes` call sites** to ensure no Blob-typed data bypasses `serialize_blob`. `serialize_bytes` is a low-level primitive; `serialize_blob` is the public typed entry point.
- [ ] **Reserve `NUMERIC_SPEC_VERSION = 2`** — do not claim conformance until Dfp and BigInt are implemented (Phase 2f). Phase 1 implementations MUST NOT advertise `NUMERIC_SPEC_VERSION >= 2`. Dfp and BigInt are deferred via explicit `Err(DCS_INVALID_STRUCT)` returns rather than `todo!()` panicking, allowing schema evolution without process crashes. Coordinate with RFC-0110 governance: the version increment from 1 to 2 requires minimum 2-epoch notice before activation per RFC-0110's upgrade procedure. RFC-0110 must be updated to include Blob in the spec version table, or a separate governance RFC must specify the activation.
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

Dynamic schemas (SQL CREATE TABLE) MUST be validated using `validate_schema()` before being registered. `dispatch_struct` does not perform runtime schema validation in production builds. Validation checks that all column types are known DCS types and that field_ids are strictly ascending. Compile-time schema definitions (e.g., Rust struct types) benefit from compile-time validation and are generally lower risk.

## Future Work

- F1: Streaming blob I/O for large data (documents, images) — per RFC-0127 Change 8 (Blob Deserialization, Streaming and chunking note), implementations SHOULD support streaming decode for Blobs larger than a configurable memory threshold (e.g., > 1MB) to prevent full payload allocation.
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

- `is_top_level = true` enables the trailing-bytes check: if bytes remain after consuming all expected struct fields, the deserializer returns `DCS_TRAILING_BYTES`
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
    assert!(matches!(result, Err(DCS_TRAILING_BYTES)));
}
```

## Row Storage Format (normative — DCS Struct encoding required)

stoolap rows MUST be stored using DCS Struct encoding. This is not optional because:

1. The DCS serialization layer (`serialize_struct`/`deserialize_struct`) is the only conformant way to handle the length-prefixed byte-chaining required for mixed-type rows containing Blobs
2. A custom row format that does not use DCS Struct encoding would require the storage engine to reimplement byte-chaining, null handling, and field ordering — introducing non-determinism

**Required format:**
- Each field: `u32_be(field_id) || serialized_value` in **strictly ascending field_id order**
- End of struct: no terminator byte — loop ends when all schema fields are consumed (per RFC-0127 Change 13)
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

**Null bitmap integration (deferred — known gap):** The null bitmap format is fully specified above, but the integration between the row-storage layer and the DCS `dispatch_struct` is deferred to a future RFC. Specifically, `dispatch_struct` as specified in Phase 2d assumes all schema fields are present in the wire — it does not currently accept a null bitmap parameter. The row-storage layer must handle null bitmap parsing and the dispatcher must be extended to receive null bitmap context.

**Conformance implication:** Nullable BYTEA columns are **not supported** in the initial RFC-0201 implementation. The schema validation layer MUST reject `CREATE TABLE` with a nullable BYTEA column at table-creation time with a clear error (e.g., "nullable BYTEA not supported: use NOT NULL or defer schema until null bitmap integration is complete"). Without this validation, queries on tables with nullable BYTEA columns would produce `DCS_INVALID_STRUCT` at deserialization time, which is an unclear error for users.

**Wire format example:**
```rust
u32_be(1) || u32_be(1) || 'a'    // field_id=1: TEXT "a"
u32_be(2) || u32_be(5) || bytes  // field_id=2: BYTEA 5-bytes
// No terminator — v5.7 for-loop ends when schema fields exhausted
```

**Conformance test:**
```rust
#[test]
fn test_row_struct_encoding_ascending_field_id() {
    // Row with fields in wrong (non-ascending) order must be rejected.
    // Correct: field 1 then field 2. Wrong: field 2 then field 1.
    // Per RFC-0127 Change 13: no terminator byte; loop ends when schema fields exhausted.
    let schema = vec![
        (1u32, ColumnType::Text),
        (2u32, ColumnType::Text),
    ];
    let mut row = Vec::new();
    row.extend_from_slice(&2u32.to_be_bytes()); // field_id = 2 (wrong order)
    row.extend_from_slice(&5u32.to_be_bytes());
    row.extend_from_slice(b"hello");
    row.extend_from_slice(&1u32.to_be_bytes()); // field_id = 1 (should precede 2)
    row.extend_from_slice(&1u32.to_be_bytes());
    row.push(b'x');

    // dispatch_struct with positional field_id matching detects out-of-order wire
    let result = dispatch_struct(&row, /* is_top_level = */ true, /* depth = */ 0, &schema);
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
- [ ] Phase 2f: DFP and BigInt Support — implement `serialize_dfp`/`deserialize_dfp` (RFC-0104) and `serialize_bigint`/`deserialize_bigint` (RFC-0110). Both return `Err(DCS_INVALID_STRUCT)` until implemented. After implementation, set `NUMERIC_SPEC_VERSION = 2` per Phase 1 item.

### Phase 2a: Hash Index for Blob Columns

```sql
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash) USING HASH;
```

**Implementation requirements:**
- Hash function: SipHash-2-4 with a 128-bit key generated at database open time
- Index structure: `HashMap<SipHash output, Vec<row_id>>`
- Blob hash key: the full 32-byte (or variable-length) blob content, not a hash of the content
- O(1) average equality lookup
- **Fallback mode (required):** If the hash index cannot be rebuilt after key loss or corruption, the database opens with the hash index disabled. Queries that would use the hash index fall back to full scans. The index is marked as degraded in catalog. This preserves data availability — the table data remains accessible; only the O(1) lookup optimization is disabled.

**Conformance test:**
```rust
#[test]
fn test_hash_index_siphash_determinism() {
    // Two identical blobs with the same SipHash key produce the same hash output.
    // This is the fundamental property that makes the hash index work.
    let blob_a = Blob::new(vec![0xDE, 0xAD, 0xBE, 0xEF]);
    let blob_b = Blob::new(vec![0xDE, 0xAD, 0xBE, 0xEF]);  // identical content
    // Using a fixed test key (in production, key is derived via HKDF-SHA256 from master key)
    let test_key = [0u8; 16];  // all-zero test key
    let hash_a = siphash_2_4(&blob_a.as_bytes(), &test_key);
    let hash_b = siphash_2_4(&blob_b.as_bytes(), &test_key);
    assert_eq!(hash_a, hash_b, "identical blobs must produce identical SipHash-2-4 output");

    // Different blobs produce different hash outputs (collision probability is 2^-64)
    let blob_c = Blob::new(vec![0xDE, 0xAD, 0xBE, 0xEE]);  // last byte differs
    let hash_c = siphash_2_4(&blob_c.as_bytes(), &test_key);
    assert_ne!(hash_a, hash_c, "different blobs must produce different SipHash-2-4 output");
}

#[test]
fn test_hash_index_key_persistence_across_restarts() {
    // The SipHash key must be persisted alongside the index. On database restart,
    // the same key must be loaded to ensure existing index entries remain valid.
    // Simulate: derive key, store it, reload it, verify same blob produces same hash.
    let test_key = [0x0D, 0x0E, 0x0A, 0x0D, 0x0B, 0x0E, 0x0E, 0x0F,
                    0x0D, 0x0E, 0x0A, 0x0D, 0x0B, 0x0E, 0x0E, 0x0F];  // test key
    let blob = Blob::from_slice(b"test blob content");
    let hash_original = siphash_2_4(&blob.as_bytes(), &test_key);
    // Simulate restart: reload key from storage
    let reloaded_key = test_key;  // in implementation, this comes from key store
    let hash_reloaded = siphash_2_4(&blob.as_bytes(), &reloaded_key);
    assert_eq!(hash_original, hash_reloaded,
        "SipHash key must be persistent — same key across restarts produces same hash");
}
```

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

pub enum DcsError {
    DCS_INVALID_BOOL,
    DCS_INVALID_SCALE,
    DCS_NON_CANONICAL,
    DCS_OVERFLOW,
    DCS_INVALID_UTF8,
    DCS_STRING_LENGTH_OVERFLOW,
    DCS_INVALID_STRING,
    DCS_INVALID_BLOB,
    DCS_BLOB_LENGTH_OVERFLOW,
    DCS_INVALID_STRUCT,
    DCS_TRAILING_BYTES,
    DCS_RECURSION_LIMIT_EXCEEDED,
}
use DcsError::*;  // bare DCS_* names in return statements

/// StructValue: the deserialized value of a DCS Struct field.
/// fields: Vec of (field_id, Value) pairs in ascending field_id order.
pub struct StructValue {
    pub fields: Vec<(u32, Value)>,
}

/// Value: stoolap's runtime value type. All DCS types are representable as Value.
/// Per RFC-0127, DCS types include: Bool, I128, Dqa, Text, Blob, Struct, Option,
/// Enum, Dvec, Dmat, Dfp, BigInt. Nullable fields are represented via schema-layer
/// null bitmap, not via a NULL Value variant — Value itself is non-nullable.
pub enum Value {
    Bool(bool),
    I128(i128),
    Dqa(String),  // Pseudocode shorthand only — production MUST use Dqa { value: i64, scale: u8 } per RFC-0105
    String(String),
    Blob(Blob),
    Struct(StructValue),
    Option(Option<Box<Value>>),
    Enum(u32, Box<Value>),  // (variant_id, variant_value)
    /// Dynamic vector: variable-length homogeneous array
    Dvec(Vec<Value>),
    /// Dynamic matrix: 2D homogeneous array
    Dmat(Vec<Vec<Value>>),
    /// Decimal floating point — deferred, serialized as opaque bytes
    Dfp(Vec<u8>),
    /// Arbitrary-precision integer — deferred, serialized as opaque bytes
    BigInt(Vec<u8>),
}

impl Value {
    /// Compare two Blob values. Used for ORDER BY on BYTEA columns.
    /// Panics if the values are not both Blob.
    fn compare_blob_same_type(&self, other: &Value) -> BlobOrdering {
        match (self, other) {
            (Value::Blob(a), Value::Blob(b)) => compare_blob(a.as_bytes(), b.as_bytes()),
            _ => panic!("compare_blob_same_type called on non-Blob values"),
        }
    }
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
    Struct(Vec<(u32, ColumnType)>),  // field_id → type mapping; field_ids MUST be strictly ascending and unique (no duplicates). Non-sequential field_ids (e.g., 1, 3, 5) are valid per RFC-0127 Change 13. Schema validation at table-creation time MUST reject schemas that violate ascending/unique constraints.
    Option(Box<ColumnType>),
    Enum(Vec<(u32, ColumnType)>),     // variant_id → type mapping
    Dvec(Box<ColumnType>),            // element type
    Dmat { rows: u32, cols: u32, elem_type: Box<ColumnType> },  // u32 to match wire format; values must be <= u32::MAX
}
```

**Dispatcher contract (normative):**
1. **Progress check**: After deserializing each field, the remaining bytes MUST differ from `remaining_after_field_id`. If they are equal, the field consumed zero bytes but declared a non-zero length — return `DCS_INVALID_STRUCT`. Exception: `ColumnType::Struct(fields)` where `fields` is empty — an empty Struct legitimately consumes 0 bytes.
2. **Empty-struct exemption**: An empty Struct (`ColumnType::Struct([])`) is valid and MUST NOT trigger the progress check. This is the only permitted zero-byte type.
3. **Recursion depth limit**: If `depth >= 64`, return `DCS_RECURSION_LIMIT_EXCEEDED` per RFC-0127 Change 13. Each `dispatch_struct` call (one per Struct nesting level) increments depth by 1. The `dispatch_struct` guard runs once per frame; `dispatch_field` does not independently check the limit. A nesting depth of 0 (top-level) through 63 (63 nested levels below top) is allowed — 64 total frames.
4. **Trailing bytes**: When `is_top_level = true`, any bytes remaining after all schema fields are consumed MUST return `DCS_TRAILING_BYTES`.
5. **Required types**: The dispatcher MUST handle at minimum: `Bool`, `I128`, `Dqa`, `Text`, `Bytea`, `Struct`, `Option`, `Dvec`, and `Dmat`. `Dfp` and `BigInt` return `Err(DCS_INVALID_STRUCT)` (deferred to Phase 2f). `Enum` is fully specified. `Dfp` and `BigInt` are not required for Blob conformance but MUST be implemented before general availability.

**`dispatch_field` specification (depth: usize to match RFC-0127):**
```rust
fn dispatch_field(input: &[u8], col_type: &ColumnType, depth: usize)
    -> Result<(&[u8], Value), DcsError>
{
    match col_type {
        ColumnType::Text => deserialize_string(input)
            .map(|(v, rem)| (rem, Value::String(v))),
        ColumnType::Bytea => deserialize_blob(input)
            .map(|(v, rem)| (rem, Value::Blob(Blob::from_deserialized(v)))),
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
            deserialize_dmat(input, rows, cols, elem_type, depth)
                .map(|(rem, v)| (rem, Value::Dmat(v)))
        },
        ColumnType::Dfp => Err(DCS_INVALID_STRUCT),  // TODO(rfc-0201-phase2e): implement deserialize_dfp
        ColumnType::BigInt => Err(DCS_INVALID_STRUCT),  // TODO(rfc-0201-phase2e): implement deserialize_bigint
        ColumnType::Enum(variants) => {
            // Enum encoded as u32 variant_id, then variant value.
            // Depth is NOT incremented — Enum is not a Struct frame.
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

**Return order convention (normative):** The underlying deserializers (`deserialize_blob`, `deserialize_string`, `deserialize_bool`, etc.) return `(value, remaining_bytes)` — value first, remaining second. `dispatch_field` returns `(remaining_bytes, value)` — remaining first, value second. The `.map()` closures in `dispatch_field` therefore swap the tuple elements: `(blob_data, remaining)` → `(remaining, Value::Blob(...))` and `(string_val, remaining)` → `(remaining, Value::String(...))`.

**Schema validation (required before dispatch_struct):**
```rust
use std::collections::HashSet;

/// Maximum number of elements in any container type (Dvec count, Dmat rows×cols product,
/// BYTEA[] count) to prevent unbounded allocation from malformed wire data.
/// RFC-0201 targets DFP/Dmat use cases with bounded sizes; 10M elements
/// is ~80MB for 8-byte values, well within memory limits.
const MAX_CONTAINER_ELEMENTS: u32 = 10_000_000;

/// Called once at CREATE TABLE / schema registration time — not at deserialization time.
/// Recursively validates all nested composite types. Returns DCS_INVALID_STRUCT if
/// field_ids are not strictly ascending (uniqueness implied by < ordering).
pub fn validate_schema(schema: &[(u32, ColumnType)]) -> Result<(), DcsError> {
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
        },
        ColumnType::Dvec(elem_type) => validate_col_type(elem_type)?,
        // Note: Dvec element count is not validated here — it is a runtime (wire) value,
        // not a schema value. MAX_CONTAINER_ELEMENTS is enforced at deserialization time
        // in deserialize_dvec. Dmat dimensions are schema-declared, hence validated here.
        ColumnType::Dmat { rows, cols, elem_type } => {
            if *rows == 0 || *cols == 0 {
                return Err(DCS_INVALID_STRUCT); // zero-dimension Dmat invalid at schema level
            }
            // Prevent schemas that would cause massive allocation at deserialization
            // (analogous to MAX_CONTAINER_ELEMENTS guard in deserialize_dvec)
            if (*rows as u64) * (*cols as u64) > MAX_CONTAINER_ELEMENTS as u64 {
                return Err(DCS_INVALID_STRUCT); // total elements exceeds limit
            }
            validate_col_type(elem_type)?;
        },
        ColumnType::Option(inner) => validate_col_type(inner)?,
        _ => {} // primitive types need no recursive validation
    }
    Ok(())
}
```

**`dispatch_struct` specification (matches RFC-0127 Change 13 wire format — also called `deserialize_struct` in RFC-0127):**
```rust
fn dispatch_struct(input: &[u8], is_top_level: bool, depth: usize,
                   schema: &[(u32, ColumnType)])  // schema fields in declaration order
    -> Result<(&[u8], StructValue), DcsError>
{
    if depth >= 64 {
        return Err(DCS_RECURSION_LIMIT_EXCEEDED);
    }

    // Schema validity is enforced at schema registration time via validate_schema().
    // dispatch_struct assumes a valid schema. The debug_assert catches dynamically
    // constructed schemas during development; in production, validate_schema() is required.
    debug_assert!(
        schema.windows(2).all(|w| w[0].0 < w[1].0),
        "dispatch_struct called with non-ascending schema field_ids — call validate_schema() at registration time"
    );

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
        // Zero-byte invariant: empty structs (Struct([])) are the only type that may consume
        // zero bytes per RFC-0127 Change 13. Nullable empty structs are not a special case —
        // the null bitmap is schema-layer; at the DCS layer, an empty Struct is always zero
        // bytes on wire (no field_ids, no null bitmap). This exemption is therefore unconditional.
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
        return Err(DCS_INVALID_STRUCT); // need count prefix — structural framing, not Blob payload
    }
    let count = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    if count > MAX_CONTAINER_ELEMENTS {
        return Err(DCS_INVALID_STRUCT); // excessive element count — prevents unbounded allocation
    }
    let mut remaining = &input[4..];
    let mut elements = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let (blob_data, rem) = deserialize_blob(remaining)?;
        elements.push(Blob::from_deserialized(blob_data));
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
    if count > MAX_CONTAINER_ELEMENTS {
        return Err(DCS_INVALID_STRUCT); // excessive element count — prevents unbounded allocation
    }
    let mut remaining = &input[4..];
    let mut elements = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let (rem_after_elem, elem_value) = dispatch_field(remaining, elem_type, depth)?;
        remaining = rem_after_elem;
        elements.push(elem_value);
    }

    Ok((remaining, elements))
}

fn deserialize_dmat(input: &[u8], schema_rows: u32, schema_cols: u32, elem_type: &ColumnType, depth: usize)
    -> Result<(&[u8], Vec<Vec<Value>>), DcsError>
{
    // Per RFC-0127 Change 2.5: u32_be(rows) || u32_be(cols) || elements...
    if input.len() < 8 {
        return Err(DCS_INVALID_STRUCT);
    }
    let wire_rows = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let wire_cols = u32::from_be_bytes([input[4], input[5], input[6], input[7]]);
    // Wire dimensions MUST match schema dimensions — mismatch indicates corruption
    // or wrong schema was used. DCS_INVALID_STRUCT (not DCS_INVALID_BLOB) is used
    // because the issue is structural (dimension header doesn't match declared schema),
    // not a Blob-specific error. RFC-0127 Change 7 defines no separate DCS_INVALID_DMAT;
    // implementations may define a custom error code but DCS_INVALID_STRUCT is conformant.
    if wire_rows != schema_rows || wire_cols != schema_cols {
        return Err(DCS_INVALID_STRUCT);  // dimension mismatch: wire vs schema
    }
    // Defense-in-depth: validate_col_type rejects zero-dimension Dmat schemas at
    // registration time. This check handles the case where deserialize_dmat is
    // called directly (bypassing validate_schema) or with an unchecked schema.
    // Zero-dimension check: a Dmat with 0 rows or 0 columns is semantically invalid
    // (not a matrix, but a dimensional error at the structural level)
    if schema_rows == 0 || schema_cols == 0 {
        return Err(DCS_INVALID_STRUCT); // zero-dimension Dmat is not a valid matrix
    }
    let mut remaining = &input[8..];
    // Guard against allocation overflow on 32-bit platforms (rows * cols could exceed usize range)
    let rows_usize = schema_rows as usize;
    let cols_usize = schema_cols as usize;
    if rows_usize > 0 && cols_usize > (usize::MAX / rows_usize) {
        // DCS_INVALID_STRUCT is used because this is a dimensional/structural error,
        // not a Blob payload overflow. RFC-0127 Change 7 defines DCS_BLOB_LENGTH_OVERFLOW
        // only for Blob fields exceeding 4GB; using it here would mislead callers.
        return Err(DCS_INVALID_STRUCT); // rows * cols overflows usize — dimensional error
    }
    let mut matrix: Vec<Vec<Value>> = Vec::with_capacity(rows_usize);

    // Schema dimensions are authoritative for iteration (wire dimensions were validated above)
    for _ in 0..rows_usize {
        let mut row: Vec<Value> = Vec::with_capacity(cols_usize);
        for _ in 0..cols_usize {
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
> **Acceptance criteria:** Phase 3 is complete when (1) `storage.rs` uses native `Blob` type instead of `hex::encode/decode`, (2) all `TODO(rfc-0201-phase3)` comments are resolved, (3) a benchmark shows storage reduction ≥ 45% for `key_hash BYTEA(32)` vs hex-encoded TEXT (64 chars + overhead).
>
> **Sequencing:** RFC-0201 Phases 1 and 2 MUST be merged and stable before RFC-0903 and RFC-0909 can be implemented. RFC-0903 and RFC-0909 reach Final status independently of RFC-0201 Phase 3. RFC-0201 Phase 3 (removing `hex::encode/decode` from `storage.rs`) is merged after both RFC-0903 and RFC-0909 have been implemented using stoolap's Phase 1+2 Blob type. This is a sequential dependency, not a circular one: [RFC-0201 Ph1+2] → [RFC-0903, RFC-0909 implemented] → [RFC-0201 Ph3 merged].

- [ ] Update `storage.rs` to use native blob (remove hex::encode/decode) — blocked on stoolap Blob implementation
- [ ] Verify storage reduction with benchmark

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 5.24 | 2026-03-28 | Moved to Accepted status — 26 rounds adversarial review (Rounds 1-26), all issues resolved. Phase 1 (core BYTEA), Phase 2d (dispatcher) fully specified and reviewed. Deferred: Phase 2a/2b/2c/2e (stoolap BYTEA implementation), Phase 2f (DFP/BigInt — blocked on RFC-0104/0110 wire format coordination), Phase 3 (storage.rs migration — blocked on stoolap + RFC-0903/0909). |
| 5.23 | 2026-03-28 | Round 26 adversarial review fixes: CRIT-1 (version history: split corrupted v5.22 row into proper v5.22/v5.21/v5.20 entries — rounds 23/24/25 fixes were concatenated; also restore v5.18 to Round 21 content), HIGH-1 (ALTER TABLE ADD COLUMN BYTEA: add explicit MUST-reject normative text — null bitmap integration required before any BYTEA column addition; not just nullable but also NOT NULL BYTEA since existing rows lack column bytes), MED-1 (serialize_dvec: add clarifying comment noting element count is not bounded at serialize time (only u32::MAX guard), with explicit reference to MAX_CONTAINER_ELEMENTS as the deserialize-time bound). |
| 5.22 | 2026-03-28 | Round 25 adversarial review fixes: HIGH-1 (test_hash_index_key_persistence_across_restarts: change Blob::new to Blob::from_slice — b"..." is &[u8], not Vec<u8>; Blob::new requires Vec<u8>), INFO-1 (validate_col_type Dvec arm: add clarifying comment explaining Dvec count asymmetry with Dmat — Dvec count is wire/runtime, not schema; MAX_CONTAINER_ELEMENTS enforced at deserialize time). |
| 5.21 | 2026-03-28 | Round 24 adversarial review fixes: CRIT-1 (test_dispatch_dmat_depth_propagation: prepend field_id=1 outer prefix to wire — rows=1 and field_id=1 numerically coincided so test passed by accident, not correctness), HIGH-1 (MAX_CONTAINER_ELEMENTS: move constant declaration to Phase 2d before validate_col_type; rename from MAX_DVEC_ELEMENTS to accurately describe all three use sites (Dvec, Dmat, BYTEA[])), MED-1 (test_validate_schema_rejects_non_ascending_field_ids: add test cases for Dmat zero-dimension (rows=0, cols=0) and element-count overflow (rows*cols > MAX_CONTAINER_ELEMENTS) at schema registration time). |
| 5.20 | 2026-03-28 | Round 23 adversarial review fixes: CRIT-1 (test_dispatch_dmat_deserialization: prepend field_id=1 prefix to all three wire vectors — dispatch_struct reads field_id first before Dmat header), HIGH-1 (validate_col_type Dmat arm: add zero-dimension check and total-element-count bound (MAX_CONTAINER_ELEMENTS) at schema registration time), MED-1 (deserialize_bytea_array: add MAX_CONTAINER_ELEMENTS guard before Vec::with_capacity — same bound as deserialize_dvec), MED-2 (Value::Dqa comment: remove misleading "per RFC-0105 struct" — clarify pseudocode shorthand with RFC-0105 {value:i64,scale:u8} requirement), LOW-1 (test_dispatch_dmat_deserialization Test 3: add assert!(remaining_blob.is_empty())), LOW-2 (deserialize_dmat zero-dimension check: add defense-in-depth comment noting validate_col_type is primary enforcement). |
| 5.19 | 2026-03-28 | Round 22 adversarial review fixes: CRIT-1 (deserialize_bytea_array: change DCS_INVALID_BLOB to DCS_INVALID_STRUCT — structural framing error, not Blob payload), CRIT-2 (zero-byte progress check: add zero-dimension invariant comment clarifying only empty Struct may consume zero bytes unconditionally), HIGH-1 (validate_col_type Enum arm: add duplicate variant_id check via HashSet), HIGH-2 (test_dmat_with_blob_elements: fix tuple destructuring — StructValue not tuple — use `let (remaining_blob, sv_blob) = result_blob.unwrap()`), HIGH-3 (deserialize_dvec: add MAX_CONTAINER_ELEMENTS=10M guard before Vec::with_capacity to prevent unbounded allocation), HIGH-4 (deserialize_bytea_array: return order convention documented — value-first from underlying deserializer, unchanged in array wrapper), MED-1 (from_slice vs from_deserialized: add normative note clarifying from_slice for existing byte sources, from_deserialized for deserialization path), MED-2 (test_dispatch_null_bitmap_interaction: rename test to clarify it documents unsupported DCS-layer behavior — non-null field omission requires schema-layer bitmap), MED-3 (deserialize_dmat: add zero-dimension check (rows=0 or cols=0 → DCS_INVALID_STRUCT) alongside existing overflow guard), MED-4 (deserialize_bytea_array: change Blob::from_slice to Blob::from_deserialized in deserialization path), MED-5 (compare_blob: fix algorithm docstring — bytes first, length as tiebreaker; code was correct, docstring was wrong), LOW-1 (BlobDeserializeError: add normative note that enum is documentation-only; all variants collapse to DCS_INVALID_BLOB per Round 18 CRIT-4), LOW-2 (test_blob_sha256_stored_as_blob: add sha2 version requirement note), LOW-3 (Phase 3 acceptance: benchmark threshold ≥ 45% reduction), LOW-4 (Value::Dqa: clarify stored as string per RFC-0105). |
| 5.18 | 2026-03-28 | Round 21 adversarial review fixes: Convention note (HIGH-1): correct return-order convention — underlying deserializers return (value, remaining_bytes); dispatch_field returns (remaining_bytes, value); swap is intentional. deserialize_string doc (HIGH-2): fix "remaining bytes and decoded string" to "decoded string and remaining bytes". validate_schema (MED-1): extend to recursively validate Enum variants, Dvec/Dmat element types, and Option inner types via validate_col_type helper. validate_schema test (MED-2): add test_validate_schema_rejects_non_ascending_field_ids with cases for non-ascending, duplicate, nested Struct in Enum. |
| 5.17 | 2026-03-28 | Round 20 adversarial review fixes: CRIT-1 (dispatch_struct: remove runtime schema validation check; keep debug_assert! only; add validate_schema() at schema registration time; update Schema Validation section from SHOULD to MUST), HIGH-1 (from_deserialized: remove assert_ne! normative instruction and "Enforced in all build modes" claim; replace with ownership-invariant prose), HIGH-2 (add return-order convention note after dispatch_field: underlying deserializers return (value, remaining); dispatch_field returns (remaining, value)), MED-1 (test_blob_sha256_stored_as_blob: change hash.into() to hash.to_param() — Into<Value> not defined for [u8;32]), MED-3 (BYTEA(N) constraint: extend from INSERT-only to INSERT and UPDATE operations), LOW-2 (add missing v5.15 row to version history). |
| 5.16 | 2026-03-28 | Round 19 adversarial review fixes: NEW-HIGH-3 (is_top_level prose: correct DCS_INVALID_STRUCT to DCS_TRAILING_BYTES at lines 1574 and 1824), NEW-HIGH-2 (test_row_struct_encoding_ascending_field_id: replace undefined deserialize_struct call with dispatch_struct(..., &schema)), NEW-CRIT-1 (deserialize_dmat overflow guard: change DCS_BLOB_LENGTH_OVERFLOW to DCS_INVALID_STRUCT — dimensional error, not Blob), CRIT-2 (dispatch_struct schema validation: add debug_assert! for dev builds + clarifying comment distinguishing wire vs schema error), HIGH-2 (Storage Note: replace "MUST NOT copy" with clarified single-copy semantics), HIGH-1 (todo!() panics: replace with Err(DCS_INVALID_STRUCT) in serialize_value and dispatch_field; reserve NUMERIC_SPEC_VERSION=2 for Phase 2e), NEW-MED-1 (test_blob_ordering: use .as_bytes() and BlobOrdering comparisons — compare_blob returns BlobOrdering, not Ordering), NEW-HIGH-1 (Row Storage Format: add missing opening ```rust fence before wire format example), NEW-LOW-1 (F1: change RFC-0127 (Motivation) to RFC-0127 Change 8), NEW-LOW-2 (Phase 3: replace circular dependency note with explicit sequential sequencing statement), NEW-MED-2 (deserialize_column_value: prefix unused remaining with underscore to suppress warning). |
| 5.15 | 2026-03-28 | Round 18 adversarial review fixes: CRIT-1 (test_dispatch_struct_rejects_trailing_bytes, test_row_deserialization_rejects_trailing_garbage: correct assertions from DCS_INVALID_STRUCT to DCS_TRAILING_BYTES per RFC-0127 Change 13), CRIT-4 (BlobDeserializeError: remove ghost LengthMismatch variant — unreachable; single error path maps to DCS_INVALID_BLOB), MED-2 (HKDF-SHA256: clarify RFC 5869 Extract/Expand split; define db_identifier as per-database unique MITM-resistance entropy), MED-6 (deserialize_dmat: add overflow guard before Vec::with_capacity(rows_usize) to prevent usize wraparound on 32-bit platforms). |
| 5.14 | 2026-03-28 | Round 17 adversarial review fixes: CRIT-1 (remove test_blob_deserialize_exceeds_max_size: literal won't compile and contradicts design — no 4GB check at deserialize; 4GB enforced at serialize only), CRIT-2 (test_dispatch_dmat_depth_propagation: fix Dmat syntax to use named fields {rows, cols, elem_type}), CRIT-3 (serialize_value: remove 4GB pre-check from Blob type-mismatch arm — type mismatch returns DCS_INVALID_STRUCT regardless of size), MED-5 (DcsError: replace `pub type DcsError = /* implementation-defined */` with concrete `pub enum DcsError { DCS_INVALID_BOOL, ... DCS_RECURSION_LIMIT_EXCEEDED }` and `use DcsError::*` for bare names in pseudocode), HIGH-3 (add test_enum_blob_serialize_roundtrip), HIGH-4 (add test_nested_struct_with_blob_serialize_roundtrip), LOW-3 (add test_dvec_empty_serialize_roundtrip). |
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

**Version:** 5.24
**Original Submission Date:** 2026-03-25
**Last Updated:** 2026-03-28
