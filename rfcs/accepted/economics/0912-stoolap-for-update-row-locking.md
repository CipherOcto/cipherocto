# RFC-0912 (Economics): Stoolap FOR UPDATE Row Locking

## Status

Accepted (v3)

## Authors

- Author: @cipherocto

## Summary

Add explicit `FOR UPDATE` SQL syntax to CipherOcto/stoolap for pessimistic row locking, enabling atomic budget updates in multi-router deployments. Implementation leverages existing internal MVCC methods (`get_visible_versions_for_update`, `get_all_visible_rows_for_update`).

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final v29)

**Optional:**

- RFC-0909: Deterministic Quota Accounting

## Motivation

RFC-0903's ledger-based architecture requires atomic budget updates to prevent overspend:

```sql
SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE;
-- Then UPDATE/INSERT spend_ledger
```

Stoolap's MVCC provides snapshot isolation, but lacks explicit `FOR UPDATE` SQL syntax for:
- Multi-router deployments (two routers processing same key concurrently)
- Budget consistency (prevent race condition on budget check)
- Deterministic accounting (same request produces same result regardless of timing)

## Code Analysis Summary

Analysis of Stoolap source at `/home/mmacedoeu/_w/databases/stoolap/src/`:

### Existing Infrastructure

| Component | Location | Status |
|-----------|----------|--------|
| `SelectStatement` AST | `parser/ast.rs:1435` | Missing `for_update` field |
| Parser | `parser/statements.rs:80` | No FOR UPDATE handling |
| Executor | `executor/query.rs:193` | `execute_select` entry point |
| Version Store | `storage/mvcc/version_store.rs:1597` | `get_visible_versions_for_update()` exists |
| Version Store | `storage/mvcc/version_store.rs:2215` | `get_all_visible_rows_for_update()` exists |
| MVCC Table | `storage/mvcc/table.rs:1808,1939,1949` | Internal calls to for_update methods |

### Key Findings

1. **MVCC Infrastructure Complete**: The version store already has `get_visible_versions_for_update()` and `get_all_visible_rows_for_update()` methods that return rows for modification.

2. **Transaction Tracking**: `TransactionRegistry` (`storage/mvcc/registry.rs:188`) tracks active transactions with `TxnState` (`registry.rs:59`) containing `begin_seq` and `state_seq`.

3. **Internal Usage**: The methods are already used internally in table.rs (lines 1808, 1864, 1939, etc.) for UPDATE operations - just not exposed via SQL syntax.

## Design

### SQL Syntax

```sql
SELECT * FROM api_keys WHERE key_id = $1 FOR UPDATE;
SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE;
```

### AST Changes

```rust
// In parser/ast.rs:1435
pub struct SelectStatement {
    pub token: Token,
    pub distinct: bool,
    pub columns: Vec<Expression>,
    pub with: Option<WithClause>,
    pub table_expr: Option<Box<Expression>>,
    pub where_clause: Option<Box<Expression>>,
    pub group_by: GroupByClause,
    pub having: Option<Box<Expression>>,
    pub window_defs: Vec<WindowDefinition>,
    pub order_by: Vec<OrderByExpression>,
    pub limit: Option<Box<Expression>>,
    pub offset: Option<Box<Expression>>,
    pub set_operations: Vec<SetOperation>,
    // NEW:
    pub for_update: bool,  // Add this field
}
```

### Parser Changes

```rust
// In parser/statements.rs - after OFFSET parsing (around line 190)
// Parse FOR UPDATE clause
if self.peek_token_is_keyword("FOR") {
    self.next_token(); // consume FOR
    if self.expect_keyword("UPDATE") {
        stmt.for_update = true;
    }
}
```

**Grammar rule:** `FOR UPDATE` is accepted after `ORDER BY`, `LIMIT`, and `OFFSET`:
```
SelectStatement ::= SELECT ... [WHERE ...] [GROUP BY ...] [HAVING ...]
                    [ORDER BY expr [ASC|DESC], ...]
                    [LIMIT n [OFFSET n]]
                    [FOR UPDATE]
```

### Executor Changes

```rust
// In executor/query.rs:193 - execute_select
pub(crate) fn execute_select(
    &self,
    stmt: &SelectStatement,
    ctx: &ExecutionContext,
) -> Result<Box<dyn QueryResult>> {
    // NEW: Check for_update flag and route to appropriate read method
    let for_update = stmt.for_update;

    // ... existing code ...

    // When reading rows, use:
    // - for_update = false: version_store.get_all_visible_rows_cached(txn_id)
    // - for_update = true: version_store.get_all_visible_rows_for_update(txn_id)
}
```

### Display Implementation

```rust
// In parser/ast.rs - SelectStatement Display
impl fmt::Display for SelectStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // ... existing formatting ...
        if self.for_update {
            write!(f, " FOR UPDATE")?;
        }
        Ok(())
    }
}
```

## Implementation Details

### Step 1: AST Modification (~30 min)

Add `for_update: bool` to `SelectStatement` in `parser/ast.rs:1435`.

### Step 2: Parser Implementation (~2 hours)

Add FOR UPDATE parsing in `parser/statements.rs` after OFFSET clause (around line 190).

### Step 3: Display Implementation (~30 min)

Update `Display` impl for `SelectStatement` to output `FOR UPDATE`.

### Step 4: Executor Integration (~3 hours)

Modify `executor/query.rs:execute_select` to:
1. Check `stmt.for_update` flag
2. Pass to table scan method
3. Use `get_visible_versions_for_update()` for reads

### Step 5: Table Scan Changes (~2 hours)

In `storage/mvcc/table.rs`:
- Modify scan methods to accept `for_update: bool` parameter
- Route to appropriate version store method based on flag

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_for_update_parser() {
    let sql = "SELECT * FROM api_keys WHERE key_id = '123' FOR UPDATE";
    let stmt = parse(sql).unwrap();
    assert!(stmt.for_update);
}

#[test]
fn test_for_update_display() {
    let mut stmt = SelectStatement::default();
    stmt.for_update = true;
    assert_eq!(stmt.to_string().contains("FOR UPDATE"), true);
}
```

### Integration Tests

```rust
#[test]
fn test_concurrent_budget_update() {
    // Start two transactions
    let tx1 = db.begin_transaction().unwrap();
    let tx2 = db.begin_transaction().unwrap();

    // Both try to SELECT ... FOR UPDATE same key
    let result1 = tx1.execute("SELECT budget FROM api_keys WHERE key_id = '123' FOR UPDATE");
    let result2 = tx2.execute("SELECT budget FROM api_keys WHERE key_id = '123' FOR UPDATE");

    // Second should wait or fail depending on isolation level
    // Verify deterministic behavior
}
```

## Feasibility Assessment

| Aspect | Finding | Effort |
|--------|---------|--------|
| AST field addition | Simple bool field | ~30 min |
| Parser | Add keyword handling after OFFSET | ~2 hours |
| Executor routing | Pass flag to table scan | ~3 hours |
| Internal methods | Already exist, just wire up | ~2 hours |
| **Total** | | **~1-2 days** |

## Why Needed

- **Critical** for multi-router deployments (prevent race conditions)
- Enables deterministic budget enforcement (per RFC-0909)
- Completes Stoolap as standalone persistence (no Redis for locking)
- Internal MVCC methods already exist - just needs SQL syntax

### Locking Contract

Invocation of `get_visible_versions_for_update` / `get_all_visible_rows_for_update` acquires an exclusive row lock held until the enclosing transaction ends (see `TransactionRegistry` in `storage/mvcc/registry.rs:188` and `TxnState` at `registry.rs:59`). This locking guarantee is required for cross-router determinism in multi-instance deployments.

## Out of Scope

- **Distributed locks across multiple Stoolap instances** - `FOR UPDATE` provides intra-instance pessimistic locking only. For production multi-router deployments with independent Stoolap processes, use:
  - Stoolap replication (leader-follower), or
  - WAL-based pub/sub (RFC-0913), or
  - A shared primary instance
- Deadlock detection (application-level lock ordering per RFC-0903)
- NOWAIT / SKIP LOCKED variants (future enhancement)

## Lock Ordering Invariant

Per RFC-0903, all transactions that lock both `teams` and `api_keys` rows MUST acquire the team lock BEFORE the key lock:

```sql
-- Correct order:
SELECT * FROM teams WHERE team_id = $1 FOR UPDATE;
SELECT * FROM api_keys WHERE key_id = $2 FOR UPDATE;
```

This prevents deadlocks in multi-router deployments.

## Approval Criteria

- [ ] SelectStatement AST has `for_update: bool` field
- [ ] Parser handles `FOR UPDATE` syntax after ORDER BY/OFFSET
- [ ] Display impl outputs `FOR UPDATE` clause
- [ ] Executor routes to `get_visible_versions_for_update` when flag is set
- [ ] Unit tests for parser, AST, and Display
- [ ] Integration test for concurrent budget updates
- [ ] Integration test for lock ordering (team before key)

## Implementation Estimate

- **Effort**: ~2 days (8 hours)
- **Risk**: Low (internal methods already exist)
- **Complexity**: Medium (affects parser, executor, table scan)

## Related Use Cases and RFCs

- Use Case: `docs/use-cases/stoolap-only-persistence.md`
- RFC-0903: Virtual API Key System (Final v29)
- RFC-0909: Deterministic Quota Accounting (Optional)
- RFC-0913: Stoolap Pub/Sub for Cache Invalidation (depends on this)

## Changelog

- **v3 (2026-03-13):** Review clarifications (Grok review)
  - Added "Locking Contract" section documenting row lock held until transaction end (references TransactionRegistry/TxnState)
  - Added grammar rule for FOR UPDATE clause position (after ORDER BY, LIMIT, OFFSET)
  - Strengthened "Out of Scope" with multi-router deployment note (intra-instance only, use RFC-0913 for distributed)

- **v2 (2026-03-13):** Added comprehensive code analysis from Stoolap source
  - Documented existing internal methods (`get_visible_versions_for_update`, `get_all_visible_rows_for_update`)
  - Located TransactionRegistry and TxnState for transaction tracking
  - Mapped executor entry points and table scan methods
  - Added detailed implementation steps with code locations
  - Added testing strategy with code examples

- **v1 (2026-03-13):** Initial draft