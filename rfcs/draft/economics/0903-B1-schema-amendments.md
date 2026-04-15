# RFC-0903-B1 (Economics): Schema Amendments to RFC-0903 Final

## Status

Draft (v7 — Amendment to RFC-0903 Final v29)

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
    request_id BLOB(32) NOT NULL,            -- Raw binary (32 bytes, SHA256 of gateway text) — RFC-0201
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
| `request_id` | `TEXT` (variable) | `BLOB(32)` (raw SHA256 bytes) | Variable; up to −32 bytes |
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
| String < 32 bytes | SHA256 of the string | `"req-123"` → SHA256 |
| String == 32 bytes | Raw bytes (already 32 bytes) | 32 raw bytes passed through |
| String > 32 bytes | SHA256 of the string | `"long-request-id-..."` → SHA256 |

> **⚠️ Hex-formatted input is not supported.** If the gateway sends a 64-char hex string (e.g., `"a1b2c3d4..."`) as input, it is SHA256-hashed as raw ASCII bytes — NOT hex-decoded first. This produces a **different** 32-byte value than hex-decoding first. Gateways MUST send raw binary/text, not hex. There is no hex-decoding path for request_id in this RFC.
>
> **⚠️ Edge case: 32-char ASCII hex is ambiguous.** If the gateway sends exactly 32 ASCII characters that happen to look like hex (e.g., `"a1b2c3d4e5f6789012345678901234ab"`), it is treated as raw text and SHA256-hashed, NOT hex-decoded. This could produce unintended results if gateways send hex-formatted IDs without their own hex layer. Gateways MUST use raw text or their own hex-encoding scheme — this RFC does not add a hex layer.

**Implementation:**

```rust
/// Encode a gateway-provided request_id string to 32 raw bytes for BLOB(32) storage.
/// All inputs are treated as raw text strings (not hex). Variable-length strings
/// are hashed via SHA256 to produce a deterministic 32-byte output.
///
/// WARNING: The gateway's input format (raw text vs hex) must be consistent across
/// all routers. A router that changes input format will produce different request_id
/// values for the same logical request, breaking idempotency.
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
/// encode_request_id() is deterministic: len==32 copies raw, else SHA256(bytes).
/// Applying this to TEXT (raw string) produces the same value as runtime inserts.
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

```
1. SELECT event_id, request_id, key_id FROM spend_ledger;  -- read TEXT values
2. For each row, compute migrate_event_id(event_id), migrate_request_id(request_id), migrate_key_id(key_id)
3. BEGIN;
4.   ALTER TABLE spend_ledger ALTER COLUMN event_id TYPE BLOB(32),
5.   ALTER COLUMN request_id TYPE BLOB(32),
6.   ALTER COLUMN key_id TYPE BLOB(16);
7.   UPDATE spend_ledger SET
8.       event_id = migrate_event_id(old_event_id_text),
9.       request_id = migrate_request_id(old_request_id_text),
10.      key_id = migrate_key_id(old_key_id_text);
11. COMMIT;
```

> **Alternative (zero-downtime):** Use a shadow column approach — add new BLOB columns alongside TEXT columns, backfill in batches, then swap columns in a subsequent release. This avoids the ALTER TYPE locking issue.

Implementations must also update `compute_event_id()` to store hex-to-binary conversion at insert time, and binary-to-hex conversion at read time.

## Relationship to RFC-0909

RFC-0909 (Deterministic Quota Accounting) adopts this amended schema. All `SpendEvent` construction and ledger recording code in RFC-0909 implementations must use the BLOB types described above.

**CRITICAL:** RFC-0903 Final's `record_spend()` and `record_spend_with_team()` functions MUST be updated to adopt RFC-0903-B1 encoding before any deployment that uses the new BLOB schema. Specifically:
- `event_id`: encode hex string → raw `BLOB(32)` via `hex_to_blob_32()` before INSERT
- `request_id`: encode raw gateway text → raw `BLOB(32)` via `encode_request_id()` before INSERT
- `key_id`: encode `uuid::Uuid` → raw `BLOB(16)` via `uuid_to_blob_16()` before INSERT

If `record_spend()` continues to use TEXT encoding while other parts of the system use BLOB encoding, the ledger will contain mixed-encoding records, breaking deterministic replay and Merkle tree construction. The entire ledger must use one encoding consistently. RFC-0903 Final must be amended (or this RFC-0903-B1 amendment explicitly scopes the required changes to `record_spend`) before deployment.

## Changelog

| Version | Date       | Changes |
|---------|------------|---------|
| v7      | 2026-04-15 | Round 12 fixes: rewrite migration as application-layer pseudocode (Rust functions not SQL UDFs), add explicit 32-char ASCII hex edge case in encoding table, add B1 adoption requirement for record_spend, update request_id Change Summary to note SHA256 |
| v6      | 2026-04-15 | Round 11 fixes: clarify request_id schema comment (SHA256 of gateway text) |
| v5      | 2026-04-15 | Round 10 fixes: fix stale string_to_blob reference in migration comment, improve event_id before/after example, add hex-formatted request_id warning |
| v4      | 2026-04-15 | Round 9 fixes: remove stale PRIMARY KEY from stoolap compat (replaced by index), fix request_id migration SQL (pad/truncate → SHA256 encode_request_id) |
| v3      | 2026-04-14 | Round 8 fixes: remove UUID encoding from request_id table (gateway provides raw text), fix encode_request_id to match actual encoding logic, clarify gateway input format (raw text, not hex) |
| v2      | 2026-04-14 | Round 7 fixes: add request_id encoding rules table + encode_request_id() function |
| v1      | 2026-04-14 | Initial amendment: event_id TEXT→BLOB(32), request_id TEXT→BLOB(32), key_id TEXT→BLOB(16); add idx_spend_ledger_event_id, idx_spend_ledger_key_created, idx_spend_ledger_pricing_hash |

---

**Draft Date:** 2026-04-15
**Version:** v7
**Amends:** RFC-0903 Final v29
**Required By:** RFC-0909 (Deterministic Quota Accounting)
**Related RFCs:** RFC-0201 (Binary BLOB Type)
