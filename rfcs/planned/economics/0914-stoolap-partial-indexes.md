# RFC-0914 (Economics): Stoolap Partial Indexes

## Status

Planned

## Summary

Add partial index support (`CREATE INDEX ... WHERE predicate`) to the CipherOcto/stoolap SQL engine. Partial indexes index only rows matching a WHERE predicate, enabling efficient active-record-only queries (e.g., `WHERE revoked = 0`) without maintaining entries for revoked/deleted rows.

## Why Needed

RFC-0903 (Virtual API Key System) defines a partial index for efficient active-key lookups:

```sql
CREATE INDEX idx_api_keys_hash_active ON api_keys(key_hash) WHERE revoked = 0;
```

The current stoolap `CreateIndexStatement` AST has no `where_clause` field — partial indexes are not supported. Binary storage (BYTEA) aside, this is the only SQL feature gap preventing RFC-0903 schema compliance.

**Use cases:**
- Active API key lookup: `WHERE revoked = 0` (vs scanning all keys including revoked)
- Time-bounded queries: `WHERE expires_at > now()`
- Soft-delete patterns: `WHERE deleted_at IS NULL`

## Scope

### SQL Syntax

```
CREATE [UNIQUE] INDEX index_name ON table_name (column [, ...]) [WHERE predicate]
```

- `predicate` is a boolean expression referencing columns of `table_name`
- Supported predicates: `column IS [NOT] NULL`, `column = value`, `column > value`, `column IN (list)`, `NOT`, `AND`, `OR`
- Unsupported: subqueries, aggregate functions, non-column expressions

### AST Changes

- Add `where_clause: Option<ParsedExpression>` field to `CreateIndexStatement` node
- Parse `WHERE predicate` after index column list, before semicolon
- Add `PartialIndexInfo { predicate: ParsedExpression, predicate_hash: [u8; 32] }` to index metadata

### Storage / Engine Changes

- Index entry written only when predicate evaluates to true on insert/update
- Index entry removed (or marked deleted) when predicate evaluates to false on update/delete
- Index-only scan supported when query WHERE clause implies partial index predicate (e.g., `WHERE revoked = 0` matches partial index with same predicate)
- Storage format: append `predicate_hash` to index entry header for fast candidate matching

### Query Planning

- Recognize when query WHERE clause is a subset of partial index predicate
- Use partial index for scans even when query doesn't explicitly include all index columns
- Cost model: partial indexes are preferred when `SELECTIVITY(predicate) < FULL_SCAN_THRESHOLD`

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final) — primary use case driver

**Optional:**

- RFC-0912: FOR UPDATE Row Locking (Accepted) — orthogonal, no direct dependency

## Related RFCs

- RFC-0903 (Final): Virtual API Key System — defines `WHERE revoked = 0` partial index requirement
- RFC-0912 (Accepted): FOR UPDATE Row Locking — another stoolap SQL feature gap
- RFC-0913 (Accepted): WAL-Only Pub/Sub — orthogonal

## Next Steps

When ready to implement: move from `rfcs/planned/economics/` to `rfcs/draft/economics/` and open PR for adversarial review.
