# RFC-0903-B1 (Economics): Schema Amendments to RFC-0903 Final

## Status

Draft (v17 — Amendment to RFC-0903 Final v29)

## Authors

- Author: @cipherocto

## Summary

This document specifies **amendments** to RFC-0903 Final v29 ("Virtual API Key System") for the `spend_ledger` table schema. These changes are required by RFC-0201 (Binary BLOB Type for Deterministic Hash Storage) and improve storage efficiency for the ledger-based quota accounting system defined in RFC-0909.

This is a **formal amendment** to an Accepted/Final RFC. It does not supersede RFC-0903 — it patches specific schema definitions while leaving all other RFC-0903 specifications intact.

## Dependencies

**Amends:**
- RFC-0903 Final v29: Virtual API Key System

**Required By:**
- RFC-0909: Deterministic Quota Accounting (depends on these schema changes)
- RFC-0903-C1: Extended Schema Amendments (uses these FK definitions as base)

**Informative:**
- RFC-0201: Binary BLOB Type for Deterministic Hash Storage (Accepted) — defines BLOB as first-class type

## Motivation

### Problem 1: Hex Encoding Waste

RFC-0903 Final v29 stores `event_id` (SHA256 output) as `TEXT` with hex encoding:

```sql
event_id TEXT PRIMARY KEY  -- 64 hex chars: "a1b2c3..."
```

This wastes 2x storage (32 raw bytes → 64 hex chars). RFC-0201 (Accepted) defines `BLOB(32)` as the canonical storage for SHA256 hashes. RFC-0903 must be amended to use it.

### Problem 2: key_id Text Storage

`key_id` is stored as `TEXT` containing a UUID:

```sql
key_id TEXT NOT NULL  -- key_id: UUID text "550e8400-e29b-41d4-a716-446655440000" (36 chars)
```

UUIDs are 16 bytes. Text storage requires 36+ bytes. `BLOB(16)` reduces storage by 56% (BLOB(16) is 44% of TEXT(36) size; savings = 20 bytes/row).

### Problem 3: Missing Composite Indexes

RFC-0903 Final schema lacks `idx_spend_ledger_key_created` for efficient `ORDER BY created_at` replay queries and `idx_spend_ledger_event_id` for direct event lookup. RFC-0909 needs both.

## Schema Amendments

### Section: spend_ledger Table

**RFC-0903 Final v29 (original):**

```sql
CREATE TABLE spend_ledger (
    event_id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    team_id TEXT,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_amount BIGINT NOT NULL,
    pricing_hash BYTEA(32) NOT NULL,
    timestamp INTEGER NOT NULL,
    token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
    tokenizer_version TEXT,
    provider_usage_json TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(key_id, request_id),
    FOREIGN KEY(key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE,
    FOREIGN KEY(team_id) REFERENCES teams(team_id) ON DELETE SET NULL
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
```

**RFC-0903-B1 (amended):**

```sql
CREATE TABLE tokenizers (
    tokenizer_id BLOB(16) NOT NULL,         -- Raw BLAKE3 hash of version string (16 bytes) — RFC-0903-B1
    version TEXT NOT NULL,                   -- e.g., "tiktoken-cl100k_base-v1.2.3"
    vocab_size INTEGER,                      -- e.g., 100000
    encoding_type TEXT,                      -- e.g., "bpe", "sentencepiece"
    PRIMARY KEY (tokenizer_id)
);

**Population mechanism:** `tokenizer_id` is derived from the version string via BLAKE3 at insert time — no pre-population of the `tokenizers` table is required. When a `spend_ledger` INSERT arrives with `tokenizer_id` set (i.e., `token_source = CanonicalTokenizer`), the application derives `tokenizer_id = BLAKE3(version_string)` and inserts it. If the corresponding row does not yet exist in `tokenizers`, the application inserts it on-demand:

```rust
// On-demand tokenizer population (application-level, not FK-enforced at INSERT)
fn ensure_tokenizer(db: &Database, version: &str) -> [u8; 16] {
    let tokenizer_id = tokenizer_version_to_id(version); // BLAKE3 of version
    // Insert if not exists (upsert pattern; idempotent)
    db.execute(
        "INSERT OR IGNORE INTO tokenizers (tokenizer_id, version) VALUES (?, ?)",
        [tokenizer_id, version],
    )?;
    tokenizer_id
}
```

This is an application-level upsert, not a FK-triggered auto-population. The `ON DELETE SET NULL` FK behavior means that if a `tokenizers` row is deleted, existing `spend_ledger` rows with that `tokenizer_id` will have NULL values — this is the intended behavior (orphan tokenizer references become unresolvable without deleting the ledger entries).

> **Note:** RFC-0910 (Pricing Table Registry) is the authoritative source for tokenizer version assignments and registry management. This RFC defines the storage schema (BLOB(16) FK and on-demand population pattern); RFC-0910 defines the version assignment and lifecycle management. This RFC does not depend on RFC-0910 for the storage mechanism — the FK relationship is valid without RFC-0910 being implemented.

CREATE TABLE spend_ledger (
    event_id BLOB(32) NOT NULL,              -- Raw SHA256 binary (32 bytes) — RFC-0201
    request_id BLOB(32) NOT NULL,            -- Raw binary (32 bytes, SHA256 of gateway text) — RFC-0201
    key_id BLOB(16) NOT NULL,                -- Raw UUID bytes (16 bytes) — was TEXT in RFC-0903 Final, BLOB per RFC-0903-B1
    team_id BLOB(16),                        -- Raw UUID bytes (16 bytes) — RFC-0903-C1
    provider TEXT NOT NULL,                   -- Unchanged
    model TEXT NOT NULL,                      -- Unchanged
    input_tokens INTEGER NOT NULL,           -- Unchanged
    output_tokens INTEGER NOT NULL,           -- Unchanged
    cost_amount BIGINT NOT NULL,             -- Unchanged
    pricing_hash BYTEA(32) NOT NULL,         -- Unchanged (pre-existing binary type, not affected by this amendment)
    timestamp INTEGER NOT NULL,              -- Unchanged
    token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
    tokenizer_id BLOB(16),                   -- FK to tokenizers(tokenizer_id) — was TEXT in RFC-0903 Final
    provider_usage_json TEXT,               -- Unchanged
    created_at INTEGER NOT NULL,             -- Unchanged (stoolap: INTEGER NOT NULL, app provides value)
    -- Idempotency: UNIQUE constraint prevents duplicate request_id per key
    -- Note: event_id is BLOB(32) NOT NULL, NOT a PRIMARY KEY (stoolap quirk).
    -- The RFC-0903 Final PRIMARY KEY on event_id is replaced by a regular
    -- index (idx_spend_ledger_event_id) and the UNIQUE(key_id, request_id) constraint.
    UNIQUE(key_id, request_id),
    FOREIGN KEY(key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE,
    FOREIGN KEY(team_id) REFERENCES teams(team_id) ON DELETE SET NULL,
    FOREIGN KEY(tokenizer_id) REFERENCES tokenizers(tokenizer_id) ON DELETE SET NULL
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);  -- pre-existing legacy (not used in deterministic replay path)
CREATE INDEX idx_spend_ledger_event_id ON spend_ledger(event_id);          -- RFC-0903-B1 ext
-- NOTE: event_id is functionally unique (SHA256 of request content), but no UNIQUE
-- constraint is added due to stoolap BLOB compatibility (BLOB columns cannot be PRIMARY KEY
-- in stoolap; a UNIQUE index is equivalent to PRIMARY KEY in most DBs and carries the same
-- restriction). The application layer MUST enforce event_id uniqueness at insert time —
-- duplicate event_id values indicate either a hash collision or a bug in compute_event_id.
-- If two rows with identical event_id are inserted, deterministic replay and Merkle tree
-- construction are silently corrupted. The UNIQUE(key_id, request_id) constraint prevents
-- duplicate request recording for a given key; event_id uniqueness is a separate concern.
CREATE INDEX idx_spend_ledger_key_created ON spend_ledger(key_id, created_at); -- RFC-0903-B1 ext
CREATE INDEX idx_spend_ledger_pricing_hash ON spend_ledger(pricing_hash); -- RFC-0903-B1 ext
CREATE INDEX idx_spend_ledger_tokenizer ON spend_ledger(tokenizer_id);   -- RFC-0903-B1 ext
```

### api_keys Table (unchanged by RFC-0903-B1)

The `api_keys` table schema in RFC-0903 Final is unchanged by this amendment. `key_hash BYTEA(32)` (HMAC-SHA256) is already binary and requires no amendment.

> **Note (RFC-0903-C1):** RFC-0903-C1 amends `api_keys.key_id` and `api_keys.team_id` to `BLOB(16)` for FK consistency. After RFC-0903-C1, the FK `spend_ledger.key_id → api_keys.key_id` is type-consistent `BLOB(16) → BLOB(16)`.

## Change Summary

| Field/Index | RFC-0903 Final | RFC-0903-B1 | Delta |
|------------|----------------|-------------|-------|
| `event_id` | `TEXT` (hex, 64 chars) | `BLOB(32)` (raw bytes) | −32 bytes/row |
| `request_id` | `TEXT` (variable) | `BLOB(32)` (raw SHA256 bytes) | Variable; up to −32 bytes |
| `key_id` | `TEXT` (UUID hex, 36 chars) | `BLOB(16)` (raw bytes) | −20+ bytes/row |
| `tokenizer_version` | `TEXT` (version string) | `BLOB(16)` (FK to tokenizers table) | −9 bytes/row on ~50% of rows |
| `tokenizers` table | *(absent)* | Added | New table |
| `idx_spend_ledger_event_id` | *(absent)* | Added | New |
| `idx_spend_ledger_key_created` | *(absent)* | Added | New |
| `idx_spend_ledger_pricing_hash` | *(absent)* | Added | New |
| `idx_spend_ledger_tokenizer` | *(absent)* | Added | New |

> **Note on `created_at`:** RFC-0903 Final specifies `created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))`. RFC-0903-B1 does not change this — stoolap uses `INTEGER NOT NULL` with application-provided values. The Change Summary does not list `created_at` because no change applies.
>
> **Note on `team_id`:** RFC-0903-B1 left `team_id` as TEXT in spend_ledger. RFC-0903-C1 amends it to `BLOB(16)` for FK consistency with `teams.team_id`.

**Storage savings per spend_ledger row:** ~52 bytes minimum (event_id 32 + key_id 20) plus up to 32 more for request_id, plus ~9 bytes for tokenizer_id on rows with CanonicalTokenizer.

## API Compatibility Notes

### event_id

- **RFC-0903 Final:** `compute_event_id()` returns `String` (hex-encoded). `event_id TEXT` in schema.
- **RFC-0903-B1:** `compute_event_id()` **still returns `String` (hex-encoded)** for API/debugging compatibility. Storage uses `BLOB(32)`. When inserting, encode the hex string to raw bytes: `hex::decode(event_id)`.

```rust
// Before (RFC-0903 Final): TEXT storage of hex string
// The String from compute_event_id() was bound directly to a TEXT column.
params![event_id.clone().into()]  // TEXT: stores "a1b2c3d4..." (64 hex chars)

// After (RFC-0903-B1): BLOB storage of raw binary
// hex::decode() converts the hex String to 32 raw bytes before INSERT.
params![stoolap::core::Value::blob(hex::decode(&event_id).unwrap())]  // BLOB(32): stores raw 32 bytes
```

### request_id

`request_id` is provided by the API gateway as a text string. It is stored as `BLOB(32)` (32 raw bytes). **The gateway provides request_id as raw text (NOT hex-encoded).** Encoding to 32 bytes uses SHA256 hashing for variable-length inputs.

**Encoding rules (all routers MUST use the same scheme):**

| Gateway format | Encoding to 32 bytes | Example |
|----------------|----------------------|---------|
| Any string (any length) | SHA256 of the string | `"req-123"` → SHA256 |

> **Design note:** SHA256 is always used regardless of input length. A previous 32-byte pass-through optimization (copying raw bytes for exactly-32-byte inputs) was removed to eliminate the encoding discontinuity and edge cases it created. All gateway request_id strings — regardless of length — are SHA256-hashed to 32 bytes.

> **⚠️ Hex-formatted input is not supported.** If the gateway sends a hex string as input, it is SHA256-hashed as raw ASCII bytes — NOT hex-decoded first. This produces a **different** 32-byte value than hex-decoding first. Gateways MUST send raw binary/text, not hex. There is no hex-decoding path for request_id in this RFC.

> **Audit trail:** The original gateway `request_id` text is not recoverable from the stored BLOB(32) (SHA256 is one-way). For forensic duplicate-suppression auditing, the gateway's raw `request_id` value is preserved in the `provider_usage_json` field (or a dedicated audit column if the implementation adds one). Applications MUST NOT rely on decoding the stored BLOB back to the original gateway string.

**Implementation:**

```rust
/// Encode a gateway-provided request_id string to 32 raw bytes for BLOB(32) storage.
/// All inputs are treated as raw text strings (not hex). Always uses SHA256 regardless
/// of input length — uniform encoding for all gateway request_id formats.
///
/// WARNING: The gateway's input format (raw text vs hex) must be consistent across
/// all routers. A router that changes input format will produce different request_id
/// values for the same logical request, breaking idempotency.
pub fn encode_request_id(request_id: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(request_id.as_bytes());
    hasher.finalize().into()
}
```

**Important:** The encoding scheme must be consistent across all routers. Document the input format (raw text string) in the deployment runbook.

### key_id

`key_id` is `uuid::Uuid` in the application. Storage/retrieval:

```rust
// Insert: UUID → BLOB(16)
let key_id_blob: Vec<u8> = key_id.as_bytes().to_vec(); // 16 bytes
params![stoolap::core::Value::blob(key_id_blob)]

// Lookup: BLOB(16) → UUID
let bytes: [u8; 16] = row.get("key_id")?;
let key_id = uuid::Uuid::from_bytes(bytes);
```

## stoolap Compatibility

These changes require stoolap to support:
1. `BLOB(n)` where `n` is a length specifier (stoolap already has `BLOB` type per RFC-0201)

Note: The PRIMARY KEY on `event_id` from RFC-0903 Final is **not retained** in RFC-0903-B1. It is replaced by `idx_spend_ledger_event_id` and `UNIQUE(key_id, request_id)`. The PRIMARY KEY is not needed for BLOB columns in stoolap's implementation.

If stoolap does not support length-specified BLOBs (`BLOB(32)` vs unconstrained `BLOB`), use `VARBINARY(32)` or unconstrained `BLOB` with application-layer length validation.

## Backward Compatibility

These are **breaking schema changes**. Existing deployments must run an application-layer migration (the functions below are Rust pseudocode — they cannot be called as SQL UDFs).

```rust
/// Migrate event_id: hex-encoded TEXT (64 chars) → raw BLOB(32).
/// RFC-0903 Final stores hex "a1b2c3..." (64 chars). hex::decode → 32 raw bytes.
fn migrate_event_id(hex_str: &str) -> [u8; 32] {
    let bytes = hex::decode(hex_str).expect("valid hex event_id");
    let mut blob = [0u8; 32];
    blob.copy_from_slice(&bytes);
    blob
}

/// Migrate request_id: raw TEXT string → raw BLOB(32).
/// RFC-0903 Final stores the raw gateway text string.
/// encode_request_id() always uses SHA256 — produces the same value as runtime inserts.
fn migrate_request_id(text: &str) -> [u8; 32] {
    encode_request_id(text) // see §request_id for definition
}

/// Migrate key_id: UUID hex string (36 chars "550e8400-e29b-41d4-a716-446655440000") → raw BLOB(16).
/// uuid::Uuid::parse_str() decodes hex → 16 raw bytes.
fn migrate_key_id(hex_str: &str) -> [u8; 16] {
    let uuid = uuid::Uuid::parse_str(hex_str).expect("valid UUID hex string");
    *uuid.as_bytes()
}
```

**Application-layer migration procedure:**

This is application-level pseudocode — the migrate_* functions run in Rust, not as SQL UDFs.
The UPDATE uses parameterized queries with `?` placeholders (JDBC/SQLite style; PostgreSQL uses `$1`, `$2`, ...).

**Migration uses shadow columns to avoid ALTER COLUMN TYPE locking and SQLite incompatibility.**
RFC-0903 Final defines `event_id TEXT PRIMARY KEY`, which is a stable unique row identifier for WHERE clauses throughout migration — no `rowid` dependency.

**CRITICAL: Write quiesce required during Phase 2 population.**
Phase 2 (populating shadow columns) requires a dual-write or write-quiesce strategy to prevent new BLOB-encoded rows from being inserted while migration is in progress. Without this, rows inserted during the migration window using the new BLOB path will coexist with partially-migrated TEXT rows after the column swap, silently breaking deterministic replay. Options:

1. **Dual-write:** During migration, the application writes to both TEXT columns (old path) and BLOB shadow columns (new path) simultaneously. On completion, the column swap is instantaneous.
2. **Write-quiesce:** Quiesce all writes to `spend_ledger` (block new INSERTs at the application layer) during Phase 2 population, then perform the column swap, then unblock writes. Suitable for maintenance windows with zero new writes.
3. **Cutover-only:** If the database is initially empty (greenfield deployment per RFC-0914), no migration is needed — the BLOB schema is created directly and no dual-write or quiesce is required.

If neither dual-write nor write-quiesce is feasible, the migration must be deferred until a maintenance window allows it. A partial migration that leaves concurrent writes active will produce silent data corruption.

```
-- Phase 1: Add BLOB shadow columns (no data modification)
ALTER TABLE spend_ledger ADD COLUMN event_id_new BLOB(32);
ALTER TABLE spend_ledger ADD COLUMN request_id_new BLOB(32);
ALTER TABLE spend_ledger ADD COLUMN key_id_new BLOB(16);

-- Phase 2: Populate shadow columns in batches (application code, repeatable)
SELECT event_id, request_id, key_id FROM spend_ledger
    WHERE event_id_new IS NULL LIMIT 1000;
For each row, compute in Rust:
    new_event_id    = migrate_event_id(event_id)         -- [u8; 32]
    new_request_id  = migrate_request_id(request_id)     -- [u8; 32]
    new_key_id      = migrate_key_id(key_id)             -- [u8; 16]
UPDATE spend_ledger
    SET event_id_new = ?, request_id_new = ?, key_id_new = ?
    WHERE event_id = ?;  -- event_id TEXT was PRIMARY KEY in RFC-0903 Final

Repeat until all rows migrated. The WHERE clause uses event_id (PRIMARY KEY in RFC-0903 Final).

-- Phase 3: Column swap (database-specific)
-- SQLite:
CREATE TABLE spend_ledger_new (LIKE spend_ledger);
INSERT INTO spend_ledger_new SELECT ..., event_id_new, request_id_new, key_id_new, ... FROM spend_ledger;
DROP TABLE spend_ledger;
ALTER TABLE spend_ledger_new RENAME TO spend_ledger;
-- Recreate indexes (dropped by CREATE TABLE LIKE in some DBs)
CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);
CREATE INDEX idx_spend_ledger_event_id ON spend_ledger(event_id);
CREATE INDEX idx_spend_ledger_key_created ON spend_ledger(key_id, created_at);
CREATE INDEX idx_spend_ledger_pricing_hash ON spend_ledger(pricing_hash);

-- PostgreSQL/MySQL:
BEGIN;
ALTER TABLE spend_ledger DROP COLUMN event_id;
ALTER TABLE spend_ledger DROP COLUMN request_id;
ALTER TABLE spend_ledger DROP COLUMN key_id;
ALTER TABLE spend_ledger RENAME COLUMN event_id_new TO event_id;
ALTER TABLE spend_ledger RENAME COLUMN request_id_new TO request_id;
ALTER TABLE spend_ledger RENAME COLUMN key_id_new TO key_id;
-- Recreate UNIQUE and FK constraints
ALTER TABLE spend_ledger ADD CONSTRAINT spend_ledger_key_request_uniq UNIQUE(key_id, request_id);
COMMIT;
```

> **Why shadow columns?** ALTER COLUMN TYPE locks the table in PostgreSQL/MySQL for the duration of data conversion. For large tables this can be minutes to hours. The shadow-column approach allows zero-downtime migration: application code populates shadow columns in batches while the old columns remain active. Column swap is a fast metadata operation. SQLite does not support ALTER COLUMN TYPE at all — shadow columns are the only path.

Implementations must also update `compute_event_id()` to store hex-to-binary conversion at insert time, and binary-to-hex conversion at read time.

## Relationship to RFC-0909

RFC-0909 (Deterministic Quota Accounting) adopts this amended schema. All `SpendEvent` construction and ledger recording code in RFC-0909 implementations must use the BLOB types described above.

**CRITICAL:** RFC-0903 Final's `record_spend()` and `record_spend_with_team()` functions MUST be updated to adopt RFC-0903-B1 encoding before any deployment that uses the new BLOB schema. Specifically:
- `event_id`: encode hex string → raw `BLOB(32)` via `hex_to_blob_32()` before INSERT
- `request_id`: encode raw gateway text → raw `BLOB(32)` via `encode_request_id()` before INSERT
- `key_id`: encode `uuid::Uuid` → raw `BLOB(16)` via `uuid_to_blob_16()` before INSERT

If `record_spend()` continues to use TEXT encoding while other parts of the system use BLOB encoding, the ledger will contain mixed-encoding records, breaking deterministic replay and Merkle tree construction. The entire ledger must use one encoding consistently. RFC-0903 Final must be amended (or this RFC-0903-B1 amendment explicitly scopes the required changes to `record_spend`) before deployment.

## Constraints

### Idempotency Scope (Known Limitation)

The `UNIQUE(key_id, request_id)` constraint scopes `request_id` to a single key. A single key issuing requests to multiple providers with the same gateway-assigned `request_id` is treated as a duplicate at the gateway layer — not a schema issue.

**Scenario:** A client key is configured to route to Provider A (primary) and Provider B (fallback). The client sends `request_id: "req-abc123"` to Provider A, then sends a second request (different logical operation, different provider) that also arrives with `request_id: "req-abc123"`. The second request is silently deduplicated.

**Assessment:** This is correct behavior per RFC-0903 Final's idempotency design. Both requests from the same client with the same `request_id` are treated as the same logical request by the client's own labeling. If multi-provider key scoping is required, a `(key_id, provider, request_id)` unique constraint would be needed — this is a future RFC-0903 extension, not this amendment.

If multi-provider key scoping becomes required, a future RFC-0903 amendment must change the constraint scope before this can be adopted as a budget enforcement guarantee.

## Changelog

| Version | Date       | Changes |
|---------|------------|---------|
| v17     | 2026-04-15 | Round 25 fixes (continued): move idempotency scope note to explicit Constraints section |
| v16     | 2026-04-15 | Round 25 fixes: add audit trail note for request_id (provider_usage_json); add write-quiesce requirement to migration; add tokenizer on-demand population mechanism; add uniqueness scope note for request_id; add event_id uniqueness enforcement note (application-layer); add RFC-0903-C1 to Required By |
| v15     | 2026-04-15 | Round 22 fixes: add tokenizers table and tokenizer_id FK (normalize tokenizer_version from TEXT to BLOB(16)); add idx_spend_ledger_tokenizer index; update Change Summary and storage savings |
| v14     | 2026-04-15 | Round 20 fixes: add missing idx_spend_ledger_key_time to both schema examples and Phase 3 SQLite index recreation; update Status v13→v14 |
| v12     | 2026-04-15 | Round 17 fixes: rewrite migration steps 7-10 as parameterized queries (remove SQL UDF syntax; all migrate_* calls are Rust, not SQL); add row identifier (rowid) to migration step 1 for per-row parameterized UPDATEs |
| v11     | 2026-04-15 | Round 16 fixes: clarify key_id UUID example in Problem 2 (remove stale "+ null"), add cross-RFC determinism warning for get_canonical_tokenizer |
| v10     | 2026-04-15 | Round 15 fixes: split multi-column ALTER TABLE into separate per-column statements (PostgreSQL/MySQL compatible), add SQLite ALTER COLUMN limitation note |
| v9      | 2026-04-15 | Round 14 fixes: fix key_id comment (cite source RFC-0903 not circular RFC-0903-B1), align index comments with RFC-0909 style ("RFC-0903-B1 ext") |
| v8      | 2026-04-15 | Round 13 fixes: fix pricing_hash BYTEA(32) misleading comment (not RFC-0201), extend 32-char ASCII edge case warning to cover non-hex 32-byte strings, fix schema column spacing |
| v7      | 2026-04-15 | Round 12 fixes: rewrite migration as application-layer pseudocode (Rust functions not SQL UDFs), add explicit 32-char ASCII hex edge case in encoding table, add B1 adoption requirement for record_spend, update request_id Change Summary to note SHA256 |
| v6      | 2026-04-15 | Round 11 fixes: clarify request_id schema comment (SHA256 of gateway text) |
| v5      | 2026-04-15 | Round 10 fixes: fix stale string_to_blob reference in migration comment, improve event_id before/after example, add hex-formatted request_id warning |
| v4      | 2026-04-15 | Round 9 fixes: remove stale PRIMARY KEY from stoolap compat (replaced by index), fix request_id migration SQL (pad/truncate → SHA256 encode_request_id) |
| v3      | 2026-04-14 | Round 8 fixes: remove UUID encoding from request_id table (gateway provides raw text), fix encode_request_id to match actual encoding logic, clarify gateway input format (raw text, not hex) |
| v2      | 2026-04-14 | Round 7 fixes: add request_id encoding rules table + encode_request_id() function |
| v1      | 2026-04-14 | Initial amendment: event_id TEXT→BLOB(32), request_id TEXT→BLOB(32), key_id TEXT→BLOB(16); add idx_spend_ledger_event_id, idx_spend_ledger_key_created, idx_spend_ledger_pricing_hash |

---

**Draft Date:** 2026-04-15
**Version:** v17
**Amends:** RFC-0903 Final v29
**Required By:** RFC-0909 (Deterministic Quota Accounting), RFC-0903-C1 (Extended Schema Amendments)
**Related RFCs:** RFC-0201 (Binary BLOB Type)
