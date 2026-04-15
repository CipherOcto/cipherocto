# RFC-0903-C1 (Economics): Extended Schema Amendments to RFC-0903 Final

## Status

Draft (v1 — Amendment to RFC-0903 Final v29 + RFC-0903-B1 amendment v15)

## Authors

- Author: @cipherocto

## Summary

This document specifies **additional amendments** to RFC-0903 Final v29 extending RFC-0903-B1. RFC-0903-B1 amended `spend_ledger` columns but explicitly left `api_keys` and `teams` unchanged. This created a type mismatch: `spend_ledger.key_id` is now `BLOB(16)` but `api_keys.key_id` remained `TEXT`, making the foreign key relationship `BLOB(16) → TEXT` — invalid in strict databases.

RFC-0903-C1 completes the consolidation by amending `api_keys.key_id`, `api_keys.team_id`, and `teams.team_id` to `BLOB(16)`, ensuring all foreign key relationships are type-consistent.

This is a **formal amendment** to an Accepted/Final RFC. It does not supersede RFC-0903 — it patches specific schema definitions while leaving all other RFC-0903 specifications intact.

## Dependencies

**Amends:**
- RFC-0903 Final v29: Virtual API Key System
- RFC-0903-B1 (Schema Amendments to RFC-0903 Final) — extends BLOB consolidation to `api_keys` and `teams`

**Required By:**
- RFC-0909: Deterministic Quota Accounting (depends on consistent BLOB types across all FK relationships)

**Informative:**
- RFC-0201: Binary BLOB Type for Deterministic Hash Storage (Accepted) — defines BLOB as first-class type

## Motivation

### Problem 1: Foreign Key Type Mismatch (RFC-0903-B1 Gap)

RFC-0903-B1 amended `spend_ledger.key_id` to `BLOB(16)` but explicitly did not amend `api_keys.key_id`:

> *"The `api_keys` table schema in RFC-0903 Final is unchanged by this amendment."*

This creates a broken foreign key:

```sql
FOREIGN KEY(key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE
-- spend_ledger.key_id:  BLOB(16)  ← new from RFC-0903-B1
-- api_keys.key_id:      TEXT      ← unchanged by RFC-0903-B1
-- FK: BLOB(16) → TEXT             ← type mismatch
```

Any database with strict type enforcement (PostgreSQL, MySQL strict mode) rejects this.

### Problem 2: Team Foreign Key Chain Also Broken

`teams.team_id` was also not amended:

```sql
spend_ledger.team_id:           TEXT       ← unchanged by RFC-0903-B1
api_keys.team_id:               TEXT       ← unchanged by RFC-0903-B1
teams.team_id:                  TEXT       ← unchanged by RFC-0903-B1
FOREIGN KEY(team_id) REFERENCES teams(team_id)  -- fine internally
```

But now `spend_ledger.key_id` is `BLOB(16)` referencing `api_keys.key_id` which is `TEXT` — the FK chain is broken at the first hop.

### Problem 3: api_keys.team_id Should be BLOB(16) Too

`api_keys.team_id` is a UUID foreign key to `teams.team_id`. If `teams.team_id` becomes `BLOB(16)`, `api_keys.team_id` must also be `BLOB(16)` to maintain FK type consistency.

## Schema Amendments

### Section: teams Table

**RFC-0903 Final v29 (original):**

```sql
CREATE TABLE teams (
    team_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    budget_limit BIGINT NOT NULL CHECK (budget_limit >= 0),
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_teams_team_id ON teams(team_id);
```

**RFC-0903-C1 (amended):**

```sql
CREATE TABLE teams (
    team_id BLOB(16) NOT NULL,           -- Raw UUID bytes (16 bytes) — RFC-0903-C1
    name TEXT NOT NULL,                   -- Unchanged
    budget_limit BIGINT NOT NULL CHECK (budget_limit >= 0),  -- Unchanged
    created_at INTEGER NOT NULL,          -- Unchanged
    PRIMARY KEY (team_id)
);

CREATE INDEX idx_teams_team_id ON teams(team_id);  -- on BLOB(16)
```

### Section: api_keys Table

**RFC-0903 Final v29 (original):**

```sql
CREATE TABLE api_keys (
    key_id TEXT PRIMARY KEY,
    key_hash BYTEA NOT NULL,
    key_prefix TEXT NOT NULL CHECK (length(key_prefix) >= 8),
    team_id TEXT,
    budget_limit BIGINT NOT NULL CHECK (budget_limit >= 0),
    rpm_limit INTEGER CHECK (rpm_limit >= 0),
    tpm_limit INTEGER CHECK (tpm_limit >= 0),
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked INTEGER DEFAULT 0,
    revoked_at INTEGER,
    revoked_by TEXT,
    revocation_reason TEXT,
    key_type TEXT DEFAULT 'default',
    allowed_routes TEXT,
    auto_rotate INTEGER DEFAULT 0,
    rotation_interval_days INTEGER,
    description TEXT,
    metadata TEXT,
    FOREIGN KEY (team_id) REFERENCES teams(team_id) ON DELETE SET NULL
);

CREATE UNIQUE INDEX idx_api_keys_key_hash_unique ON api_keys(key_hash);
CREATE INDEX idx_api_keys_team_id ON api_keys(team_id);
CREATE INDEX idx_api_keys_expires ON api_keys(expires_at);
```

**RFC-0903-C1 (amended):**

```sql
CREATE TABLE api_keys (
    key_id BLOB(16) NOT NULL,           -- Raw UUID bytes (16 bytes) — was TEXT in RFC-0903 Final, BLOB per RFC-0903-C1
    key_hash BYTEA(32) NOT NULL,         -- Unchanged (pre-existing binary type)
    key_prefix TEXT NOT NULL CHECK (length(key_prefix) >= 8),  -- Unchanged
    team_id BLOB(16),                   -- Raw UUID bytes (16 bytes) — was TEXT in RFC-0903 Final, BLOB per RFC-0903-C1
    budget_limit BIGINT NOT NULL CHECK (budget_limit >= 0),  -- Unchanged
    rpm_limit INTEGER CHECK (rpm_limit >= 0),  -- Unchanged
    tpm_limit INTEGER CHECK (tpm_limit >= 0),  -- Unchanged
    created_at INTEGER NOT NULL,          -- Unchanged
    expires_at INTEGER,                   -- Unchanged
    revoked INTEGER DEFAULT 0,            -- Unchanged
    revoked_at INTEGER,                   -- Unchanged
    revoked_by TEXT,                      -- Unchanged
    revocation_reason TEXT,               -- Unchanged
    key_type TEXT DEFAULT 'default',      -- Unchanged
    allowed_routes TEXT,                  -- Unchanged
    auto_rotate INTEGER DEFAULT 0,        -- Unchanged
    rotation_interval_days INTEGER,       -- Unchanged
    description TEXT,                     -- Unchanged
    metadata TEXT,                         -- Unchanged
    PRIMARY KEY (key_id),
    FOREIGN KEY (team_id) REFERENCES teams(team_id) ON DELETE SET NULL  -- BLOB(16) → BLOB(16)
);

CREATE UNIQUE INDEX idx_api_keys_key_hash_unique ON api_keys(key_hash);  -- on BYTEA (unchanged)
CREATE INDEX idx_api_keys_team_id ON api_keys(team_id);  -- on BLOB(16)
CREATE INDEX idx_api_keys_expires ON api_keys(expires_at);  -- Unchanged
```

### Section: spend_ledger Table (RFC-0903-B1 + RFC-0903-C1 compatibility)

The `spend_ledger` schema from RFC-0903-B1 includes `tokenizers` table and `tokenizer_id FK`. RFC-0903-C1 adds `BLOB(16)` for `key_id` and `team_id`:

```sql
CREATE TABLE tokenizers (
    tokenizer_id BLOB(16) NOT NULL,         -- Raw BLAKE3 hash of version string (16 bytes)
    version TEXT NOT NULL,                   -- e.g., "tiktoken-cl100k_base-v1.2.3"
    vocab_size INTEGER,
    encoding_type TEXT,                      -- e.g., "bpe", "sentencepiece"
    PRIMARY KEY (tokenizer_id)
);

CREATE TABLE spend_ledger (
    event_id BLOB(32) NOT NULL,              -- Raw SHA256 binary (32 bytes) — RFC-0903-B1
    request_id BLOB(32) NOT NULL,           -- Raw binary (32 bytes, SHA256 of gateway text) — RFC-0903-B1
    key_id BLOB(16) NOT NULL,                -- Raw UUID bytes (16 bytes) — RFC-0903-B1
    team_id BLOB(16),                        -- Raw UUID bytes (16 bytes) — RFC-0903-C1 (was TEXT)
    provider TEXT NOT NULL,                  -- Unchanged
    model TEXT NOT NULL,                     -- Unchanged
    input_tokens INTEGER NOT NULL,            -- Unchanged
    output_tokens INTEGER NOT NULL,           -- Unchanged
    cost_amount BIGINT NOT NULL,              -- Unchanged
    pricing_hash BYTEA(32) NOT NULL,          -- Unchanged
    timestamp INTEGER NOT NULL,               -- Unchanged
    token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),  -- Unchanged
    tokenizer_id BLOB(16),                  -- FK to tokenizers(tokenizer_id) — RFC-0903-B1
    provider_usage_json TEXT,                -- Unchanged
    created_at INTEGER NOT NULL,             -- Unchanged
    UNIQUE(key_id, request_id),
    FOREIGN KEY(key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE,    -- BLOB(16) → BLOB(16) ✓
    FOREIGN KEY(team_id) REFERENCES teams(team_id) ON DELETE SET NULL,    -- BLOB(16) → BLOB(16) ✓
    FOREIGN KEY(tokenizer_id) REFERENCES tokenizers(tokenizer_id) ON DELETE SET NULL  -- BLOB(16) → BLOB(16) ✓
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);           -- on BLOB(16)
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);         -- on BLOB(16)
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);
CREATE INDEX idx_spend_ledger_event_id ON spend_ledger(event_id);        -- RFC-0903-B1
CREATE INDEX idx_spend_ledger_key_created ON spend_ledger(key_id, created_at);  -- RFC-0903-B1
CREATE INDEX idx_spend_ledger_pricing_hash ON spend_ledger(pricing_hash);         -- RFC-0903-B1
CREATE INDEX idx_spend_ledger_tokenizer ON spend_ledger(tokenizer_id);   -- RFC-0903-B1
```

> **Note:** RFC-0903-B1 defined `team_id TEXT` as unchanged to avoid amending `teams`. RFC-0903-C1 completes this by also amending `teams.team_id` and `api_keys.team_id`, allowing `spend_ledger.team_id` to be BLOB(16) consistently.

## Change Summary

| Field/Index | RFC-0903 Final | RFC-0903-C1 | Delta |
|------------|----------------|-------------|-------|
| `teams.team_id` | `TEXT` (UUID hex, 36 chars) | `BLOB(16)` (raw UUID bytes) | −20 bytes/row |
| `api_keys.key_id` | `TEXT` (UUID hex, 36 chars) | `BLOB(16)` (raw UUID bytes) | −20 bytes/row |
| `api_keys.team_id` | `TEXT` (UUID nullable) | `BLOB(16)` (raw UUID bytes) | −20 bytes/row (nullable) |
| `idx_teams_team_id` | *(on TEXT)* | *(on BLOB(16))* | Updated |
| `idx_api_keys_team_id` | *(on TEXT)* | *(on BLOB(16))* | Updated |

**FK consistency after RFC-0903-C1:**

| Relationship | Before (RFC-0903 Final) | After (RFC-0903-C1) |
|--------------|-------------------------|---------------------|
| `spend_ledger.key_id` → `api_keys.key_id` | TEXT → TEXT | BLOB(16) → BLOB(16) ✓ |
| `spend_ledger.team_id` → `teams.team_id` | TEXT → TEXT | BLOB(16) → BLOB(16) ✓ |
| `api_keys.team_id` → `teams.team_id` | TEXT → TEXT | BLOB(16) → BLOB(16) ✓ |

## API Compatibility Notes

### key_id

`key_id` is `uuid::Uuid` in the application. Storage/retrieval for all tables:

```rust
// Insert: UUID → BLOB(16)
let key_id_blob: Vec<u8> = key_id.as_bytes().to_vec(); // 16 bytes
params![stoolap::core::Value::blob(key_id_blob)]

// Lookup: BLOB(16) → UUID
let bytes: [u8; 16] = row.get("key_id")?;
let key_id = uuid::Uuid::from_bytes(bytes);
```

### team_id

`team_id` is `uuid::Uuid` in the application (wrapped in `Option<Uuid>` for nullable columns). Storage/retrieval:

```rust
// Insert (non-null): UUID → BLOB(16)
let team_id_blob: Vec<u8> = team_id.as_bytes().to_vec();
params![stoolap::core::Value::blob(team_id_blob)]

// Insert (nullable): Option<Uuid> → Option<Vec<u8>>
let team_id_blob: Option<Vec<u8>> = team_id.map(|t| t.as_bytes().to_vec());
params![team_id_blob.map(stoolap::core::Value::blob)]

// Lookup: BLOB(16) → UUID
let bytes: [u8; 16] = row.get("team_id")?;
let team_id = uuid::Uuid::from_bytes(bytes);
```

## Relationship to RFC-0903-B1

RFC-0903-B1 amended only `spend_ledger` columns, leaving `api_keys` and `teams` unchanged. RFC-0903-C1 extends the same BLOB consolidation to those tables.

After both amendments:
- All primary keys that are UUIDs are `BLOB(16)`
- All foreign keys that reference UUID primary keys are `BLOB(16)`
- All FK relationships are type-consistent

**Combined RFC-0903-B1 + RFC-0903-C1 storage savings:**

| Table | Bytes saved per row |
|-------|---------------------|
| `spend_ledger` | ~52 bytes minimum (event_id 32 + key_id 20) + up to 32 for request_id + 20 for team_id |
| `api_keys` | 20 bytes (key_id 20) + up to 20 for team_id |
| `teams` | 20 bytes (team_id 20) |

## Changelog

| Version | Date       | Changes |
|---------|------------|---------|
| v1      | 2026-04-15 | Initial: amend teams.team_id, api_keys.key_id, api_keys.team_id to BLOB(16); fix FK type mismatch caused by RFC-0903-B1 leaving api_keys/teams unchanged |

---

**Draft Date:** 2026-04-15
**Version:** v1
**Amends:** RFC-0903 Final v29 + RFC-0903-B1
**Required By:** RFC-0909 (Deterministic Quota Accounting)
**Related RFCs:** RFC-0201 (Binary BLOB Type), RFC-0903-B1 (Schema Amendments)
