# RFC-0903-B1 (Economics): Schema Amendments to RFC-0903 Final

## Status

Draft (v2 — Amendment to RFC-0903 Final v29)

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
key_id TEXT NOT NULL  -- "550e8400-e29b-41d4-a716-446655440000" (36 chars + null)
```

UUIDs are 16 bytes. Text storage requires 36+ bytes. `BLOB(16)` reduces storage by 44%.

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
CREATE TABLE spend_ledger (
    event_id BLOB(32) NOT NULL,              -- Raw SHA256 binary (32 bytes) — RFC-0201
    request_id BLOB(32) NOT NULL,            -- Raw binary (32 bytes) — RFC-0201
    key_id BLOB(16) NOT NULL,                -- Raw UUID bytes (16 bytes) — RFC-0903-B1
    team_id TEXT,                            -- Unchanged
    provider TEXT NOT NULL,                   -- Unchanged
    model TEXT NOT NULL,                      -- Unchanged
    input_tokens INTEGER NOT NULL,            -- Unchanged
    output_tokens INTEGER NOT NULL,           -- Unchanged
    cost_amount BIGINT NOT NULL,              -- Unchanged
    pricing_hash BYTEA(32) NOT NULL,         -- Unchanged (already binary)
    timestamp INTEGER NOT NULL,               -- Unchanged
    token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
    tokenizer_version TEXT,                   -- Unchanged
    provider_usage_json TEXT,                -- Unchanged
    created_at INTEGER NOT NULL,              -- Unchanged (stoolap: INTEGER NOT NULL, app provides value)
    -- Idempotency: UNIQUE constraint prevents duplicate request_id per key
    -- Note: event_id is BLOB(32) NOT NULL, NOT a PRIMARY KEY (stoolap quirk).
    -- The RFC-0903 Final PRIMARY KEY on event_id is replaced by a regular
    -- index (idx_spend_ledger_event_id) and the UNIQUE(key_id, request_id) constraint.
    UNIQUE(key_id, request_id),              -- Unchanged
    FOREIGN KEY(key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE,
    FOREIGN KEY(team_id) REFERENCES teams(team_id) ON DELETE SET NULL
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
CREATE INDEX idx_spend_ledger_event_id ON spend_ledger(event_id);          -- NEW: RFC-0903-B1
CREATE INDEX idx_spend_ledger_key_created ON spend_ledger(key_id, created_at); -- NEW: RFC-0903-B1
CREATE INDEX idx_spend_ledger_pricing_hash ON spend_ledger(pricing_hash); -- NEW: RFC-0903-B1
```

### api_keys Table (unchanged)

The `api_keys` table schema in RFC-0903 Final is unchanged by this amendment. `key_hash BYTEA(32)` (HMAC-SHA256) is already binary and requires no amendment.

## Change Summary

| Field/Index | RFC-0903 Final | RFC-0903-B1 | Delta |
|------------|----------------|-------------|-------|
| `event_id` | `TEXT` (hex, 64 chars) | `BLOB(32)` (raw bytes) | −32 bytes/row |
| `request_id` | `TEXT` (variable) | `BLOB(32)` (raw bytes) | Variable; up to −32 bytes |
| `key_id` | `TEXT` (UUID hex, 36 chars) | `BLOB(16)` (raw bytes) | −20+ bytes/row |
| `idx_spend_ledger_event_id` | *(absent)* | Added | New |
| `idx_spend_ledger_key_created` | *(absent)* | Added | New |
| `idx_spend_ledger_pricing_hash` | *(absent)* | Added | New |

> **Note on `created_at`:** RFC-0903 Final specifies `created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))`. RFC-0903-B1 does not change this — stoolap uses `INTEGER NOT NULL` with application-provided values. The Change Summary does not list `created_at` because no change applies.

**Storage savings per spend_ledger row:** ~52 bytes minimum (event_id 32 + key_id 20) plus up to 32 more for request_id.

## API Compatibility Notes

### event_id

- **RFC-0903 Final:** `compute_event_id()` returns `String` (hex-encoded). `event_id TEXT` in schema.
- **RFC-0903-B1:** `compute_event_id()` **still returns `String` (hex-encoded)** for API/debugging compatibility. Storage uses `BLOB(32)`. When inserting, encode the hex string to raw bytes: `hex::decode(event_id)`.

```rust
// Before (RFC-0903 Final): TEXT storage
params![event_id.clone().into()]  // inserts hex string

// After (RFC-0903-B1): BLOB storage
params![stoolap::core::Value::blob(hex::decode(&event_id).unwrap())]
```

### request_id

`request_id` is provided by the API gateway as a text string. It is stored as `BLOB(32)` (32 raw bytes).

**Encoding rules (all routers MUST use the same scheme):**

| Gateway format | Encoding to 32 bytes |
|----------------|----------------------|
| UUID (36 chars) | Take first 16 bytes of UUID bytes; zero-pad remaining 16 |
| String < 32 bytes | SHA256 of the string, take first 32 bytes |
| String == 32 bytes | Raw bytes (already 32 bytes) |
| String > 32 bytes | SHA256 of the string (output is 32 bytes) |

**Implementation:**

```rust
/// Encode a gateway-provided request_id string to 32 raw bytes for BLOB(32) storage.
/// Uses SHA256 to hash variable-length strings deterministically.
pub fn encode_request_id(request_id: &str) -> [u8; 32] {
    let bytes = request_id.as_bytes();
    if bytes.len() == 32 {
        let mut out = [0u8; 32];
        out.copy_from_slice(bytes);
        out
    } else {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher.finalize().into()
    }
}
```

**Important:** The encoding scheme must be consistent across all routers. A router that changes encoding schemes will produce different `request_id` values for the same logical request, breaking idempotency. Document the chosen scheme in the deployment runbook.

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
2. `PRIMARY KEY` on `BLOB(32)` columns (event_id)

If stoolap does not support length-specified BLOBs (`BLOB(32)` vs unconstrained `BLOB`), use `VARBINARY(32)` or unconstrained `BLOB` with application-layer length validation.

## Backward Compatibility

These are **breaking schema changes**. Existing deployments must run a migration:

```sql
-- Migration: TEXT → BLOB for spend_ledger
ALTER TABLE spend_ledger
    ALTER COLUMN event_id TYPE BLOB(32) USING hex_to_blob(event_id),
    ALTER COLUMN request_id TYPE BLOB(32) USING string_to_blob(request_id),  -- pad/truncate
    ALTER COLUMN key_id TYPE BLOB(16) USING uuid_to_blob(key_id);
```

Implementations must also update `compute_event_id()` to store hex-to-binary conversion at insert time, and binary-to-hex conversion at read time.

## Relationship to RFC-0909

RFC-0909 (Deterministic Quota Accounting) adopts this amended schema. All `SpendEvent` construction and ledger recording code in RFC-0909 implementations must use the BLOB types described above.

## Changelog

| Version | Date       | Changes |
| ------- | ---------- | ------- |
| v1      | 2026-04-14 | Initial amendment: event_id TEXT→BLOB(32), request_id TEXT→BLOB(32), key_id TEXT→BLOB(16); add idx_spend_ledger_event_id, idx_spend_ledger_key_created, idx_spend_ledger_pricing_hash |

---

**Draft Date:** 2026-04-14
**Version:** v1
**Amends:** RFC-0903 Final v29
**Required By:** RFC-0909 (Deterministic Quota Accounting)
**Related RFCs:** RFC-0201 (Binary BLOB Type)
