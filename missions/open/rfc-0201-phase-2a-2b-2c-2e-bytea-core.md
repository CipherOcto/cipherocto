# Mission: RFC-0201 Phase 2a/2b/2c/2e — BYTEA Core Blob Type

## Status

Open

## RFC

RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage

## Dependencies

Implementation dependencies (must complete first):
- `octo-determin` crate available in stoolap (for CompactArc)

## Acceptance Criteria

- [ ] `DataType::Blob` added as variant 10 in `src/core/types.rs`, with `FromStr` parsing for BYTEA/BLOB/BINARY/VARBINARY
- [ ] `Value::Blob(CompactArc<[u8]>)` variant added in `src/core/value.rs` (first-class, NOT Extension)
- [ ] `compare_blob` function implemented: bytes-first comparison, length as tiebreaker; returns `BlobOrdering`
- [ ] `Value::compare` and `Value::Ord` integration for Blob
- [ ] `Value::Blob` serialization (wire tag 12) and deserialization in `src/storage/mvcc/persistence.rs`
- [ ] `SchemaColumn.blob_length: Option<u32>` added for BYTEA(N) length constraint
- [ ] DDL parser updated: `BYTEA`, `BLOB`, `BINARY`, `VARBINARY` → `DataType::Blob` (currently maps to Text)
- [ ] `CREATE TABLE` with BYTEA column rejected with clear error (null bitmap not yet integrated)
- [ ] Hash index (`CREATE INDEX ... USING HASH ON blob_column`) functional for equality lookups
- [ ] `cargo test` passes including new Blob tests
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes

## Description

Implement the core BYTEA/BLOB type in stoolap per RFC-0201. This is Phase 2a (hash index), 2b (equality evaluation), 2c (projection/selection), and 2e (BYTEA[] array support) of the RFC.

## Technical Details

### 1. DataType (`src/core/types.rs`)

```rust
/// Binary large object for cryptographic hashes and binary data
Blob = 10,
```

`FromStr`: `"BYTEA" | "BLOB" | "BINARY" | "VARBINARY" => Ok(DataType::Blob)`

`is_orderable`: Blob IS orderable (add to the not-orderable exclusion list alongside Json/Vector)

### 2. Value::Blob (`src/core/value.rs`)

```rust
/// Binary large object — stored as CompactArc<[u8]> for zero-copy sharing.
/// INVARIANT: The Arc is always heap-allocated; there is no inline/blob case.
Blob(CompactArc<[u8]>),
```

**Constructors:**
```rust
pub fn blob(data: &[u8]) -> Self          // from slice — copies into CompactArc
pub fn blob_from_vec(data: Vec<u8>) -> Self // from owned vec
pub fn blob_from_arc(data: CompactArc<[u8]>) -> Self // zero-copy
pub fn as_blob(&self) -> Option<&[u8]>     // accessor
```

### 3. compare_blob

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlobOrdering { Less, Equal, Greater }

fn compare_blob(a: &[u8], b: &[u8]) -> BlobOrdering {
    // 1. Compare bytes in ascending index order until difference
    // 2. If all equal, compare lengths (shorter = less)
}
```

Used in `Value::compare_same_type` and `Value::Ord`. `Value::PartialEq` uses direct byte comparison.

### 4. Serialization (`src/storage/mvcc/persistence.rs`)

Wire format (wire tag 12):
```
[u8: 12] [u32_le: length] [u8..len: data]
```

Tag 12 is free — existing tags are 0-11.

### 5. SchemaColumn Extension (`src/core/schema.rs`)

```rust
/// Fixed length for BLOB columns (None = variable length)
pub blob_length: Option<u32>,
```

### 6. DDL Parser (`src/executor/ddl.rs`)

Current workaround at line ~1131: `BLOB | "BINARY" | "VARBINARY" => Ok(DataType::Text)`
Change to: `"BYTEA" | "BLOB" | "BINARY" | "VARBINARY" => Ok(DataType::Blob)`

Handle `BYTEA(N)` via regex parsing, storing N in `SchemaColumn.blob_length`.

### 7. Rejection of BYTEA in DDL

Per RFC-0201, nullable and NOT NULL BYTEA columns must be rejected until null bitmap integration:
```rust
if column.data_type == DataType::Blob {
    return Err("BYTEA columns not supported: null bitmap integration required".into());
}
```

### 8. Hash Index (Phase 2a)

The existing `HashIndex` (`src/storage/index/hash.rs`) uses ahash. It already handles arbitrary `Value` types via `Value::hash`. Functional correctness is sufficient for Phase 2a — no hasher changes required. SipHash with persistent key is a Phase 2a follow-up.

## Implementation Phases

### Phase 1: Core types
1. Add `DataType::Blob = 10`
2. Add `Value::Blob(CompactArc<[u8]>)`
3. Add `compare_blob` and `BlobOrdering`
4. Integrate into `Value::compare`, `PartialEq`, `Ord`, `Hash`
5. Add constructors and accessors to `Value`

### Phase 2: Serialization
1. Add `serialize_value` arm for `Value::Blob` (tag 12)
2. Add `deserialize_value` arm for tag 12
3. Add `blob_length` to `SchemaColumn`

### Phase 3: Parser and DDL
1. Update DDL parser to recognize BYTEA/BLOB/BINARY/VARBINARY
2. Handle BYTEA(N) length constraint
3. Reject BYTEA columns with clear error (null bitmap not integrated)

### Phase 4: Hash Index
1. Verify hash index works with `Value::Blob` keys
2. Add round-trip test: insert blob, lookup by blob value

## Key Files to Modify

| File | Change |
|------|--------|
| `src/core/types.rs` | Add `Blob = 10`, update `FromStr`, update `is_orderable` |
| `src/core/value.rs` | Add `Value::Blob` variant, constructors, `compare_blob`, integration |
| `src/core/schema.rs` | Add `blob_length: Option<u32>` to `SchemaColumn` |
| `src/storage/mvcc/persistence.rs` | Serialize/deserialize `Value::Blob` (tag 12) |
| `src/executor/ddl.rs` | BYTEA/BLOB/BINARY/VARBINARY → `DataType::Blob` |

## Design Reference

Full design rationale: `docs/plans/2026-03-28-rfc-0201-blob-implementation-missions.md`

---

**Mission Type:** Implementation
**Priority:** High
**Phase:** Phase 2a/2b/2c/2e
