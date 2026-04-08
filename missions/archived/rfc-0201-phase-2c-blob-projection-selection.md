# Mission: RFC-0201 Phase 2c — Blob in Projection/Selection

## Status
Archived
Complete

## Claimant

@claude-agent

## RFC

- RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage — Phase 2c
- RFC-0127 (Numeric): DCS Blob Amendment — Accepted

## Dependencies

- Phase 1 (core BYTEA): DataType::Blob, Value::Blob, wire tag 12 — **Complete**
- Phase 2b (Blob Equality in Expression Evaluation) — recommended before this phase

## Context

Phase 2c verifies that Blob columns work correctly in SQL projection and selection contexts:

```sql
SELECT key_hash, signature FROM usage_ledger WHERE event_id = $1;
```

**What's needed:**
- Integration tests verifying Blob columns appear correctly in SELECT results
- Verify `Value::Blob` serializes correctly through the query result encoding path
- Hash index usage for Blob equality in WHERE clauses

**stoolap implementation status:**
- `Value::Blob` already has full comparison (byte-by-byte), hashing, and Display implemented
- `serialize_value` / `deserialize_value` handle wire tag 12 correctly
- Hash table hashing already handles `Value::Blob`
- Phase 2a (hash index) and 2b (expression equality) are likely already satisfied by existing code

## Acceptance Criteria

- [x] Integration test: `CREATE TABLE t (id INTEGER, key_hash BYTEA(32)); INSERT INTO t VALUES (1, $1); SELECT key_hash FROM t WHERE id = 1;`
- [x] Verify result set correctly returns Blob value (hex-encoded in wire response)
- [x] Verify Blob equality works in WHERE clause with hash index available
- [x] `cargo test --lib` passes with 0 failures
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes

## Technical Notes

### Blob Wire Format (per RFC-0201)

Format: `[u8:12][u32_be:length][u8..len:data]`
- Tag 12 for Blob
- Big-endian length prefix
- Raw bytes (no UTF-8 validation)

### Result Set Encoding

stoolap's result set uses `serialize_value` for encoding column values. This already handles `Value::Blob` correctly — no special result encoding needed.

## Key Files to Modify

| File | Change |
|------|--------|
| `tests/` | Add integration tests for Blob in projection/selection |

## Design Reference

- RFC-0201 Phase 2c specification: `rfcs/accepted/storage/0201-binary-blob-type-support.md` §Phase 2c
- Existing Blob implementation: `src/core/value.rs`, `src/storage/mvcc/persistence.rs`

---

**Mission Type:** Testing / Verification
**Priority:** Medium
**Phase:** Phase 2c
