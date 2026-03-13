# RFC-0912 (Economics): Stoolap FOR UPDATE Row Locking

## Status

Draft (v1)

## Authors

- Author: @cipherocto

## Summary

Add explicit `FOR UPDATE` SQL syntax to CipherOcto/stoolap for pessimistic row locking, enabling atomic budget updates in multi-router deployments.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final)

**Optional:**

- RFC-0909: Deterministic Quota Accounting

## Motivation

RFC-0903's ledger-based architecture requires atomic budget updates to prevent overspend:

```sql
SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE;
-- Then UPDATE/INSERT spend_ledger
```

Stoolap's MVCC provides snapshot isolation, but lacks explicit `FOR UPDATE` for:
- Multi-router deployments (two routers processing same key concurrently)
- Budget consistency (prevent race condition on budget check)
- Deterministic accounting (same request produces same result regardless of timing)

## Design

### SQL Syntax

```sql
SELECT * FROM api_keys WHERE key_id = $1 FOR UPDATE;
```

### AST Changes

```rust
// In parser/ast.rs
pub struct SelectStatement {
    // ... existing fields ...
    pub for_update: bool,  // NEW: Add this field
}
```

### Parser Changes

```rust
// In parser/statements.rs - after ORDER BY parsing
if self.peek_token_is_keyword("FOR") {
    self.next_token();
    if self.expect_keyword("UPDATE") {
        stmt.for_update = true;
    }
}
```

### Executor Changes

When `for_update` is true:
1. Call `version_store.get_visible_versions_for_update()` instead of read-only
2. Mark rows in transaction's write_set
3. On commit, validate no conflicting modifications

## Implementation Notes

- Estimated effort: ~2-3 days
- MVCC infrastructure already exists (TransactionRegistry, TxnState)
- Need syntax parser + wire to existing internal methods

## Why Needed

- Critical for multi-router deployments (prevent race conditions)
- Enables deterministic budget enforcement
- Completes Stoolap as standalone persistence (no Redis for locking)

## Out of Scope

- Distributed locks (use WAL-based pub/sub instead)
- Deadlock detection (application-level lock ordering per RFC-0903)

## Approval Criteria

- [ ] SelectStatement AST has for_update field
- [ ] Parser handles FOR UPDATE syntax
- [ ] Executor calls get_visible_versions_for_update
- [ ] Test concurrent budget updates with multiple routers

## Related Use Case

- `docs/use-cases/stoolap-only-persistence.md`