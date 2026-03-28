# Mission: RFC-0201 Phase 2a — Hash Index for Blob Columns

## Status

Claimed

## Claimant

@claude-agent

## RFC

RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage — Phase 2a

## Dependencies

- Phase 1 (core BYTEA): `DataType::Blob`, `Value::Blob`, wire tag 12 — **Complete**

## Context

Phase 2a implements a SipHash-based hash index for Blob columns:

```sql
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash) USING HASH;
```

**RFC Requirement:**
- Hash function: SipHash-2-4 with a 128-bit key generated at database open time
- Index structure: `HashMap<SipHash output, Vec<row_id>>`
- Blob hash key: the full 32-byte (or variable-length) blob content
- O(1) average equality lookup
- **Fallback mode (required):** If hash index cannot be rebuilt after key loss, database opens with hash index disabled. Queries fall back to full scans.

**stoolap implementation status:**
- `HashIndex` exists in `src/storage/index/hash.rs` using `ahash`
- `Value::hash` already handles `Value::Blob` via standard hasher
- Current ahash may not be SipHash - need to verify or implement SipHash

## Acceptance Criteria

- [ ] Hash index functional with `Value::Blob` keys
- [ ] Round-trip test: insert blob, lookup by blob value
- [ ] Fallback mode works when hash index is disabled
- [ ] `cargo test --lib` passes with 0 failures
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes

## Technical Details

### RFC-0201 SipHash Requirement

```rust
// SipHash-2-4 with 128-bit key
// Key is generated at database open time and persisted
let test_key = [0u8; 16];  // In production: HKDF-SHA256 derived key

fn siphash_2_4(data: &[u8], key: &[u8; 16]) -> u64 {
    // Reference: https://131002.net/siphash/
}
```

### Fallback Mode

If the hash index key is lost or corrupted:
1. Database opens with hash index marked as "degraded"
2. Queries using `=` on blob columns do full table scan
3. Index can be rebuilt via `REINDEX`

### Integration Test

```rust
#[test]
fn test_hash_index_blob_equality() {
    let db = Database::open_in_memory().expect("Failed to create database");

    db.execute(
        "CREATE TABLE api_keys (id INTEGER PRIMARY KEY, key_hash BYTEA(32))",
        (),
    ).expect("Failed to create table");

    db.execute(
        "CREATE INDEX idx_hash ON api_keys(key_hash) USING HASH",
        (),
    ).expect("Failed to create index");

    let key1 = vec![0x01u8; 32];
    let key2 = vec![0x02u8; 32];

    db.execute("INSERT INTO api_keys VALUES (1, $1)", (key1.clone(),)).unwrap();
    db.execute("INSERT INTO api_keys VALUES (2, $1)", (key2.clone(),)).unwrap();

    // Lookup by blob value - should use hash index
    let result: Vec<i64> = db
        .query_one::<Vec<i64>, _>(
            "SELECT id FROM api_keys WHERE key_hash = $1",
            (key1.clone(),),
        )
        .expect("Failed to query");

    assert_eq!(result, vec![1]);
}
```

## Key Files to Modify

| File | Change |
|------|--------|
| `src/storage/index/hash.rs` | Verify/implement SipHash, add fallback mode |
| `tests/` | Add integration tests for Blob hash index |

## Design Reference

- RFC-0201 Phase 2a specification: `rfcs/accepted/storage/0201-binary-blob-type-support.md` §Phase 2a
- Existing HashIndex: `src/storage/index/hash.rs`
- SipHash reference: https://131002.net/siphash/

---

**Mission Type:** Implementation + Testing
**Priority:** High
**Phase:** Phase 2a
