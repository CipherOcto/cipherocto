# RFC-0203: REINDEX Command for BTree Index Rebuild

## Status

Draft

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Add `REINDEX` SQL command to stoolap that rebuilds BTree indexes on BIGINT and DECIMAL columns (and optionally all indexes). This is required per RFC-0202-A §6.11 for production deployment when migrating from pre-lexicographic encoding or recovering from index corruption.

## Dependencies

**Requires:**

- RFC-0202-A: Stoolap BIGINT and DECIMAL Core Types (for lexicographic encoding support)

**Optional:**

- RFC-0201: Expression VM Gas Metering Integration

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Deterministic rebuild | Same input → same output index |
| G2 | No data loss | All rows must be re-indexed correctly |
| G3 | Transparent to queries | Other sessions continue normally during rebuild |

## Motivation

RFC-0202-A §6.11 introduces lexicographic key encoding for BIGINT and DECIMAL BTree indexes. Production deployment with pre-existing BIGINT/DECIMAL data requires a mechanism to rebuild indexes with the new encoding. Additionally, B-tree indexes can become fragmented or corrupted over time, requiring a rebuild capability.

Stoolap currently has no `REINDEX` command. This RFC adds:
1. `REINDEX INDEX index_name` — rebuild a specific index
2. `REINDEX TABLE table_name` — rebuild all indexes on a table
3. `REINDEX DATABASE` — rebuild all indexes in the database (optional, lower priority)

## Specification

### Syntax

```sql
-- Rebuild a specific index
REINDEX INDEX index_name;

-- Rebuild all indexes on a table
REINDEX TABLE table_name;

-- Rebuild all indexes in the database (optional)
REINDEX DATABASE;
```

### Grammar (BNF-like)

```
reindex_stmt ::= REINDEX (INDEX index_name | TABLE table_name | DATABASE)
```

### Parser Changes

**File:** `src/parser/ast.rs`

Add to `Statement` enum:
```rust
Reindex(ReindexStatement),
```

Add `ReindexStatement` struct:
```rust
pub struct ReindexStatement {
    pub token: Token,
    pub target: ReindexTarget,
}

pub enum ReindexTarget {
    Index(Identifier),       // REINDEX INDEX idx_name
    Table(Identifier),      // REINDEX TABLE tbl_name
    Database,               // REINDEX DATABASE (all indexes)
}
```

**File:** `src/parser/statements.rs`

Add parser rule to parse `REINDEX` keyword and construct `Statement::Reindex`.

### Executor Changes

**File:** `src/executor/ddl.rs`

Add `execute_reindex(&self, stmt: &ReindexStatement, _ctx: &ExecutionContext) -> Result<Box<dyn QueryResult>>`:

```rust
pub(crate) fn execute_reindex(
    &self,
    stmt: &ReindexStatement,
    _ctx: &ExecutionContext,
) -> Result<Box<dyn QueryResult>> {
    match &stmt.target {
        ReindexTarget::Index(idx_name) => self.reindex_index(idx_name),
        ReindexTarget::Table(tbl_name) => self.reindex_table(tbl_name),
        ReindexTarget::Database => self.reindex_database(),
    }
}
```

### Index Rebuild Algorithm

**For each index to rebuild:**

1. Acquire table write lock
2. Clear existing BTree index data (`sorted_values.clear()`)
3. Scan all rows in the table (via storage layer)
4. For each row, compute index key and re-insert into BTree
5. Invalidate min/max cache
6. Release lock

**Lexicographic encoding for BIGINT/DECIMAL:**
- Use `encode_bigint_lexicographic(bi)` from `storage/index/btree.rs`
- Use `encode_decimal_lexicographic(d)` from `storage/index/btree.rs`
- Fall back to raw `Value::Ord` for other types

### Error Handling

| Error | Condition | Response |
|-------|-----------|----------|
| IndexNotFound | REINDEX INDEX on non-existent index | `Error::IndexNotFound` |
| TableNotFound | REINDEX TABLE on non-existent table | `Error::TableNotFound` |
| IndexInUse | Another transaction holds index lock | `Error::Busy` with retry hint |
| Corruption | Checksum mismatch during scan | `Error::internal("index corruption detected")` |

### Determinism Requirements

Rebuild must be deterministic for verification:
- Same input rows → same output index structure
- Scan order must be consistent (by row_id ascending)
- Key encoding must match RFC-0202-A §6.11 exactly

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Throughput | >10k rows/sec | Single-threaded rebuild |
| Memory | O(index_size) | No temporary structures beyond index itself |

## Security Considerations

- Only ADMIN role or table owner can execute REINDEX
- REINDEX holds write locks — long-running rebuilds block concurrent writes
- No data is deleted, only re-arranged — rollback possible on failure

## Adversarial Review

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Lock contention | Medium | REINDEX acquires exclusive lock; warn in docs |
| Partial rebuild on crash | High | Atomic: clear then rebuild; crash leaves empty (detectable) |
| Memory exhaustion | Medium | Process in batches if index is very large |

## Compatibility

- **Backward compatible**: existing databases without BIGINT/DECIMAL indexes unaffected
- **REINDEX DATABASE** is optional — not all deployments need it

## Alternatives Considered

| Approach | Pros | Cons |
|---------|------|------|
| REINDEX as new command | Explicit, clear semantics | Parser/executor changes required |
| Background online rebuild | Non-blocking | Complex, may not be deterministic |
| Auto-upgrade on access | Transparent | Hidden side-effects, non-deterministic |

## Implementation Phases

### Phase 1: Core

- [ ] Add `ReindexStatement` and `ReindexTarget` to `parser/ast.rs`
- [ ] Add parser rule for `REINDEX` in `parser/statements.rs`
- [ ] Add `Statement::Reindex` match arm in executor
- [ ] Implement `execute_reindex` in `executor/ddl.rs`
- [ ] Implement `reindex_index(idx_name)` — single index rebuild

### Phase 2: Table and Database

- [ ] Implement `reindex_table(tbl_name)` — all indexes on table
- [ ] Implement `reindex_database()` — all indexes in database (optional)

### Phase 3: Verification

- [ ] Test: REINDEX on BIGINT BTree index preserves data
- [ ] Test: REINDEX on DECIMAL BTree index preserves data
- [ ] Test: REINDEX on empty index is no-op
- [ ] Test: REINDEX on non-existent index returns error
- [ ] Test: Concurrent REINDEX blocked by lock

## Key Files to Modify

| File | Change |
|------|--------|
| `src/parser/ast.rs` | Add `Statement::Reindex`, `ReindexStatement`, `ReindexTarget` |
| `src/parser/statements.rs` | Add REINDEX parser rule |
| `src/parser/mod.rs` | Export new types |
| `src/executor/ddl.rs` | Add `execute_reindex`, `reindex_index`, `reindex_table` |
| `src/executor/mod.rs` | Wire REINDEX to executor |
| `src/storage/index/btree.rs` | Ensure `encode_bigint_lexicographic` / `encode_decimal_lexicographic` are `pub` |

## Future Work

- F1: Online (non-blocking) REINDEX using shadow index and cutover
- F2: Parallel REINDEX using multiple reader threads
- F3: Partial REINDEX (single partition of a sharded table)

## Rationale

`REINDEX` is a standard SQL command (PostgreSQL, SQLite, MySQL all support it). The RFC-0202-A AC explicitly requires it for production deployment of BIGINT/DECIMAL with lexicographic encoding. Implementing it as a first-class SQL command keeps the UX consistent with other DDL commands (CREATE INDEX, DROP INDEX).

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-04-11 | Initial draft |

## Related RFCs

- [RFC-0202-A: Stoolap BIGINT and DECIMAL Core Types](./0202-stoolap-bigint-decimal-conversions.md) — defines lexicographic encoding
- [RFC-0201: Expression VM Gas Metering Integration](./0201-expression-vm-gas-metering.md) — gas metering infrastructure

## Related Use Cases

- [Mission 0202-c: BIGINT/DECIMAL Persistence (AC-9: REINDEX documentation)](../../missions/claimed/0202-c-bigint-decimal-persistence.md)