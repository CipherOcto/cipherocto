# Mission: RFC-0201 Phase 2b — Blob Equality in Expression Evaluation

## Status

Claimed

## Claimant

@claude-agent

## RFC

RFC-0201 (Storage): Binary BLOB Type for Deterministic Hash Storage — Phase 2b

## Dependencies

- Phase 1 (core BYTEA): `DataType::Blob`, `Value::Blob`, wire tag 12 — **Complete**
- Phase 2c (projection/selection) — **Complete**

## Context

Phase 2b verifies that Blob equality works correctly in SQL expressions, specifically in `WHERE` clauses:

```sql
SELECT * FROM events WHERE event_id = $1;
SELECT * FROM api_keys WHERE key_hash = $1;
```

**stoolap implementation status:**
- `Value::Blob` has `PartialEq`, `Ord`, and `Hash` implemented
- Byte-by-byte comparison is the default behavior
- `Value::compare_same_type` handles Blob comparison
- Expression VM should route `=` (Eq) and `<>` (NotEq) comparisons through existing `Value::compare` path

**What's needed:**
- Verify Blob equality works in WHERE clauses (may already work)
- Add integration test for Blob equality in expression context

## Acceptance Criteria

- [ ] Integration test: Blob equality in WHERE clause
- [ ] Integration test: Blob inequality in WHERE clause
- [ ] `cargo test --lib` passes with 0 failures
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes

## Technical Notes

### Blob Comparison

```rust
impl PartialEq for Value::Blob {
    fn eq(&self, other: &Self) -> bool {
        // Byte-by-byte comparison
        self.0 == other.0
    }
}
```

The expression VM's `evaluate_binary_op` for `=` should call `Value::eq` which uses this implementation.

### Expected Behavior

```sql
CREATE TABLE t (id INTEGER, key BYTEA(32));
INSERT INTO t VALUES (1, x'0102030405060708091011121314151617181920212223242526272829303132');
INSERT INTO t VALUES (2, x'0102030405060708091011121314151617181920212223242526272829303132');
INSERT INTO t VALUES (3, x'FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF');

-- Returns row 1 (exact match)
SELECT * FROM t WHERE key = x'0102030405060708091011121314151617181920212223242526272829303132';

-- Returns rows 1 and 2 (both start with 01... but only 1 and 2 are identical)
SELECT * FROM t WHERE key = key;

-- Returns row 3 (different from the other two)
SELECT * FROM t WHERE key <> key;
```

## Key Files to Modify

| File | Change |
|------|--------|
| `tests/` | Add integration tests for Blob equality in expressions |

## Design Reference

- RFC-0201 Phase 2b specification: `rfcs/accepted/storage/0201-binary-blob-type-support.md` §Phase 2b
- Existing Blob implementation: `src/core/value.rs`

---

**Mission Type:** Testing / Verification
**Priority:** Medium
**Phase:** Phase 2b
