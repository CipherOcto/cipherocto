# RFC-0914: Stoolap Partial Indexes

## Status

Draft (v13.0)

## Authors

- Author: @agent

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Add partial index support (`CREATE INDEX ... WHERE predicate`) to the CipherOcto/stoolap SQL engine. Partial indexes index only rows matching a WHERE predicate, enabling efficient active-record-only queries without maintaining entries for revoked/deleted rows. This is the final SQL feature gap preventing RFC-0903 (Virtual API Key System) full schema compliance.

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final) — primary use case driver

**Optional:**

- RFC-0912: FOR UPDATE Row Locking (Final) — orthogonal, no direct dependency

## Design Goals

| Goal | Target                       | Metric                                                                  |
| ---- | ---------------------------- | ----------------------------------------------------------------------- |
| G1   | Minimal AST changes          | Single `where_clause` field added to CreateIndexStatement               |
| G2   | Predicate subset recognition | Query WHERE matching partial index predicate uses index                 |
| G3   | Backward compatibility       | Existing indexes work without modification                              |
| G4   | RFC-0903 compliance          | `WHERE revoked = 0` partial index syntax supported                      |
| G5   | Expression serialization     | Predicate stored compactly in IndexMetadata, not per-entry              |
| G6   | Safe UNIQUE semantics        | Unique partial indexes require application-level immutability guarantee |
| G7   | Query planning clarity       | Distinguish query planning from scan execution                          |

## Motivation

RFC-0903 (Virtual API Key System) requires an efficient active-key lookup pattern:

```sql
CREATE INDEX idx_api_keys_hash_active ON api_keys(key_hash) WHERE revoked = 0;
```

This partial index ensures:

- Only active (non-revoked) keys are indexed
- `key_hash` lookups skip revoked keys without full scan
- Space is not wasted indexing revoked keys that will never be queried

The current stoolap `CreateIndexStatement` AST has no `where_clause` field. Binary storage (BYTEA) for `key_hash` aside, **partial indexes are the only SQL feature gap** preventing RFC-0903 schema compliance.

**Use cases:**

- Active API key lookup: `WHERE revoked = 0`
- Soft-delete patterns: `WHERE deleted_at IS NULL`
- Multi-tenant isolation: `WHERE tenant_id = 1` (tenant ID is set at row creation and immutable)

## Specification

### SQL Syntax

```
CREATE [UNIQUE] INDEX index_name ON table_name (column [, ...]) [WHERE predicate]
```

**WITH clause placement (PostgreSQL-compatible):**

```
CREATE [UNIQUE] INDEX index_name ON table_name (column [, ...]) [WITH (storage_options)] [WHERE predicate]
```

The `WHERE` clause follows the `WITH` clause if present, otherwise follows the column list directly.

**IF NOT EXISTS behavior:**

- `CREATE INDEX IF NOT EXISTS idx ON t(c) WHERE p` — succeeds if index `idx` exists with identical predicate `p`
- `CREATE INDEX IF NOT EXISTS idx ON t(c) WHERE p1` where `idx` exists with different predicate `p2` — succeeds with warning W001 logged: "index 'idx' already exists with different predicate"
- Rationale: Silent success with different predicate can confuse applications; warning enables debugging

**Predicate grammar:**

```
predicate ::= comparison_predicate | null_predicate | in_predicate | and_predicate | or_predicate | not_predicate | between_predicate

comparison_predicate ::= column {= | > | >= | < | <= | !=} value
null_predicate ::= column IS [NOT] NULL
in_predicate ::= column IN (value [, ...])
between_predicate ::= column BETWEEN value AND value
and_predicate ::= predicate AND predicate [AND predicate ...]  /* N-ary AND supported */
or_predicate ::= predicate OR predicate [OR predicate ...]  /* N-ary OR supported */
not_predicate ::= NOT predicate

value ::= literal | column
```

**N-ary operators:** The grammar explicitly supports N-ary AND and OR (e.g., `a AND b AND c AND d`). The parser constructs a left-deep tree but canonicalization sorts operands, making the tree structure irrelevant for equivalence checking.

**Supported expressions in predicates:**

- Column references to columns of the indexed table
- Constant literals (integers, strings, floats, BigInt, Decimal)
- Comparison operators: `=`, `>`, `>=`, `<`, `<=`, `!=`
- NULL checks: `IS NULL`, `IS NOT NULL`
- Set membership: `IN (list)`
- BETWEEN: `col BETWEEN 1 AND 10` (equivalent to `col >= 1 AND col <= 10`)
- Boolean logic: `AND`, `OR`, `NOT`

**Explicitly unsupported in predicates:**

- Subqueries (`EXISTS`, `IN (SELECT ...)`, scalar subqueries)
- Aggregate functions (`SUM`, `COUNT`, etc.)
- Non-column expressions (e.g., `a + b > 10`)
- Joins
- LIKE/ILIKE
- Function calls (including `now()`, `current_timestamp`, `current_date`, etc.)
- Parameterized placeholders (`$1`, `?`) — use constant literals only
- Row constructors (`(a, b) IN ((1, 2), (3, 4))`)
- EXISTS predicate

**Time-Based Predicates:**

Time-dependent predicates (`now()`, `current_timestamp`, etc.) are explicitly unsupported because they violate determinism requirements (see Section: Determinism Requirements).

For use cases requiring time-bounded queries, use constant literals:

```sql
-- Unsupported (non-deterministic):
CREATE INDEX idx ON t(c) WHERE expires_at > current_timestamp

-- Supported (deterministic):
CREATE INDEX idx ON t(c) WHERE expires_at > TIMESTAMP '2026-01-01 00:00:00'
```

Applications should refresh time-bounded indexes periodically using `DROP INDEX` + `CREATE INDEX` with updated constants.

### AST Changes

```rust
// In parser/ast.rs — CreateIndexStatement
pub struct CreateIndexStatement {
    pub token: Token,
    pub index_name: Identifier,
    pub table_name: Identifier,
    pub columns: Vec<Identifier>,
    pub is_unique: bool,
    pub if_not_exists: bool,
    pub index_method: Option<IndexMethod>,
    pub options: Vec<(String, String)>,
    /// WHERE clause for partial index
    /// None = full index (all rows)
    /// Some(predicate) = partial index (matching rows only)
    pub where_clause: Option<Expression>,
}
```

### Expression Serialization

The predicate `Expression` must be serializable for storage in `IndexMetadata`. The serialization format uses a compact binary representation with version header:

**Serialized form (stored once in IndexMetadata):**

```
predicate_binary ::= VERSION (u8) + LENGTH (u16) + rkyv(Expression)
```

- `VERSION`: 1-byte serialization format version (current: 0x01)
- `LENGTH`: 2-byte unsigned little-endian length (max 64KB expression size)
- `rkyv(Expression)`: rkyv (zero-copy deserialization) encoded Expression AST

**Serialization version history:**

| Version | Format       | Notes          |
| ------- | ------------ | -------------- |
| 0x00    | (reserved)   | Not used       |
| 0x01    | rkyv current | Initial format |

**Requirements:**

1. `Expression` must implement `rkyv::Archive` for zero-copy serialization
2. `Expression` must implement `Clone` for evaluation copies
3. Maximum serialized predicate size: 64KB (enforced at parse time)
4. Predicate depth limit: 20 AST levels (enforced at parse time)
5. Maximum AND/OR terms: 32 (enforced at parse time)
6. Maximum IN list items: 32 (enforced at parse time)
7. Maximum BETWEEN terms: 1 per column (enforced at parse time). Multiple BETWEEN on the same column in a single predicate is rejected (E011). Multiple BETWEEN on different columns are allowed (e.g., `WHERE a BETWEEN 1 AND 2 AND b BETWEEN 3 AND 4`).

**Forward compatibility:**

- Serialization format includes version byte to allow future format changes
- Old code (pre-v4.0) reading v4.0+ format: **hard incompatibility** — old code expects raw rkyv without VERSION byte; will misinterpret VERSION byte (0x01) as rkyv data, causing deserialization failure. Users must recreate partial indexes when downgrading.
- Old code reading old format: deserialize normally (backward compatible)
- New code reading old format: if Expression structure unchanged, deserialize normally
- If Expression structure changes in a future version, version header allows graceful error

**Operational requirement:** Before downgrading from v4.0+ to pre-v4.0, users MUST drop all partial indexes first. Failure to do so will result in corrupted partial indexes — old code will misinterpret the VERSION byte as rkyv data, causing deserialization failures and potential data corruption. Implementation MUST provide a `stoolap index-convert` tool to convert partial indexes to full indexes as part of the downgrade workflow.

**`stoolap index-convert` tool specification:**

- **Purpose:** Converts one or more partial indexes to equivalent full indexes before downgrading
- **Operation:** For each partial index `idx` on table `t` with columns `(c1, c2, ...)` and predicate `P`, creates a new full index `idx_full` on `(c1, c2, ...)` and drops the original partial index `idx`
- **Command syntax:** `stoolap index-convert --db <path> [--index <name>] [--all]`
  - `--all` (default): converts all partial indexes in the database
  - `--index <name>`: converts only the named partial index
- **Preservation:** The new full index preserves the original's name with `_full` suffix appended (e.g., `idx_active` → `idx_active_full`). The original partial index is dropped after the full index is successfully created.
- **Behavior:** Operates offline (database must not be open for writes); returns error if any conversion fails; on error, no changes are made (atomic)
- **Example workflow:** `stoolap index-convert --db prod.db --all && stoolap dump-prod.db-v4-schema > prod-v4.sql`

**Note on serialization format:** The RFC uses `rkyv` because stoolap already uses rkyv for zero-copy serialization elsewhere. If `Expression` does not implement `rkyv::Archive`, a fallback to `serde` serialization with `bincode` may be used, at the cost of zero-copy properties. Implementation should verify which serialization format is available and document the choice.

### Storage Format

**Index entry format (unchanged from full index):**

```
key_bytes || value_bytes
```

**Critical distinction: Query Planning vs. Scan Execution**

The `predicate_hash` stored in `IndexMetadata` is used for **query planning only** (matching query predicates to index predicates), not for scan filtering. During scan, the predicate must be re-evaluated on each candidate row.

**Why predicates must be re-evaluated on scan:**

1. Index entries are stored as `key_bytes || value_bytes` — no per-entry predicate hash
2. When scanning, we read entries from B-tree and must determine which match the query
3. Since entries don't contain predicate metadata, we must evaluate the predicate against each row
4. The `predicate_hash` enables O(1) lookup to find candidate indexes during planning; actual filtering happens at execution time

**Exception: Index-only scan.** If the query predicate exactly matches the index predicate AND all columns in the predicate are indexed columns, no re-evaluation is needed — the index itself guarantees the predicate is satisfied. **However, MVCC visibility checks still apply:** index-only scans must verify row visibility against the current transaction snapshot, not just return indexed entries blindly.

**Predicate hash computation:**

```
predicate_hash = SHA-256(canonicalize_predicate(predicate, &column_types))
```

The canonical form is serialized to bytes before hashing to ensure deterministic results across restarts. **Note:** The canonicalization function requires column type information to apply type coercion correctly (e.g., `col = 0` where `col` is BIGINT → `col = BIGINT '0'`). Both index creation (at `CREATE INDEX` time) and query planning (at `SELECT` time) must use the same column types to produce matching hashes.

**Index metadata extension:**

```rust
// In storage/mvcc/persistence.rs — IndexMetadata
pub struct IndexMetadata {
    pub name: String,
    pub table_name: String,
    pub column_names: Vec<String>,
    pub column_ids: Vec<i32>,
    pub data_types: Vec<DataType>,
    pub is_unique: bool,
    pub index_type: IndexType,
    pub hnsw_m: Option<u16>,
    pub hnsw_ef_construction: Option<u16>,
    pub hnsw_ef_search: Option<u16>,
    pub hnsw_distance_metric: Option<u8>,
    /// Predicate hash for partial index query planning (SHA-256)
    /// Only populated for partial indexes
    pub partial_index_hash: Option<[u8; 32]>,
    /// Serialized predicate expression (rkyv bytes with version header)
    /// Only populated for partial indexes
    pub partial_index_predicate: Option<Vec<u8>>,
}
```

**On INSERT:**

1. Evaluate predicate on new row using current column values
2. If true: write index entry normally
3. If false: do not write index entry (row is not indexed)

**On UPDATE:**

1. Evaluate predicate on row BEFORE update → `pred_before`
2. Evaluate predicate on row AFTER update → `pred_after`
3. If `pred_before` is false AND `pred_after` is true: insert new index entry
4. If `pred_before` is true AND `pred_after` is false: delete index entry
5. If both true: entry remains (key may have changed; update B-tree as normal)
6. If both false: no index entry exists; do nothing

**On DELETE:**

- Remove index entries using row key (no predicate evaluation needed)

**On UPSERT (INSERT ... ON CONFLICT ... DO UPDATE):**

1. For the INSERT portion: evaluate predicate and index accordingly
2. For the UPDATE portion: evaluate before (existing row) and after (new values); update index entries as per UPDATE rules above
3. If a row leaves the predicate due to UPDATE, its index entry is deleted
4. If a row enters the predicate due to UPDATE, a new index entry is inserted

### UNIQUE Partial Index Semantics

**Critical semantic difference from regular UNIQUE indexes:**

For `CREATE UNIQUE INDEX ... WHERE predicate`, the unique constraint **applies only to indexed rows**. This has important implications:

```
Table t: (id INTEGER PRIMARY KEY, key TEXT, status TEXT)
Data:
  Row 1: (1, 'abc', 'revoked')
  Row 2: (2, 'abc', 'active')

Partial index: CREATE UNIQUE INDEX idx_key ON t(key) WHERE status = 'active'

-- Row 1 is NOT indexed (status = 'revoked')
-- Row 2 IS indexed (status = 'active')
-- Unique constraint checks only indexed rows
-- No violation reported (key 'abc' appears once in active rows)
```

**Applications MUST NOT assume that a key returned by a partial unique index is globally unique.** Only uniqueness among indexed (matching predicate) rows is guaranteed.

**Predicate Immutability Requirement (Phase 1):**

Phase 1 does NOT implement trigger-based duplicate detection. Therefore:

**`CREATE UNIQUE INDEX ... WHERE predicate` requires an application-level monotonicity guarantee.** The key requirement is: **once a row ceases to match the predicate, it must never re-enter the predicate while retaining the same key value.** If this guarantee is violated, the UNIQUE constraint may be silently violated.

**Why monotonicity matters:**

Consider `WHERE tenant_id = 1`:

1. Row created with tenant_id=1 → matches predicate → indexed
2. Row updated to tenant_id=2 → no longer matches predicate → NOT indexed (correct)
3. Row later updated back to tenant_id=1 → matches predicate again → RE-ENTERED into index
4. If another row with the same key has tenant_id=1 and never left the index, we have a duplicate

The guarantee must prevent step 3 from occurring: once a row leaves the predicate (for a given key), it must never re-enter.

**Examples of predicates that satisfy the monotonicity guarantee:**

| Predicate                  | Monotonicity Guarantee                                                             | Why it works                                                              |
| -------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `WHERE status = 'active'`  | Once status becomes anything other than 'active', it will never return to 'active' | Status transitions are one-way: active → revoked (never back)             |
| `WHERE tenant_id = 1`      | Once tenant_id is changed from 1 to anything else, it will never return to 1       | Application enforces tenant reassignment is permanent (1→2→1 not allowed) |
| `WHERE deleted_at IS NULL` | Once deleted_at is set (row is "deleted"), the row is physically deleted           | Deleted rows cannot be undeleted                                          |
| `WHERE key IS NOT NULL`    | Once key becomes NULL, it stays NULL                                               | NULL is the final state; never transitions back                           |

**Counterexamples (NOT monotonic, NOT allowed for UNIQUE):**

| Predicate                                   | Why NOT monotonic                       | Problem                                                      |
| ------------------------------------------- | --------------------------------------- | ------------------------------------------------------------ |
| `WHERE balance > 0`                         | Balance can fluctuate above and below 0 | Row can leave (balance=0) and re-enter (balance>0)           |
| `WHERE expires_at > TIMESTAMP '2026-01-01'` | Row's expires_at value can change       | Row can leave and re-enter predicate as column value changes |
| `WHERE status IN ('active', 'pending')`     | Status can cycle through values         | Row can leave (active→revoked) and re-enter (pending)        |

**What the implementation checks (Phase 1):**

The implementation checks that the predicate does not use operators that typically indicate mutability:

| Operator                     | Check Result                                                        |
| ---------------------------- | ------------------------------------------------------------------- |
| `=`, `IN` with constant list | Allowed (constant comparison)                                       |
| `IS NULL`, `IS NOT NULL`     | Allowed (nullness check)                                            |
| `>`, `>=`, `<`, `<=`         | Rejected: indicates range/inequality comparison                     |
| `!=`                         | Rejected: row can enter and leave predicate, violating monotonicity |
| `LIKE`, `ILIKE`              | Rejected: indicates pattern matching                                |

**What the implementation does NOT check:**

- Whether a column can be updated after row creation
- Whether application logic will actually maintain the immutability guarantee
- Whether `WHERE tenant_id = 1` is actually immutable (application must guarantee)

**Phase 2 Enhancement:** A trigger-based mechanism will be added to enforce that no two non-indexed rows have duplicate key values.

**Special case: `WHERE key IS NOT NULL`:**

This pattern is **allowed for UNIQUE partial indexes** because:

- NULL is treated specially in SQL UNIQUE constraints (multiple NULLs are allowed)
- If a key transitions from non-NULL to NULL, the row simply leaves the index
- The UNIQUE constraint remains valid for all non-NULL keys

```sql
CREATE UNIQUE INDEX idx_key ON t(key) WHERE key IS NOT NULL;
-- Ensures uniqueness among non-null keys; allows multiple NULL keys
```

### Canonicalization Algorithm

To ensure predicate matching is deterministic, predicates are canonicalized before hashing:

**Canonicalization rules (applied recursively):**

1. **BETWEEN expansion:** First expand BETWEEN to AND of >= and <=
   - `col BETWEEN 1 AND 10` → `col >= 1 AND col <= 10`

2. **AND/OR sorting:** Sort AND/OR children by column name, then by operator precedence, then by value
   - `b = 1 AND a = 1` → `a = 1 AND b = 1`
   - `b = 1 OR a = 1 OR c = 1` → `a = 1 OR b = 1 OR c = 1`
   - **Operator precedence for sorting:** `LT < LTE < EQ < NE < GTE < GT` (sorted by enum value or explicit order)

3. **NOT normalization (De Morgan's laws):**
   - `NOT (a OR b)` → `(NOT a) AND (NOT b)`
   - `NOT (a AND b)` → `(NOT a) OR (NOT b)`
   - `NOT (a = b)` → `a != b` (normalized comparison)
   - `NOT (a != b)` → `a = b`

4. **Comparison normalization:** Always put constant on right
   - `5 < col` → `col > 5` (constant right: `col > 5`)
   - `col = 5` is already canonical (constant right)
   - `col != 5` is already canonical (`!=` has constant on right; no normalization needed)

5. **IN list normalization:** Single-item IN normalizes to equality; multi-item IN sorts and deduplicates
   - `col IN (1)` → `col = 1`
   - `col IN (3, 1, 2, 1, 3)` → `col IN (1, 2, 3)`

6. **Null comparison normalization:**
   - `NOT (col IS NULL)` → `col IS NOT NULL`
   - `NOT (col IS NOT NULL)` → `col IS NULL`

7. **Type coercion normalization:**
   - When comparing column to literal, coerce literal to column's data type
   - `col = 0` where `col` is INTEGER → canonical: `col = 0`
   - `col = 0` where `col` is BIGINT → canonical: `col = BIGINT '0'`
   - Rationale: Same logical predicate on different types must produce different hashes to prevent false equivalence
   - **Implementation:** Canonicalization must receive table schema (column types) as context. Both query predicates and index predicates must be canonicalized against the same column types to produce matching hashes.

8. **BETWEEN bounds validation and normalization:**
   - If lower > upper (e.g., `col BETWEEN 10 AND 5`): normalize to impossible predicate (constant false). The index will always be empty.
   - If lower == upper (e.g., `col BETWEEN 5 AND 5`): collapse to equality `col = lower` (per rule 5, single-item IN → EQ)
   - Rationale: Invalid ranges should not cause errors at CREATE INDEX time since the predicate itself is syntactically valid; canonicalization handles them as empty or EQ.

**Order of operations:** BETWEEN is expanded first (step 1), then the resulting AND is sorted along with all other AND predicates (step 2). This ensures `col BETWEEN 1 AND 10` canonicalizes identically regardless of how it was originally expressed.

**Canonical form serialization:**

The canonical form is serialized using rkyv, then hashed with SHA-256.

### Query Planning

**Phase 1: Exact Predicate Matching Only**

In Phase 1, index selection requires exact predicate equivalence:

```rust
pub fn select_index(table: &Table, query_predicate: &Expression) -> Option<Index> {
    let query_hash = canonicalize_predicate(query_predicate, &table.schema).hash;
    for index in &table.indexes {
        if let Some(idx_hash) = index.partial_index_hash {
            if idx_hash == query_hash {
                return Some(index); // Exact match
            }
        }
    }
    None // No partial index matches
}
```

**Critical: Type coercion requires table schema.** Canonicalization must receive the table schema to apply type coercion correctly. The query predicate `col = 0` (INTEGER literal) and index predicate `col = BIGINT '0'` produce different hashes unless both are canonicalized against the column's actual type. Without schema context, the query hash would not match the index hash and the index would not be used.

**Index-only scan condition (Phase 2):** If the query predicate exactly matches the index predicate AND all predicate columns are indexed columns, no re-evaluation is needed. **Note:** Phase 1 requires exact match for index selection; index-only scan optimization is implemented in Phase 2.

**Queries that do NOT use partial index in Phase 1:**

- `WHERE status = 0 AND type = 'a'` when index is `WHERE status = 0` (superset query)
- `WHERE status = 0` when index is `WHERE status = 0 AND type = 'a'` (subset query)
- `WHERE status = 0` when index is `WHERE status = 0 AND deleted_at IS NULL` (partial overlap)

**Phase 2 (future):** Implication-based selection will enable superset/subset matching with post-filtering.

**Why Phase 1 is intentionally limited:**

Exact matching is:

1. Simple to implement and verify
2. Deterministic (no false positives/negatives from complex implication logic)
3. Sufficient for RFC-0903 use case (`WHERE revoked = 0` queries use `WHERE revoked = 0` indexes)

**Implication algorithm (Phase 2):**

A query predicate Q **implies** an index predicate P if all rows satisfying Q also satisfy P.

| Q (query)                   | P (index)                   | Relationship | Action                                                                             |
| --------------------------- | --------------------------- | ------------ | ---------------------------------------------------------------------------------- |
| `status = 0`                | `status = 0`                | Q ≡ P        | Index-only scan                                                                    |
| `status = 0 AND type = 'a'` | `status = 0`                | Q ⊇ P        | Index scan + post-filter on `type = 'a'`                                           |
| `status = 0`                | `status = 0 AND type = 'a'` | Q ⊅ P        | No index (index predicate adds constraints not in query; using it would omit rows) |
| `status = 0 AND type = 'a'` | `status = 0 AND type = 'a'` | Q ≡ P        | Index-only scan                                                                    |
| `status = 0 AND type = 'a'` | `status = 0 AND type = 'b'` | Q ⊅ P        | No index (disjoint)                                                                |

### Error Handling

| Code   | Condition                                            | Handling                                                                                                                                                                                                                                                                                                                                                                          |
| ------ | ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `E001` | Predicate references non-existent column             | Parser error: "column 'x' does not exist in table 't'"                                                                                                                                                                                                                                                                                                                            |
| `E002` | Predicate contains subquery                          | Parser error: "subqueries not allowed in partial index predicates"                                                                                                                                                                                                                                                                                                                |
| `E003` | Predicate contains function call                     | Parser error: "function calls not allowed in partial index predicates"                                                                                                                                                                                                                                                                                                            |
| `E004` | Predicate exceeds depth limit                        | Parser error: "predicate too complex (max depth 20)"                                                                                                                                                                                                                                                                                                                              |
| `E005` | Predicate IN list too large                          | Parser error: "IN list exceeds 32 items"                                                                                                                                                                                                                                                                                                                                          |
| `E006` | Predicate serialization fails                        | Internal error: "predicate encoding failed"                                                                                                                                                                                                                                                                                                                                       |
| `E007` | Predicate evaluation error                           | DML: operation succeeds, row not indexed, warning logged; Query: return error. **For UNIQUE partial indexes:** DML operation fails and is rolled back — a row that cannot be evaluated for the predicate cannot be indexed, and allowing it to exist would risk silent uniqueness violations (e.g., two rows with same key both fail predicate evaluation but succeed as INSERT). |
| `E008` | Unique partial index with mutable predicate          | Parser error: "predicate contains mutable operator ('>', '>=', '<', '<=', '!=')"                                                                                                                                                                                                                                                                                                  |
| `E009` | Predicate contains EXISTS                            | Parser error: "EXISTS not allowed in partial index predicates"                                                                                                                                                                                                                                                                                                                    |
| `E010` | Predicate contains LIKE/ILIKE                        | Parser error: "LIKE/ILIKE not allowed in partial index predicates"                                                                                                                                                                                                                                                                                                                |
| `E011` | Multiple BETWEEN on same column                      | Parser error: "multiple BETWEEN on same column in predicate"                                                                                                                                                                                                                                                                                                                      |
| `E012` | Unsupported serialization format version             | Runtime error: "predicate format version not supported"                                                                                                                                                                                                                                                                                                                           |
| `W001` | IF NOT EXISTS: index exists with different predicate | Success with warning: "index 'idx' already exists with different predicate"                                                                                                                                                                                                                                                                                                       |

### Transactional Consistency

Partial index maintenance is atomic with DML operations:

| Property        | Behavior                                                                                                       |
| --------------- | -------------------------------------------------------------------------------------------------------------- |
| **Atomicity**   | INSERT/UPDATE/DELETE and corresponding index changes are atomic; either both succeed or both fail              |
| **Isolation**   | Default isolation level applies; partial index changes are visible only after transaction commits              |
| **Recovery**    | After crash, index is rebuilt by scanning table and evaluating predicates; no special recovery needed          |
| **WAL logging** | Partial index changes are logged as part of normal DML WAL entries; predicate is stored separately in metadata |

**Index rebuild process after recovery:**

1. Load `IndexMetadata` for all indexes on the table
2. For each partial index, read `partial_index_predicate` (check version byte)
3. Deserialize predicate expression
4. Scan table sequentially (full table scan required to ensure all rows are evaluated against current column values)
5. For each row, evaluate predicate using current row values; if true, insert into partial index
6. This is the same as `CREATE INDEX ... WHERE` starting from existing data

**Recovery semantics:** The rebuild uses current row values from the table (not a historical state). If WAL entries are unflushed at crash time, the table reflects the committed state; any inconsistency between index and table is resolved by the rebuild. After rebuild completes, the partial index is consistent with the table.

**Recommended recovery implementation:** The implementation SHOULD log rebuild progress periodically (e.g., every 100,000 rows) to a persistent checkpoint marker (separate from WAL). On restart, if a partial index is detected as incomplete (checkpoint marker exists but index is not fully populated), the rebuild SHOULD resume from the last checkpoint rather than restarting from row 1. A partial index is considered incomplete if predicate evaluation on sample rows shows mismatch rate significantly different from expected selectivity. Checkpoint persistence uses a sidecar file (not WAL) to avoid WAL pollution during bulk rebuilds.

**Note:** For large tables, index rebuild may take significant time. This is expected behavior.

**On E007 (predicate evaluation error):**

For **non-UNIQUE partial indexes**: The DML operation succeeds but the row is not indexed. A warning is logged with details of the evaluation failure. The row remains accessible via table scan but will not be found via the partial index. This enables applications to recover data even if predicate evaluation fails for edge cases (e.g., type coercion issues).

For **UNIQUE partial indexes**: The DML operation fails and is rolled back. A predicate evaluation error on a UNIQUE partial index indicates a data integrity risk — allowing the operation to succeed would risk silent uniqueness violations if another row with the same key also fails predicate evaluation. Applications must resolve the underlying issue (e.g., fix type mismatches) before retrying.

### Column and Table Rename Impact

**Column rename:**

If a column referenced in a partial index predicate is renamed:

```sql
CREATE INDEX idx ON t(c) WHERE status = 0;
ALTER TABLE t RENAME COLUMN status TO active_status;
```

The index becomes invalid. The predicate now references non-existent column `status`.

**This is expected behavior.** Partial indexes reference column names directly in their predicates. When a column is renamed, the index must be dropped and recreated with the new column name.

**Post-rename DML behavior:** After column rename, any DML operation that would evaluate the partial index predicate (INSERT, UPDATE, DELETE) returns error E001 (COLUMN_NOT_FOUND). The application must execute `DROP INDEX idx` and `CREATE INDEX idx ON t(c) WHERE status = 0` (with new column name) before normal DML resumes.

**Design rationale:** This is a consequence of storing column names (not column IDs) in predicates. An alternative design using column IDs would avoid this issue but adds complexity and is out of scope for Phase 1. **Future work (F5):** `ALTER INDEX ... RENAME COLUMN` syntax would allow renaming columns in predicates without dropping/recreating indexes.

**Recommendation:** Applications using partial indexes SHOULD NOT rename columns that appear in partial index predicates. For UNIQUE partial indexes, column rename blocks ALL DML on the table until the index is fixed — this can cause production downtime. Schema migration tools MUST detect partial index dependencies before renaming columns.

**Table rename:**

If a table with partial indexes is renamed:

```sql
CREATE INDEX idx ON old_name(c) WHERE status = 0;
ALTER TABLE old_name RENAME TO new_name;
```

The index remains valid. The `IndexMetadata.table_name` is updated to `new_name` by the `ALTER TABLE` rename operation. The predicate references columns by name, not table name.

### Determinism Requirements

**This RFC affects Class A (Protocol Deterministic) code paths:**

- Predicate canonicalization must be deterministic across restarts
- Predicate hashing must be deterministic (SHA-256 of canonical bytes)
- Index entry presence/absence must be deterministic for identical row states
- **Time-dependent predicates are explicitly prohibited** — they would violate determinism

**Implication:** The canonicalization algorithm must produce identical output on all implementations and restarts. Seeded by the RFC specification, not by runtime state.

**Why now() is prohibited:**

`now()` returns a non-deterministic value (current time). Two identical databases at different times would produce different index contents, violating reproducibility requirements.

## Performance Targets

| Metric                     | Target                        | Measurement                           | Notes                                                      |
| -------------------------- | ----------------------------- | ------------------------------------- | ---------------------------------------------------------- |
| Index write overhead       | <15% vs full index            | Predicate eval on INSERT/UPDATE       | O(columns in predicate)                                    |
| Index storage              | Proportional to matching rows | Bytes per indexed row                 | ~0 bytes overhead (absence = inactive)                     |
| Query speedup              | 10x-100x                      | Queries matching selective predicates | Only matching rows scanned                                 |
| Predicate canonicalization | <1ms                          | Per predicate                         | SHA-256 + sort operations                                  |
| Query planning lookup      | O(indexes)                    | Hash comparison per index             | Pre-filter candidate indexes                               |
| Scan filtering             | O(predicate eval)             | Per candidate row                     | Re-evaluate predicate on each row (except index-only scan) |

**Index-only scan (Phase 2):** When the partial index covers all query columns and predicate exactly matches, scan requires zero re-evaluation — the index guarantees predicate satisfaction.

**Storage savings estimate:**

For RFC-0903 `api_keys` table with 10% revocation rate:

- Full index: 100% of rows
- Partial index `WHERE revoked = 0`: 90% of rows
- Savings: 10% storage + 10% write overhead reduction

## Security Considerations

### Consensus Attacks

| Threat                           | Impact   | Mitigation                                                                  |
| -------------------------------- | -------- | --------------------------------------------------------------------------- |
| Predicate injection via SQL      | High     | Parser enforces allowed expression types only; no raw string evaluation     |
| Maliciously complex predicate    | Medium   | Stack depth limit (20), term count limit (32), serialized size limit (64KB) |
| Hash collision in predicate_hash | Very Low | SHA-256 32-byte hash; collision probability ~2^-256                         |
| DoS via predicate evaluation     | Medium   | Per-row evaluation is bounded O(1) per column; timeouts on queries          |

### Economic Exploits

- Not applicable — partial indexes are an internal storage optimization, not an economic mechanism

### Proof Forgery / Replay Attacks

- Not applicable — partial indexes are local storage, not consensus-relevant

### Determinism Violations

| Violation Path                           | Mitigation                                                                   |
| ---------------------------------------- | ---------------------------------------------------------------------------- |
| Non-deterministic canonicalization       | Algorithm specified in RFC; test vectors enforce determinism                 |
| Platform-dependent expression evaluation | Expression evaluator already platform-consistent (per RFC-0104)              |
| Time-dependent predicates                | `now()` etc. explicitly unsupported; would produce non-deterministic results |

## Adversarial Review

### Failure Modes and Mitigations

| Failure Mode                                         | Probability | Impact                          | Mitigation                                                                                                                                                                                                                                                                                                                       |
| ---------------------------------------------------- | ----------- | ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Predicate canonicalization bug                       | Low         | Index selects wrong entries     | Test vectors for all canonicalization rules                                                                                                                                                                                                                                                                                      |
| Hash collision                                       | Negligible  | False positive index match      | Acceptable risk; 2^-256 probability                                                                                                                                                                                                                                                                                              |
| Predicate evaluation error on edge case              | Medium      | Row not indexed                 | E007: operation succeeds, warning logged, row accessible via table scan                                                                                                                                                                                                                                                          |
| UNIQUE partial index with duplicate non-indexed keys | Medium      | Silent uniqueness violation     | Phase 1 requires application-level immutability guarantee; Phase 2 triggers                                                                                                                                                                                                                                                      |
| Partial index not used due to complex query          | High        | Performance regression          | Phase 1 requires exact match; Phase 2 adds implication logic                                                                                                                                                                                                                                                                     |
| Serialization format version mismatch                | Low         | Index unusable after upgrade    | Version header allows graceful error; user can recreate index                                                                                                                                                                                                                                                                    |
| Column renamed without index update                  | Low         | Index becomes invalid           | Applications must drop/recreate dependent indexes                                                                                                                                                                                                                                                                                |
| Crash during index rebuild                           | Low         | Partial index may be incomplete | Rebuild entries are not WAL-logged (for performance); however, a checkpoint marker (last processed row ID/page) is persisted to a sidecar file. On next access, if checkpoint exists but index is incomplete, resume from checkpoint. Detection uses predicate re-evaluation on sample rows rather than simple count comparison. |

### Predicate Rejection Validation

The parser must validate predicates at CREATE INDEX time:

1. **Type check:** All column references must exist in the target table
2. **Allowed expression types:** Only simple comparisons, NULL checks, IN lists, BETWEEN, AND/OR/NOT
3. **No function calls:** Reject `now()`, `current_timestamp`, `current_date`, `length()`, etc.
4. **No subqueries:** Reject `EXISTS`, `IN (SELECT ...)`, scalar subqueries
5. **No EXISTS:** Reject `EXISTS (SELECT ...)`
6. **No LIKE/ILIKE:** Reject `col LIKE 'pattern'`
7. **No row constructors:** Reject `(a, b) IN (...)`
8. **Complexity limits:** Depth ≤ 20, terms ≤ 32, IN list ≤ 32, BETWEEN ranges ≤ 1 per column, serialized size ≤ 64KB
9. **UNIQUE predicate operators:** For UNIQUE partial indexes, reject operators `>`, `>=`, `<`, `<=`, `!=`, `LIKE`, `ILIKE`. The `!=` (NE) operator is rejected because a row can enter the predicate (e.g., `WHERE status != 'deleted'` when status = 'active') and later leave (status becomes 'deleted'), violating the monotonicity guarantee.

## Economic Analysis

Not applicable — this is a storage optimization, not an economic mechanism.

## Compatibility

### Backward Compatibility

- **Existing indexes:** Continue to work without modification — `where_clause = None` and `partial_index_hash = None` means full index
- **Existing queries:** No changes required for queries without partial indexes
- **Existing storage format:** `IndexMetadata` additions are optional fields; existing indexes load with `partial_index_hash = None`
- **Migration:** No schema migration needed; partial indexes are purely additive

### Forward Compatibility

- **Old clients accessing new databases:** Old clients reading `IndexMetadata` with new fields see default values (None); partial indexes are invisible to old clients (no index selection)
- **New clients accessing old databases:** Old databases have no partial index fields; behavior unchanged

### Index Type Compatibility

- **B-tree indexes:** Fully compatible with partial indexes
- **HNSW/vector indexes:** Partial index support is **out of scope** for Phase 1; HNSW indexes must not be created with WHERE clauses

**Rationale:** HNSW indexes have different physical structure and search semantics. Adding partial support would require separate design. A separate RFC (RFC-0202) should address vector partial indexes.

## Test Vectors

### Positive Test Cases

| Test | SQL                                                                              | Expected                                                                   |
| ---- | -------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| T01  | `CREATE INDEX idx_active ON t(c) WHERE status = 0`                               | Index created; `where_clause = status = 0`; `partial_index_hash` populated |
| T02  | `CREATE UNIQUE INDEX idx_active ON t(c) WHERE active = 1`                        | Unique partial index created (predicate is immutable)                      |
| T03  | `INSERT INTO t VALUES (1, 0)` → `SELECT * FROM t WHERE c = 1`                    | Index entry written for row (1, 0)                                         |
| T04  | `INSERT INTO t VALUES (1, 1)`                                                    | Row (1, 1) inserted; no index entry written (status = 1)                   |
| T05  | `UPDATE t SET status = 1 WHERE c = 1`                                            | Index entry deleted for row where c=1                                      |
| T06  | `UPDATE t SET c = 2 WHERE c = 1`                                                 | Index entry updated (old deleted, new inserted)                            |
| T07  | `SELECT * FROM t WHERE status = 0`                                               | Uses partial index                                                         |
| T08  | `DELETE FROM t WHERE c = 1`                                                      | Index entry removed                                                        |
| T09  | `DROP INDEX idx_active`                                                          | Partial index dropped cleanly                                              |
| T10  | Display round-trip                                                               | `CREATE INDEX idx ON t(c) WHERE x > 5` → Display → same SQL                |
| T11  | `CREATE INDEX idx ON t(c) WHERE a BETWEEN 1 AND 10`                              | Index created with BETWEEN predicate                                       |
| T12  | `CREATE INDEX IF NOT EXISTS idx ON t(c) WHERE p` (idx exists with identical p)   | Succeeds silently                                                          |
| T13  | `CREATE INDEX IF NOT EXISTS idx ON t(c) WHERE p1` (idx exists with different p2) | Succeeds with W001 warning                                                 |
| T14  | `CREATE UNIQUE INDEX idx ON t(c) WHERE c IS NOT NULL`                            | Unique partial index created for non-null keys                             |

### Negative Test Cases

| Test | SQL                                                                                         | Expected Error                                                                                    |
| ---- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| N01  | `CREATE INDEX idx ON t(c) WHERE no_such_col = 0`                                            | E001: column 'no_such_col' does not exist                                                         |
| N02  | `CREATE INDEX idx ON t(c) WHERE c = (SELECT 1)`                                             | E002: subqueries not allowed                                                                      |
| N03  | `CREATE INDEX idx ON t(c) WHERE c > current_timestamp`                                      | E003: function calls not allowed                                                                  |
| N04  | `CREATE INDEX idx ON t(c) WHERE c IN (1, 2, ..., 100)` (100 items)                          | E005: IN list exceeds 32 items                                                                    |
| N05  | `CREATE INDEX idx ON t(c) WHERE a > 1 AND b > 2 AND c > 3 AND d > 4 AND ...` (33 AND terms) | E004: predicate too complex                                                                       |
| N06  | `CREATE INDEX idx ON t(c) WHERE (a = 1 AND b = 2) OR (c = 3 AND d = 4)` (complex nesting)   | E004: predicate too deep                                                                          |
| N07  | `CREATE INDEX idx ON t(c) WHERE a = 1 AND a = 2`                                            | Valid (index will be empty, but no error)                                                         |
| N08  | `CREATE INDEX idx ON t(c) WHERE 1 = 0`                                                      | Valid (index always empty)                                                                        |
| N09  | `CREATE INDEX idx ON t(c) WHERE 1 = 1`                                                      | Valid (index includes all rows, acts as full index)                                               |
| N10  | `SELECT * FROM t WHERE status = 0` when index is `WHERE status = 0 AND type = 'a'`          | No index (Phase 1: partial overlap not matched)                                                   |
| N11  | `CREATE INDEX idx ON t(c) WHERE c LIKE 'prefix%'`                                           | E010: LIKE not allowed                                                                            |
| N12  | `CREATE INDEX idx ON t(c) WHERE EXISTS (SELECT 1)`                                          | E009: EXISTS not allowed                                                                          |
| N13  | `CREATE UNIQUE INDEX idx ON t(c) WHERE balance > 0`                                         | E008: predicate contains mutable operator                                                         |
| N14  | `CREATE INDEX idx ON t(c) WHERE a BETWEEN 1 AND 2 AND b BETWEEN 3 AND 4`                    | Valid (two independent BETWEEN on different columns; ranges on different columns do not interact) |
| N14a | `CREATE INDEX idx ON t(c) WHERE a BETWEEN 1 AND 5 AND a BETWEEN 3 AND 7`                    | E011: overlapping BETWEEN on same column                                                          |
| N15  | `CREATE UNIQUE INDEX idx ON t(c) WHERE status IN ('active', 'pending')`                     | E008: IN with multiple values not allowed for UNIQUE (cycling not verifiable)                     |
| N16  | `CREATE UNIQUE INDEX idx ON t(c) WHERE status != 'deleted'`                                 | E008: predicate contains mutable operator (`!=` rejected for UNIQUE)                              |

### Canonicalization Test Cases

| Input                                          | Canonical Output                                    |
| ---------------------------------------------- | --------------------------------------------------- |
| `b = 1 AND a = 1`                              | `a = 1 AND b = 1`                                   |
| `c IN (3, 1, 2, 1)`                            | `c IN (1, 2, 3)`                                    |
| `c IN (5)`                                     | `c = 5` (single-item IN normalizes to EQ)           |
| `5 < col` (col > 5)                            | `col > 5`                                           |
| `col != 5`                                     | `col != 5` (already canonical: constant on right)   |
| `NOT (a IS NULL)`                              | `a IS NOT NULL`                                     |
| `NOT (a = 1 OR b = 2)`                         | `(a != 1) AND (b != 2)`                             |
| `NOT ((a = 1 AND b = 2) OR (a = 3 AND b = 4))` | `(a != 1 OR b != 2) AND (a != 3 OR b != 4)`         |
| `b = 1 OR a = 1`                               | `a = 1 OR b = 1` (sorted by column name)            |
| `col BETWEEN 1 AND 10`                         | `(col >= 1) AND (col <= 10)` (expanded, sorted)     |
| `col BETWEEN 5 AND 5`                          | `col = 5` (degenerate BETWEEN collapses to EQ)      |
| `col BETWEEN 10 AND 5`                         | `Constant(False)` (reversed bounds → impossible)    |
| `col = 0` (BIGINT column, INTEGER literal)     | `col = BIGINT '0'` (type coerced to column type)    |
| `a >= 5 AND a <= 5`                            | `a >= 5 AND a <= 5` (sorted by operator precedence) |

### Index Selection Test Cases (Phase 1)

| Query Predicate             | Partial Index Predicate     | Index Used? | Notes                                            |
| --------------------------- | --------------------------- | ----------- | ------------------------------------------------ |
| `status = 0`                | `status = 0`                | Yes         | Exact match (Phase 1); index-only scan (Phase 2) |
| `status = 0 AND type = 'a'` | `status = 0`                | **No**      | Phase 1: superset not matched                    |
| `status = 0 AND type = 'a'` | `status = 0 AND type = 'a'` | Yes         | Exact match; index-only scan                     |
| `status = 0`                | `status = 0 AND type = 'a'` | **No**      | Phase 1: subset not matched                      |
| `status = 0 AND type = 'a'` | `status = 0 AND type = 'b'` | **No**      | Disjoint predicates                              |

### Concurrency Test Cases

| Test | Scenario                                                              | Expected                                                                                                                                                                                                                      |
| ---- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| C01  | Concurrent INSERT while CREATE INDEX running                          | Index includes all committed inserts at time of scan                                                                                                                                                                          |
| C02  | UPDATE changes predicate from true→false during concurrent scan       | Scan may read entry before delete completes (isolation level). If row is re-evaluated, predicate is false and row excluded from results. If not re-evaluated, row may appear in results (acceptable under default isolation). |
| C03  | Two partial indexes on same table, different predicates               | Each index maintained independently based on its predicate                                                                                                                                                                    |
| C04  | Unique partial index: two rows with same key, one revoked, one active | Only active row indexed; no violation reported (Phase 1, application guarantee)                                                                                                                                               |
| C05  | Column renamed without dropping index                                 | Index becomes invalid; subsequent DML on index returns error                                                                                                                                                                  |
| C06  | UPSERT: INSERT new row matching index                                 | Index entry inserted                                                                                                                                                                                                          |
| C07  | UPSERT: UPDATE existing row leaving predicate                         | Index entry deleted                                                                                                                                                                                                           |
| C08  | UPSERT: UPDATE existing row entering predicate                        | Index entry inserted                                                                                                                                                                                                          |

**Concurrency note:** Partial index maintenance (predicate re-evaluation during INSERT/UPDATE/DELETE) follows standard MVCC isolation semantics. Under the default isolation level, a scan sees all rows committed before the scan began. If a row's predicate state changes (e.g., `status` changes from `'active'` to `'revoked'`) during a concurrent transaction, the index maintenance operation sees the committed state at the time of evaluation. For UNIQUE partial indexes, Phase 1 relies on application-level guarantees; concurrent transactions that violate uniqueness will not be blocked by the index itself but will be detected and rejected by the application layer.

| Test | SQL                                                                                                        | Expected                                          |
| ---- | ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| M01  | `CREATE INDEX idx1 ON t(c) WHERE status = 'active'` + `CREATE INDEX idx2 ON t(c) WHERE status = 'revoked'` | Both indexes created                              |
| M02  | INSERT `(1, 'key1', 'active')`                                                                             | Indexed in idx1 only                              |
| M03  | INSERT `(2, 'key1', 'revoked')`                                                                            | Indexed in idx2 only                              |
| M04  | UPDATE row 1 SET status = 'revoked' WHERE c = 1                                                            | Entry deleted from idx1, inserted to idx2         |
| M05  | UPDATE row 1 SET key = 'key2' WHERE c = 1                                                                  | Entry updated in idx1 (old deleted, new inserted) |

### Overlapping Partial Index Test

A single row can be indexed in multiple partial indexes if it matches multiple predicates.

| Test | SQL                                                                                                   | Expected                                                      |
| ---- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| O01  | `CREATE INDEX idx1 ON t(c) WHERE status = 'active'` + `CREATE INDEX idx2 ON t(c) WHERE tenant_id = 1` | Both indexes created                                          |
| O02  | INSERT `(1, 'key1', 'active', 1)`                                                                     | Indexed in BOTH idx1 (status='active') AND idx2 (tenant_id=1) |
| O03  | UPDATE row 1 SET status = 'revoked' WHERE c = 1                                                       | Entry deleted from idx1 only; remains in idx2                 |
| O04  | INSERT `(2, 'key2', 'revoked', 1)`                                                                    | Indexed in idx2 only (status='revoked', not in idx1)          |
| O05  | INSERT `(3, 'key3', 'active', 2)`                                                                     | Indexed in idx1 only (tenant_id=2, not in idx2)               |

### Unique Partial Index Duplicate Test

| Test | Scenario                                                                                                          | Expected                                                                                                                                                                                    |
| ---- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D01  | Two rows with key='abc', both with status='active'                                                                | UNIQUE partial index violation at INSERT of second row                                                                                                                                      |
| D02  | One row in index (key='abc', status='active'), second row same key but status='revoked'                           | No violation; second row not indexed                                                                                                                                                        |
| D03  | One row in index (key='abc', status='active'), second row updated from status='revoked' to 'active' with same key | **Phase 1:** No enforcement; application guarantee relied upon — UPDATE succeeds silently and uniqueness may be violated. **Phase 2:** Trigger-based detection returns error at UPDATE time |
| D04  | Row leaves predicate (status='revoked'), later re-enters (status='active') with same key                          | Allowed; first departure deleted index entry, re-entry inserts new entry                                                                                                                    |

### Display Round-Trip Test Cases

| Input SQL                                             | Displayed SQL                                                                           |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `CREATE INDEX idx ON t(c) WHERE status = 0`           | `CREATE INDEX idx ON t(c) WHERE status = 0`                                             |
| `CREATE UNIQUE INDEX idx ON t(c) WHERE active = 1`    | `CREATE UNIQUE INDEX idx ON t(c) WHERE active = 1`                                      |
| `CREATE INDEX idx ON t(c) WHERE x > 5`                | `CREATE INDEX idx ON t(c) WHERE x > 5`                                                  |
| `CREATE INDEX idx ON t(c) WHERE col != 5`             | `CREATE INDEX idx ON t(c) WHERE col != 5`                                               |
| `CREATE INDEX idx ON t(c) WHERE col BETWEEN 1 AND 10` | `CREATE INDEX idx ON t(c) WHERE col BETWEEN 1 AND 10` (BETWEEN preserved, not expanded) |

**Note:** Display preserves the original WHERE clause structure as stored in the AST. BETWEEN is stored as Between expression and displayed as `BETWEEN ... AND ...`; it is NOT expanded to AND during display. Users see the same form they entered. This is a round-trip guarantee: parsing + display produces equivalent SQL.

## Alternatives Considered

| Alternative                        | Pros                  | Cons                                   | Chosen? |
| ---------------------------------- | --------------------- | -------------------------------------- | ------- |
| PostgreSQL-style WHERE clause      | Familiar syntax       | Complex parser                         | ✅      |
| Store predicate per-entry          | Simple implementation | 2x storage overhead                    | ❌      |
| Bitmap-based active flag           | Fast toggle on update | Extra space, complex management        | ❌      |
| Predicate evaluation on every scan | No storage overhead   | O(n) scan cost, defeats purpose        | ❌      |
| Separate active/inactive tables    | Clear separation      | Schema changes required                | ❌      |
| Materialized view instead of index | More flexible         | Not true indexes, eventual consistency | ❌      |
| Application-level filtering        | No SQL changes        | Fragile, error-prone                   | ❌      |

## Implementation Phases

### Phase 1: Core (Minimal Viable Product)

**Acceptance criteria:**

- [ ] `CREATE INDEX ... WHERE` parses without error
- [ ] Partial index metadata stored in IndexMetadata with `partial_index_hash` and `partial_index_predicate`
- [ ] INSERT evaluates predicate and skips indexing when false
- [ ] UPDATE evaluates before/after and inserts/deletes index entries
- [ ] DELETE removes index entries
- [ ] UPSERT handles partial index maintenance correctly
- [ ] SELECT with exact predicate match uses partial index
- [ ] Display impl outputs WHERE clause correctly (BETWEEN preserved as entered, not expanded)
- [ ] All positive test cases pass
- [ ] All negative test cases produce correct errors
- [ ] `IF NOT EXISTS` behavior correct (warning on predicate mismatch)
- [ ] UNIQUE partial index restricted to immutable predicates (+ IS NOT NULL allowed)
- [ ] Serialization format includes version header

**Key files:**

| File                                      | Change                                                                                |
| ----------------------------------------- | ------------------------------------------------------------------------------------- |
| `src/parser/ast.rs`                       | Add `where_clause: Option<Expression>` to `CreateIndexStatement`; update Display impl |
| `src/parser/statements.rs`                | Parse `WHERE predicate` after WITH clause; add predicate validation                   |
| `src/storage/mvcc/persistence.rs`         | Add `partial_index_hash` and `partial_index_predicate` to `IndexMetadata`             |
| `src/executor/ddl.rs`                     | Handle `where_clause` in `execute_create_index`                                       |
| `src/executor/expression/mod.rs`          | Add `evaluate_partial_index_predicate()` function                                     |
| `src/executor/expression/canonicalize.rs` | (New) Predicate canonicalization algorithm; add to module exports                     |
| `src/planner/index_select.rs`             | Add partial index selection logic (exact match for Phase 1)                           |
| `tests/partial_index_test.rs`             | (New) Integration tests for partial indexes                                           |

### Phase 2: Enhanced (Implication-Based Selection + Triggers)

**Acceptance criteria:**

- [ ] Predicate implication algorithm implemented
- [ ] Query with superset predicate uses index with post-filter
- [ ] Query with subset predicate uses index-only scan
- [ ] `NOT` predicates handled correctly in implication
- [ ] Range predicates (`>`, `<`, `>=`, `<=`) handled in implication
- [ ] Trigger mechanism to prevent duplicate keys in non-indexed rows for UNIQUE partial indexes

**Estimated effort:** 2-3 weeks

### Phase 3: HNSW Partial Indexes (Out of Scope for Phase 1)

Requires separate RFC design for vector partial indexes (see RFC-0202).

## Key Files to Modify

| File                                      | Change                                                                                                       |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `src/parser/ast.rs`                       | Add `where_clause: Option<Expression>` to `CreateIndexStatement`; update Display impl                        |
| `src/parser/statements.rs`                | Add WHERE clause parsing in `parse_create_index_statement`; add predicate validation                         |
| `src/storage/mvcc/persistence.rs`         | Add `partial_index_hash: Option<[u8; 32]>` and `partial_index_predicate: Option<Vec<u8>>` to `IndexMetadata` |
| `src/executor/ddl.rs`                     | Pass `where_clause` to index creation; evaluate predicate on INSERT/UPDATE/DELETE/UPSERT                     |
| `src/executor/expression/mod.rs`          | Add `evaluate_partial_index_predicate()` function                                                            |
| `src/executor/expression/canonicalize.rs` | (New) Predicate canonicalization algorithm; module must be added to executor expression module exports       |
| `src/executor/expression/implication.rs`  | (New) Predicate implication checking (Phase 2)                                                               |
| `src/planner/index_select.rs`             | Add partial index selection logic                                                                            |
| `tests/partial_index_test.rs`             | (New) Integration tests for partial indexes                                                                  |

## Future Work

- **F1:** Phase 2 implication-based selection with post-filtering
- **F2:** RFC-0202: HNSW/vector partial indexes (separate RFC)
- **F3:** Parameterized partial index predicates (using constants, not runtime parameters)
- **F4:** Partial index introspection (`SELECT * FROM stoolap_indexes WHERE predicate IS NOT NULL`)
- **F5:** `ALTER INDEX ... SET WHERE predicate` for predicate changes
- **F6:** Partial index advisor (auto-suggest partial indexes based on query patterns)
- **F7:** Trigger mechanism for UNIQUE partial index duplicate detection

## Rationale

### Why This Approach?

The `where_clause: Option<Expression>` approach was chosen over alternatives because:

1. **Minimal AST change:** One new optional field on an existing struct
2. **PostgreSQL-compatible:** Familiar syntax for users coming from PostgreSQL
3. **Extensible:** Expression type already handles AND/OR/NOT for complex predicates
4. **Backward compatible:** Existing indexes have `where_clause = None`; no migration needed

### Why Not Per-Entry Metadata?

Storing `predicate_hash` and `is_active` per-entry was rejected because:

1. **Storage overhead:** 33 extra bytes per index entry
2. **Update complexity:** Need to maintain consistency across crashes
3. **Scan overhead:** Must read metadata on every scan iteration

Our approach (absence = inactive) achieves the same semantics at zero per-entry overhead.

### Why Re-Evaluate on Scans?

Re-evaluating the predicate on each candidate row during scan is necessary because:

1. **No per-entry metadata:** Index entries don't contain predicate state
2. **Dynamic state:** A row's predicate evaluation can change between INSERT/UPDATE and scan
3. **Simplicity:** No need to maintain consistent per-entry state across crashes

The overhead is acceptable because:

- Predicate evaluation is O(1) per column (simple comparisons)
- Only _candidate_ rows are re-evaluated (those returned by index lookup), not all rows
- For selective predicates, the number of candidates is small
- For index-only scans (exact predicate match, all predicate columns indexed), no re-evaluation is needed

### Why UNIQUE Partial Indexes Require Application Guarantee?

Unlike database-enforced constraints, column immutability after row creation cannot be enforced by the database alone — it requires application-level discipline. By making this an explicit application-level guarantee, we avoid the complexity of trigger-based duplicate detection while enabling the common UNIQUE partial index patterns (e.g., `WHERE tenant_id = N`).

Phase 2 adds triggers for applications that cannot guarantee immutability.

## Version History

| Version | Date       | Changes                                                                                                                                                                                                                                                                                                                      |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 13.0    | 2026-04-13 | Round 13 (external review): clarify index-only scan MVCC visibility checks, add rebuild checkpoint recommendation, add column rename mitigation note, add downgrade tooling requirement (MUST), add E007 UNIQUE-specific behavior (DML fails and rolls back)                                                                 |
| 12.0    | 2026-04-13 | Round 12 fixes: fix RFC-0912 link path from ../accepted/ to ../final/ to match Dependencies (Final) status                                                                                                                                                                                                                   |
| 11.0    | 2026-04-13 | Round 11 fixes: add planner/index_select.rs to Phase 1 key files, quote operators in E008 error message, change RFC-0912 status from Accepted to Final to match Related RFCs path                                                                                                                                            |
| 10.0    | 2026-04-13 | Round 10 fixes: fix Phase 1 key file paths (add src/ prefix), change Error column to Code for W001, remove stale RFC-0903 version number, clarify partial index rebuild detection, clarify N14 cross-column BETWEEN explanation                                                                                              |
| 9.0     | 2026-04-13 | Round 9 fixes: add W001 to Error Handling table, clarify expires_at counterexample description, fix T10 description from "full index" to generic "Display round-trip"                                                                                                                                                        |
| 8.0     | 2026-04-13 | Round 8 fixes: fix Index Selection test Phase claim, add != to operator check table, add lower>upper check in BETWEEN pseudocode, add col!=5 canonicalization test, add N16 test for != on UNIQUE, fix T10 test description, add reversed bounds BETWEEN test, clarify != already canonical, clarify C02 concurrency outcome |
| 7.0     | 2026-04-13 | Round 7 fixes: add != to grammar and operators list, fix E008 error message, add != to UNIQUE rejection list in Appendix D, clarify crash recovery rebuild, add != display round-trip test, update predicate_hash formula                                                                                                    |
| 6.0     | 2026-04-13 | Round 6 fixes: add table schema to canonicalization, add BETWEEN bounds validation, fix display acceptance criterion, clarify BETWEEN per-column limit, add NE to UNIQUE rejection, fix implication table, add Phase 2 note to index-only scan, replace N10 with partial-overlap test                                        |
| 5.0     | 2026-04-13 | Round 5 fixes: fix N14, clarify D03 Phase 1 vs Phase 2, clarify E011 same-column, update Display preserve form, fix forward compatibility, specify crash-during-rebuild recovery, add degenerate BETWEEN test case                                                                                                           |
| 4.0     | 2026-04-12 | Round 4 fixes: rewrite immutability with monotonicity guarantee, add operator precedence, add IN single-item canonicalization, add D01-D04 and O01-O05 tests, clarify serialization hard incompatibility, add N15 test                                                                                                       |
| 3.0     | 2026-04-12 | Round 3 fixes: fix immutability algorithm, remove now() from motivation, clarify BETWEEN, add version header, allow IS NOT NULL for UNIQUE, add UPSERT, IF NOT EXISTS warning, N-ary grammar                                                                                                                                 |
| 2.0     | 2026-04-12 | Round 2 fixes: clarify query planning vs scan, fix UNIQUE semantics, remove now() contradiction, fix u16 prefix, add transactional consistency, column/table rename impact, add EXISTS/BETWEEN rejection                                                                                                                     |
| 1.3     | 2026-04-12 | Add missing BLUEPRINT sections; storage format, error handling, canonicalization, test vectors, concurrency tests                                                                                                                                                                                                            |
| 1.2     | 2026-04-12 | Prettier formatting                                                                                                                                                                                                                                                                                                          |
| 1.1     | 2026-04-12 | Initial draft with partial specification                                                                                                                                                                                                                                                                                     |
| 1.0     | 2026-04-12 | Initial creation                                                                                                                                                                                                                                                                                                             |

## Related RFCs

- [RFC-0903: Virtual API Key System](../final/economics/0903-virtual-api-key-system.md) — primary use case driver
- [RFC-0912: FOR UPDATE Row Locking](../final/economics/0912-stoolap-for-update-row-locking.md) — orthogonal stoolap SQL feature
- [RFC-0913: WAL-Only Pub/Sub](../accepted/economics/0913-stoolap-pubsub-cache-invalidation.md) — orthogonal
- [RFC-0202: Vector Partial Indexes](../planned/storage/0202-vector-partial-indexes.md) — future work (HNSW partial indexes) _(Note: verify file exists before finalizing)_

## Related Use Cases

- [Enhanced Quota Router Gateway](../../docs/use-cases/enhanced-quota-router-gateway.md) — requires partial index for API key lookups

## Appendices

### A. Predicate Canonicalization Pseudocode

```
function canonicalize(expr):
    match expr:
        BinOp(AND, a, b):
            return sort_by_operator(AND, canonicalize(a), canonicalize(b))
        BinOp(OR, a, b):
            return sort_by_operator(OR, canonicalize(a), canonicalize(b))
        UnaryOp(NOT, inner):
            return push_not_down(NOT, canonicalize(inner))
        Between(col, lower, upper):
            // Step 1: Check bounds validity
            if lower > upper:
                return Constant(False)  // impossible range: normalize to constant false
            // Step 2: Expand BETWEEN to AND of >= and <=, then recurse
            return canonicalize(AND(
                Comparison(col, GTE, coerce(lower, col.type)),
                Comparison(col, LTE, coerce(upper, col.type))
            ))
        Comparison(col, op, const):
            return normalize_comparison(col, op, coerce(const, col.type))
        In(col, values):
            if values.length == 1:
                return canonicalize(Comparison(col, EQ, coerce(values[0], col.type)))
            else:
                return In(col, sort(dedup(coerce_each(values, col.type))))
        _:
            return expr  // leaf nodes unchanged

function push_not_down(NOT, expr):
    match expr:
        BinOp(OR, a, b):
            return AND(push_not_down(NOT, a), push_not_down(NOT, b))
        BinOp(AND, a, b):
            return OR(push_not_down(NOT, a), push_not_down(NOT, b))
        Comparison(col, EQ, val):
            return Comparison(col, NE, val)
        Comparison(col, NE, val):
            return Comparison(col, EQ, val)
        IsNull(col):
            return IsNotNull(col)
        IsNotNull(col):
            return IsNull(col)
        _:
            return NOT(expr)

// Sort by column name, then by operator precedence (LT<LTE<EQ<NE<GTE<GT), then by value
function sort_by_operator(op, left, right):
    if compare_column(left, right) < 0:
        return BinOp(op, left, right)
    else:
        return BinOp(op, right, left)

function compare_column(a, b):
    // Column comparison for sorting: column name primary key
    if a.column.name != b.column.name:
        return a.column.name <=> b.column.name
    // Same column: sort by operator precedence
    if a.op != b.op:
        return operator_precedence(a.op) <=> operator_precedence(b.op)
    // Same column and operator: sort by value
    return a.value <=> b.value

function operator_precedence(op):
    // Lower value = sorts first
    match op:
        LT: 0
        LTE: 1
        EQ: 2
        NE: 3
        GTE: 4
        GT: 5
        default: 99
```

### B. Serialization Format

**Version header format:**

```
Byte 0: VERSION (0x01 for current format)
Bytes 1-2: LENGTH (u16, unsigned little-endian, max 65535)
Bytes 3+: rkyv(CanonicalExpression)
```

**Handling format version mismatches:**

| Read Version | Code Version                        | Action                                                                                           |
| ------------ | ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| 0x01         | current (supports VERSION byte)     | Deserialize normally (skip VERSION byte, read LENGTH, read rkyv)                                 |
| 0x01         | older (before VERSION byte support) | **Cannot read**: older code expects raw rkyv without VERSION/LENGTH prefix; hard incompatibility |
| any          | newer (increased format version)    | Error: "predicate format version not supported"                                                  |
| 0x00         | (reserved)                          | Error: "predicate format version not supported"                                                  |

**Critical:** Old code (before v4.0) cannot read databases created by v4.0+ code because it will interpret the VERSION byte (0x01) as the first byte of rkyv data. This is a hard incompatibility, not a gracefully degradable one. Users must recreate partial indexes when downgrading.

### C. Error Code Reference

| Code | Name                            | Description                                                                                                                                       |
| ---- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| E001 | COLUMN_NOT_FOUND                | Column referenced in predicate does not exist                                                                                                     |
| E002 | SUBQUERY_NOT_ALLOWED            | Subqueries not permitted in partial index predicates                                                                                              |
| E003 | FUNCTION_NOT_ALLOWED            | Function calls not permitted in partial index predicates                                                                                          |
| E004 | PREDICATE_TOO_COMPLEX           | Predicate exceeds depth or term limits                                                                                                            |
| E005 | IN_LIST_TOO_LARGE               | IN list exceeds 32 items                                                                                                                          |
| E006 | PREDICATE_ENCODING_FAILED       | Expression serialization failed                                                                                                                   |
| E007 | PREDICATE_EVAL_FAILED           | Predicate evaluation error during DML; operation succeeds, row not indexed, warning logged                                                        |
| E008 | UNIQUE_PREDICATE_NOT_IMMUTABLE  | Predicate contains mutable operator for UNIQUE partial index                                                                                      |
| E009 | EXISTS_NOT_ALLOWED              | EXISTS not permitted in partial index predicates                                                                                                  |
| E010 | LIKE_NOT_ALLOWED                | LIKE/ILIKE not permitted in partial index predicates                                                                                              |
| E011 | MULTIPLE_BETWEEN_ON_SAME_COLUMN | Multiple BETWEEN on same column in predicate                                                                                                      |
| E012 | UNSUPPORTED_FORMAT_VERSION      | Predicate serialization format version not supported. Currently only version 0x01 is supported; E012 fires for 0x00 (reserved) or future versions |
| W001 | INDEX_EXISTS_DIFFERENT_PRED     | CREATE INDEX IF NOT EXISTS: index exists with different predicate; succeeds with warning                                                          |

### D. UNIQUE Partial Index Immutability

**Application-level guarantee model (Phase 1):**

The implementation checks that the predicate uses only immutable operators. The application guarantees that the data itself satisfies the immutability requirement.

**Implementation check (E008):**

Reject predicates containing: `>`, `>=`, `<`, `<=`, `!=`, `LIKE`, `ILIKE`

Allow predicates containing: `=`, `IN`, `IS NULL`, `IS NOT NULL`

**Canonicalization timing:** E008 validation runs **after** canonicalization. Since `col IN (1)` canonicalizes to `col = 1` before validation, single-item IN lists are allowed for UNIQUE partial indexes. Multi-item IN lists (`col IN (1, 2)`) are rejected because they cannot be verified as cycling-safe.

**Application guarantee:**

| Predicate                  | Application Guarantees                                           |
| -------------------------- | ---------------------------------------------------------------- |
| `WHERE status = 'active'`  | Status transitions are one-way: active → revoked (never back)    |
| `WHERE tenant_id = 1`      | tenant_id column is never updated after row creation             |
| `WHERE deleted_at IS NULL` | Rows are physically deleted when deleted_at is set               |
| `WHERE key IS NOT NULL`    | NULL is the final state; key never changes from non-NULL to NULL |

**What happens if application violates guarantee:**

If the application updates a row in a way that violates the immutability guarantee, the UNIQUE constraint may be silently violated (duplicate key in indexed rows). Phase 2 adds trigger-based enforcement to detect this.

---

**Submission Date:** 2026-04-12
**Last Updated:** 2026-04-13
