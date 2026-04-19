# RFC-0909 (Economics): Deterministic Quota Accounting

## Status

Draft (v52 — aligned with RFC-0903 Final v29 + RFC-0903-B1 v22 + RFC-0903-C1 v3, RFC-0126 (Accepted v2.5.1), RFC-0201 (Accepted v5.24))

## Authors

- Author: @cipherocto

## Summary

This RFC defines a **deterministic quota accounting system** used by the quota router to measure, record, and enforce usage for API keys.

The system ensures that:

- usage accounting is **deterministic**
- billing records are **reproducible**
- quota deductions are **auditable**
- multi-node routers produce **identical accounting results**

This is required for future integration with:

- verifiable billing
- decentralized compute markets
- cryptographic settlement layers

## Dependencies

**Requires:**

- RFC-0903: Virtual API Key System (Final v29 + RFC-0903-B1 amendment v22 + RFC-0903-C1 amendment v3)
- RFC-0126: Deterministic Serialization (Accepted v2.5.1 — for canonical JSON serialization)
- RFC-0201: Binary BLOB Type for Deterministic Hash Storage (Accepted v5.24)

**Optional:**

- RFC-0900: AI Quota Marketplace Protocol
- RFC-0901: Quota Router Agent Specification
- RFC-0910: Pricing Table Registry (Draft v2 — for immutable pricing tables)

## Motivation

Standard API gateways track usage using **non-deterministic counters**.

Examples:

- floating point cost calculations
- asynchronous usage aggregation
- delayed billing pipelines

These approaches are unsuitable for:

- distributed execution
- cryptographic audit
- verifiable marketplaces

The quota router must produce:

```
deterministic accounting state transitions
```

such that two independent nodes processing the same requests produce identical quota results.

## Design Goals

The accounting system must guarantee:

### Determinism

All cost calculations must produce identical results across implementations.

### Atomicity

Quota deductions must occur atomically with request execution.

### Auditability

All usage events must be reproducible from logs.

### Replay Safety

Replaying the same event stream must reproduce the same quota state.

## Deterministic Cost Units

Quota usage is measured in **integer cost units**.

Floating point accounting is prohibited.

```rust
type CostUnit = u64;
```

Example unit definitions:

| Resource           | Cost Unit |
| ------------------ | --------- |
| 1 token            | 1 CU      |
| 1 prompt token     | 1 CU      |
| 1 completion token | 1 CU      |
| 1 ms GPU compute   | N CU      |

The conversion from provider billing to CU must be **deterministic and integer-based**.

## Cost Calculation

Cost is computed using deterministic rules.

```rust
// Simple cost: just tokens (result is u32; cast to u64 to match CostUnit)
let cost = (input_tokens as u64) + (output_tokens as u64);

// Or rate-based cost:
let cost = (input_tokens * prompt_rate) +
           (output_tokens * completion_rate);
```

Rates must be represented using **integer scaling**.

```rust
// 1 token = 1000 micro-cost units to avoid floating point
const TOKEN_SCALE: u64 = 1000;
```

## Usage Event Model

Each request generates a **Usage Event** (called `SpendEvent` per RFC-0903 Final).

```rust
use serde::{Deserialize, Serialize};

/// Token source for deterministic accounting
/// Uses const fn methods returning &'static str for zero-cost string access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenSource {
    /// Token counts from provider response usage metadata
    ProviderUsage,
    /// Token counts from canonical tokenizer fallback
    CanonicalTokenizer,
}

/// String conversion methods for TokenSource enum values
impl TokenSource {
    /// String used in event_id hash input (for deterministic identity)
    /// DIFFERENT from to_db_str() — shorter for compact hashing
    pub const fn to_hash_str(&self) -> &'static str {
        match self {
            TokenSource::ProviderUsage => "provider",
            TokenSource::CanonicalTokenizer => "tokenizer",
        }
    }

    /// String used in database storage (for CHECK constraint and audit)
    pub const fn to_db_str(&self) -> &'static str {
        match self {
            TokenSource::ProviderUsage => "provider_usage",
            TokenSource::CanonicalTokenizer => "canonical_tokenizer",
        }
    }

    /// Parse from database string
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "provider_usage" => Some(TokenSource::ProviderUsage),
            "canonical_tokenizer" => Some(TokenSource::CanonicalTokenizer),
            _ => None,
        }
    }
}

/// Complete spend event for deterministic accounting
/// Aligns with RFC-0903 Final §SpendEvent and RFC-0903-B1 amendment
///
/// Storage encoding per RFC-0903-B1:
/// - event_id: BLOB(32) — raw 32-byte SHA256 binary. Struct field is hex String for API compat.
/// - request_id: BLOB(32) — raw 32-byte binary (SHA256 of gateway text). Struct field is String.
///   (Gateway provides raw text; storage encodes via SHA256 per RFC-0903-B1 §request_id.)
/// - key_id: BLOB(16) — raw 16-byte UUID binary. Struct field is uuid::Uuid for type safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendEvent {
    /// Deterministic event identifier (SHA256 hex string — hex for API/debug compat)
    /// Stored as BLOB(32) per RFC-0903-B1; hex→raw via hex_to_blob_32() at insert, raw→hex via blob_32_to_hex() at read.
    /// NOTE: event_id uses hex encoding (compute_event_id returns hex). For request_id, the gateway provides
    /// raw text and storage uses SHA256 — see RFC-0903-B1 §request_id. These are different encodings.
    pub event_id: String,
    /// Request identifier for idempotency (UNIQUE constraint)
    /// Stored as BLOB(32) per RFC-0903-B1; gateway provides raw text, SHA256-encoded at insert
    /// via encode_request_id() (defined in RFC-0903-B1 §request_id) — NOT hex-encoded. See RFC-0903-B1 §request_id encoding.
    ///
    /// **Round-trip note:** request_id encoding is one-way (SHA256). The original gateway text is NOT recoverable from stored BLOB(32). An auditor verifying event_id must obtain the original gateway request_id text from the event stream — it cannot be reconstructed from spend_ledger alone. This has implications for §Audit Proof Generation: external auditors who can observe the event stream (which contains raw request_id) can verify event_id independently; auditors with only spend_ledger access cannot re-derive event_id without the original gateway text.
    ///
    /// **Type semantics:** Before persistence: raw gateway text String. After round-trip: hex-encoded
    /// SHA256 bytes. This field has different meaning before and after persistence — callers MUST NOT
    /// compare request_id values across the persistence boundary directly.
    pub request_id: String,
    /// API key that made the request
    /// Stored as BLOB(16) per RFC-0903-B1; uuid::Uuid↔[u8;16] conversion at storage boundary.
    pub key_id: uuid::Uuid,
    /// Team ID (if applicable)
    /// Stored as BLOB(16) per RFC-0903-C1; uuid::Uuid↔[u8;16] conversion at storage boundary.
    pub team_id: Option<uuid::Uuid>,
    /// Provider name
    pub provider: String,
    /// Model name
    pub model: String,
    /// Number of prompt tokens
    pub input_tokens: u32,
    /// Number of completion tokens
    pub output_tokens: u32,
    /// Total cost units (deterministic)
    pub cost_amount: u64,
    /// Pricing hash (32 bytes — fixed-size array, stored as BLOB in DB)
    /// Matches RFC-0903 Final type: [u8; 32]
    pub pricing_hash: [u8; 32],
    /// Token source for deterministic accounting (CRITICAL for cross-router determinism)
    pub token_source: TokenSource,
    /// Tokenizer ID (BLAKE3 of version string — FK to tokenizers table)
    /// Stored as BLOB(16) per RFC-0903-B1; version string → BLAKE3 at storage boundary.
    /// None when token_source is ProviderUsage.
    pub tokenizer_id: Option<[u8; 16]>,
    /// Raw provider usage JSON for audit
    pub provider_usage_json: Option<String>,
    /// Event timestamp (epoch seconds - from provider response, NOT insert time)
    pub timestamp: i64,
}

/// Generate deterministic event_id from request content
/// Aligns with RFC-0903 Final §compute_event_id
/// Returns hex-encoded SHA256 string for API compatibility.
/// Storage uses BLOB(32) per RFC-0903-B1; hex→binary conversion occurs at the storage boundary.
///
/// # UUID Format Mandate
///
/// `key_id.to_string()` uses RFC 4122 hyphenated lowercase format:
/// e.g., `"550e8400-e29b-41d4-a716-446655440000"` (36 chars with hyphens)
///
/// ALL router implementations MUST use `uuid::Uuid::to_string()` (hyphenated lowercase)
/// and MUST NOT use `to_simple().to_string()` (32-char no hyphen) or other variants.
/// A single router using a different UUID format will produce different event_id values
/// for identical requests, silently breaking cross-router determinism.
/// Test vectors in the Approval Criteria verify hyphenated lowercase format compliance.
///
/// # Security Note — No Field Delimiters
///
/// `compute_event_id` concatenates fields without length prefixes or delimiters.
/// A constructed `request_id` equal to `key_id_str + provider_bytes + model_bytes`
/// could theoretically collide with a different field combination. In practice:
/// - `key_id` is always a 36-char RFC 4122 hyphenated UUID (not user-controlled)
/// - `provider` and `model` are specified by the client in the API request
/// - In single-tenant or internally-trusted deployments, this construction is safe
///
/// **Deployments processing multi-tenant requests MUST either:**
/// 1. **Use length-prefixed encoding or field separators** in a custom `compute_event_id` variant, **or**
/// 2. **Isolate each tenant's events** so that `build_merkle_tree` is called with events
///    filtered to a single tenant's `key_id` scope — whether events reside in separate
///    tables, separate databases, or a tenant-filtered query on `spend_ledger`
///    (e.g., `WHERE key_id IN (SELECT key_id FROM api_keys WHERE team_id = $tenant_id)`).
///    A filtered view over a unified ledger is sufficient **only if** the filter is
///    applied consistently to every call that builds or verifies a Merkle proof.
/// 
/// This RFC's primary stated integration targets ("verifiable billing," "decentralized compute
/// markets," "cryptographic settlement") are inherently multi-tenant — multiple independent
/// parties who cannot be assumed to be non-adversarial. In these contexts, a malicious client
/// who knows another tenant's `key_id` could craft a `request_id` causing cross-tenant event_id
/// collision: both INSERTs succeed (different `key_id` values bypass `UNIQUE(key_id, request_id)`),
/// `build_merkle_tree` receives two leaves with identical hex strings, sorts them, and produces
/// a corrupted Merkle root — the billing proof is invalid and verification fails.
///
/// If field constraints ever relax, field separators or length-prefixed encoding
/// MUST be added immediately.
pub fn compute_event_id(
    request_id: &str,
    key_id: &uuid::Uuid,
    provider: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    pricing_hash: &[u8; 32],
    token_source: TokenSource,
) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(request_id.as_bytes());
    hasher.update(key_id.to_string().as_bytes());
    hasher.update(provider.as_bytes());
    hasher.update(model.as_bytes());
    hasher.update(input_tokens.to_le_bytes());
    hasher.update(output_tokens.to_le_bytes());
    hasher.update(pricing_hash);
    hasher.update(token_source.to_hash_str().as_bytes());
    // Return hex string (BLOB(32) storage per RFC-0903-B1; hex→raw conversion at insert)
    format!("{:x}", hasher.finalize())
}

/// Convert event_id from hex String (struct/API layer) to raw [u8; 32] for BLOB(32) storage.
/// Used at the storage boundary before INSERT per RFC-0903-B1.
///
/// NOTE: For request_id, use encode_request_id() (SHA256) — NOT this function.
/// This function hex-decodes its input. Applying it to request_id (which is SHA256-hashed,
/// not hex) would produce wrong results.
#[inline]
pub fn hex_to_blob_32(hex_str: &str) -> [u8; 32] {
    // hex_str is the ASCII hex representation (64 chars) of a 32-byte binary.
    // This is the format returned by compute_event_id() for event_id (64 hex chars).
    let bytes = hex::decode(hex_str).expect("valid hex event_id");
    let mut blob = [0u8; 32];
    blob.copy_from_slice(&bytes);
    blob
}

/// Convert event_id from raw [u8; 32] (BLOB(32) storage) to hex String for struct/API layer.
/// Used at the storage boundary after SELECT per RFC-0903-B1.
///
/// NOTE: For request_id, the stored value is raw SHA256 binary — there is no hex representation
/// and this function does NOT apply to request_id. Request_id has no API-compatible hex form.
#[inline]
pub fn blob_32_to_hex(blob: &[u8; 32]) -> String {
    hex::encode(blob)
}

/// Convert key_id from uuid::Uuid (struct/API layer) to raw [u8; 16] for BLOB(16) storage.
/// Used at the storage boundary before INSERT per RFC-0903-B1.
#[inline]
pub fn uuid_to_blob_16(uuid: &uuid::Uuid) -> [u8; 16] {
    *uuid.as_bytes()
}

/// Convert key_id from raw [u8; 16] (BLOB(16) storage) to uuid::Uuid for struct/API layer.
/// Used at the storage boundary after SELECT per RFC-0903-B1.
#[inline]
pub fn blob_16_to_uuid(blob: &[u8; 16]) -> uuid::Uuid {
    uuid::Uuid::from_bytes(*blob)
}

/// Convert tokenizer version string to tokenizer_id for BLOB(16) storage.
/// Uses BLAKE3 for deterministic 16-byte output from any-length input.
/// No DB lookup needed — version string is the source of truth, ID is derived.
/// This function is used at the storage boundary before INSERT per RFC-0903-B1.
///
/// # Truncation Note
/// BLAKE3 produces 32 bytes; this function truncates to the first 16 bytes.
/// Collision probability with 16-byte output is 1/2^64 after ~2^32 versions —
/// acceptable for tokenizer versioning use case.
///
/// # Test Vector
/// `tokenizer_version_to_id("tiktoken-cl100k_base-v1.2.3")` → `e3c8e8ff724411c6416dd4fb135368e3` (16 bytes hex)
/// Full BLAKE3: `e3c8e8ff724411c6416dd4fb135368e36b5fdcec3ecc2cd13920767ed230b103`
#[inline]
pub fn tokenizer_version_to_id(version: &str) -> [u8; 16] {
    use blake3::Hasher;
    let mut hasher = Hasher::new();
    hasher.update(version.as_bytes());
    let hash: blake3::Hash = hasher.finalize();
    // Convert to [u8; 32], then truncate to first 16 bytes
    // blake3::Hash implements Into<[u8; 32]> but NOT Into<[u8; 16]>
    let bytes: [u8; 32] = hash.into();
    bytes[..16].try_into().unwrap()
}

/// Convert tokenizer_id from raw [u8; 16] (BLOB(16) storage) to version string.
/// Used at the storage boundary after SELECT to resolve tokenizer metadata.
/// Requires a lookup against the tokenizers table.
///
/// # Error Handling
/// When fully implemented, returns `Ok(Some(version_string))` if the tokenizer exists,
/// `Ok(None)` if no matching row is found, or `Err("...")` if the lookup path is not
/// implemented. A DB-level error (connection failure, etc.) would propagate via a
/// different error path — callers should substitute `Err(KeyError::Storage)` in the
/// error arm until a unified error strategy is defined.
///
/// **Current stub:** always returns `Err("tokenizer_id_to_version: requires DB lookup
/// implementation")`. The `Ok(Some)` and `Ok(None)` cases are only reached in a real DB
/// implementation.
///
/// # Implementation
/// The implementation must use the raw 16-byte tokenizer_id as the lookup key.
/// The SQL query is: `SELECT version FROM tokenizers WHERE tokenizer_id = ?`.
/// When implemented: `Ok(Some(version_string))` on a match, `Ok(None)` on no match.
///
#[inline]
pub fn tokenizer_id_to_version(id: &[u8; 16]) -> Result<Option<String>, &'static str> {
    // This function requires a database lookup against the tokenizers table.
    // In the current implementation, this function is NOT called — tokenizers are
    // populated on-demand at INSERT time and the version string is stored in the
    // spend_ledger.provider_usage_json field for audit (format is provider-dependent;
    // no structured retrieval path is defined in this RFC).
    // The correct implementation is a DB query: SELECT version FROM tokenizers WHERE tokenizer_id = $1
    // returning Ok(Some(version)) on found, Ok(None) on not found.
    let _ = id;
    Err("tokenizer_id_to_version: requires DB lookup implementation")
}

Events represent the **canonical accounting record**.

Quota state must be derivable from the ordered sequence of events.

## Event Ordering

Events must be processed in deterministic order. Two paths exist with different ordering requirements:

**Canonical path (external verification):** `event_id ASC` only. This is the authoritative cross-router ordering — external verifiers reconstruct events from the log stream (which contains `event_id` but not `created_at`) and compute identical Merkle roots.

**Internal DB audit path:** `ORDER BY created_at ASC, event_id ASC`. Used when the router's own insertion timestamp is trusted for reconciliation. This path is described in RFC-0903 Final's §Deterministic Replay Procedure.

**Important:** `SpendEvent` struct has **no `created_at` field** — it only exists in the DB schema. In-memory replay (via `replay_events()`) sorts by `event_id` only since `created_at` is not available. The canonical Merkle path uses `event_id`-only ordering throughout.

**No ORDER BY is needed for budget state computation** (`SUM(cost_amount)`) — aggregation is order-independent. See §Budget Computation Procedure.

**Two ordering paths — two consumers:**

| Path | Ordering | Consumer |
|------|----------|----------|
| **Canonical (external verification)** | `event_id ASC` | External verifiers who reconstruct events from logs; they have the event stream but not `created_at` |
| **Internal (DB audit)** | `ORDER BY created_at ASC, event_id ASC` | Internal audit where `created_at` is authoritative insertion order |

**Critical distinction:** The Summary promise ("two independent nodes processing the same requests produce identical quota results") applies to the **canonical path** — routers processing identical requests produce identical event streams, and external verifiers reading those streams compute identical Merkle roots using `event_id`-only ordering.

The **internal DB path** is for reconciliation where the router's own `created_at` is trusted as the insertion timestamp. Two routers reading from their own DBs with different `created_at` values for the same logical events WILL produce different orderings — this is expected and by design for internal audit. It does NOT contradict the cross-router determinism guarantee, which is about the event stream path, not the DB path.

## Atomic Quota Deduction

Quota deduction must be performed atomically using the ledger-based approach (see Ledger-Based Architecture below). The ledger is the authoritative source of truth.

## Quota Consistency Model

**Critical consistency rule:**

Multiple routers processing requests simultaneously can cause **cross-router double-spend** if quota enforcement is not properly isolated.

**The double-spend problem:**

```

budget_limit = 1000
current_spend = 990

Router A reads: current_spend = 990
Router B reads: current_spend = 990

Both check: 990 + 20 ≤ 1000 ✓
Both commit: current_spend = 1030

Budget exceeded - double-spend occurred!

```

**Quota enforcement rules:**

```

1. Quota enforcement MUST occur against a strongly consistent
   primary database instance with row-level locking.

2. Routers MUST NOT enforce quotas using replica reads
   or eventually consistent storage.

3. All quota updates MUST occur via atomic SQL transactions.

4. The budget invariant MUST hold at all times:
   0 ≤ current_spend ≤ budget_limit

```

> **Note on lower bound:** The `0 ≤ current_spend` condition describes the accounting lower bound for recorded events — since `cost_amount` values are non-negative, `current_spend` (a sum of recorded costs) can never go negative. However, when an upstream request incurs cost but the ledger recording fails (e.g., BudgetExceeded error, storage failure), the true incurred cost exceeds the recorded `current_spend`. In this case `true_cost > current_spend` by the unrecorded amount, violating the stronger lower bound `true_cost - N ≤ current_spend` from §Economic Invariants. This bounded divergence is an accepted loss (§Failure Handling).

**Budget enforcement:** The ledger-based approach uses `FOR UPDATE` row locking and checks `SUM(cost_amount) <= budget_limit` atomically. Since `current_spend` is derived from the ledger (not stored), no CHECK constraint on `api_keys` is needed. The ledger INSERT itself enforces the budget via the atomic transaction pattern.

**Canonical approach:** Use `record_spend()` (key-level) or `record_spend_with_team()` (team+key) from the Ledger-Based Architecture section below. These use `FOR UPDATE` row locking and derive spend from the ledger, providing deterministic accounting.

**Single-writer principle:**

For deterministic accounting across multiple routers:

```
Router → Primary DB (strong consistency) → Usage Event Recorded
```

## Lock Ordering Invariant

**CRITICAL for transactions that lock BOTH team and key rows:**

ALL transactions that lock both `teams` and `api_keys` rows MUST acquire the team lock BEFORE the key lock to prevent deadlocks:

```
1. SELECT ... FROM teams WHERE ... FOR UPDATE
2. SELECT ... FROM api_keys WHERE ... FOR UPDATE
```

This order must be followed consistently across ALL code paths that lock both rows. Any code that violates this order risks deadlock under concurrent load.

> **Note:** `record_spend()` (key-level only) does NOT lock a team row — it locks only the key row.
> The lock ordering rule applies ONLY to `record_spend_with_team()` and similar functions that
> enforce both team and key budgets simultaneously.

See RFC-0903 Final §Lock Ordering Invariant for full specification.

## Idempotent Event Recording

To support retries, event recording must be idempotent.

Each request receives a **deterministic request_id**.

### event_id vs request_id: Two Identifiers, Two Purposes

Every `SpendEvent` carries two distinct identifiers with different encodings:

| Field | What it identifies | Encoding in struct | Storage (BLOB) | Encoding path |
|-------|--------------------|--------------------|-----------------|---------------|
| `event_id` | The complete spend event (all fields) | `String` (hex) | `BLOB(32)` | `compute_event_id()` → hex → `hex_to_blob_32()` |
| `request_id` | The provider request (idempotency key) | `String` (raw text) | `BLOB(32)` | `encode_request_id()` (SHA256 of raw text) — defined in RFC-0903-B1 §request_id |

**event_id** is derived from ALL event fields including `request_id`, `key_id`, `provider`, `model`, token counts, `pricing_hash`, and `token_source`. It is the SHA256 hash of the complete event content — two events with identical `event_id` are economically identical.

**request_id** is the gateway-provided idempotency key. It is stored as SHA256 of the raw gateway text (not hex). The `UNIQUE(key_id, request_id)` constraint prevents duplicate charging for the same logical request.

**Why two encodings?**
- `event_id` uses hex encoding because it is displayed in API responses, logs, and audit trails — hex is the human-readable form. The raw binary is for storage.
- `request_id` uses raw SHA256 because gateway text is variable-length and must be deterministically mapped to 32 bytes. SHA256 is the canonical 32-byte encoding for variable-length text. Gateways typically produce request IDs of 16–256 bytes; the validator accepts up to 1024 bytes (aligning with HTTP header size limits).

**Cross-router determinism:** Both encodings are deterministic. The same gateway `request_id` string always produces the same SHA256 `request_id` BLOB, and the same event content always produces the same hex `event_id`. Two routers processing identical requests independently produce identical `SpendEvent` records.

```rust
/// Validate request_id format and bounds
/// The request_id is provided by the API gateway, not generated here.
/// It serves as the idempotency key for deduplication.
pub fn validate_request_id(request_id: &str) -> Result<(), KeyError> {
    if request_id.is_empty() {
        return Err(KeyError::InvalidFormat);
    }
    // Reject unreasonably long request_ids to prevent storage abuse.
    // Raised from 256 to 1024 — aligns with common HTTP header size limits (8KB typical),
    // covers edge cases where providers embed metadata in request IDs, and still fits
    // comfortably in BLOB(32) storage (SHA256 output is fixed 32 bytes regardless of input).
    const MAX_REQUEST_ID_LEN: usize = 1024;
    if request_id.len() > MAX_REQUEST_ID_LEN {
        return Err(KeyError::InvalidFormat);
    }
    Ok(())
}
```

The database enforces:

```sql
UNIQUE(key_id, request_id)
```

Duplicate requests therefore cannot double charge.

## Usage Ledger

All usage events are written to a **ledger table**.

**Schema note:** Per RFC-0903-B1 amendment, `event_id` and `request_id` are stored as `BLOB(32)` (raw SHA256 binary), and `key_id` is stored as `BLOB(16)` (raw UUID bytes). The application struct uses `String` (hex) for API/debug compatibility; storage converts at the boundary. See RFC-0903-B1 §Schema Amendments for the full specification including the hex↔binary conversion rules.

```sql
-- Spend ledger - THE authoritative economic record
-- Schema per RFC-0903 Final v29 + RFC-0903-B1 amendment (BLOB storage)
-- Token counts MUST originate from provider when available (see Canonical Token Accounting)
CREATE TABLE spend_ledger (
    event_id BLOB(32) NOT NULL,              -- Raw SHA256 binary (32 bytes) — RFC-0903-B1
    request_id BLOB(32) NOT NULL,           -- Raw binary (32 bytes, SHA256 of gateway text) — RFC-0903-B1
    key_id BLOB(16) NOT NULL,                -- Raw UUID bytes (16 bytes) — RFC-0903-B1
    team_id BLOB(16),                        -- Raw UUID bytes (16 bytes) — RFC-0903-C1 (was TEXT)
    provider TEXT NOT NULL,                  -- Provider name
    model TEXT NOT NULL,                     -- Model name
    input_tokens INTEGER NOT NULL,            -- Prompt tokens
    output_tokens INTEGER NOT NULL,           -- Completion tokens
    cost_amount BIGINT NOT NULL,             -- Cost in smallest unit (u64)
    pricing_hash BYTEA(32) NOT NULL,       -- Raw SHA256 binary (32 bytes) — unchanged from RFC-0903 Final (pre-existing BYTEA type, not affected by RFC-0903-B1)
    timestamp INTEGER NOT NULL,               -- Unix epoch (authoritative event time)
    token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
    tokenizer_id BLOB(16),                   -- FK to tokenizers(tokenizer_id) — RFC-0903-B1 (was tokenizer_version TEXT)
    provider_usage_json TEXT,               -- Raw provider usage for audit
    created_at INTEGER NOT NULL,              -- Insert timestamp (app provides value at insert; no DEFAULT added per RFC-0903-B1)
    -- Idempotency: UNIQUE constraint prevents duplicate request_id per key
    -- Note: event_id is BLOB so no PRIMARY KEY (stoolap BLOB PK is supported; RFC-0903-B1
    -- uses BLOB(32) which stoolap stores as VARBINARY). Index on event_id for lookup.
    UNIQUE(key_id, request_id),
    -- Foreign keys for integrity
    FOREIGN KEY(key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE,    -- BLOB(16) → BLOB(16) — RFC-0903-C1
    FOREIGN KEY(team_id) REFERENCES teams(team_id) ON DELETE SET NULL,    -- BLOB(16) → BLOB(16) — RFC-0903-C1
    FOREIGN KEY(tokenizer_id) REFERENCES tokenizers(tokenizer_id) ON DELETE SET NULL  -- BLOB(16) → BLOB(16) — RFC-0903-B1
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
-- Pre-existing index from RFC-0903 Final (not used in deterministic replay path)
CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);
CREATE INDEX idx_spend_ledger_event_id ON spend_ledger(event_id);  -- RFC-0903-B1 ext
-- Composite index for efficient replay with ORDER BY created_at — RFC-0903-B1 ext
CREATE INDEX idx_spend_ledger_key_created ON spend_ledger(key_id, created_at);
-- Index for pricing verification queries — RFC-0903-B1 ext
CREATE INDEX idx_spend_ledger_pricing_hash ON spend_ledger(pricing_hash);
-- Index for tokenizer lookup — RFC-0903-B1 ext
CREATE INDEX idx_spend_ledger_tokenizer ON spend_ledger(tokenizer_id);
```

## Replay and Verification

Quota state must be reproducible via replay.

```rust
/// Reconstruct quota state from events (in-memory struct replay)
/// Uses BTreeMap for deterministic iteration ordering
///
/// Note: The SpendEvent struct has no `created_at` field (it is DB schema only).
/// Therefore in-memory replay uses event_id for canonical ordering.
/// For database-level replay (SQL), use: ORDER BY created_at ASC, event_id ASC
/// (created_at is the authoritative insertion order; event_id is the tiebreaker).
///
/// NOTE: This function returns per-key spend aggregates suitable for quota
/// enforcement and budget checks. It is NOT suitable for Merkle proof
/// generation — see `build_merkle_tree()` instead.
pub fn replay_events(events: &[SpendEvent]) -> std::collections::BTreeMap<String, u64> {
    use std::collections::BTreeMap;

    let mut key_spend: BTreeMap<String, u64> = BTreeMap::new();

    // In-memory struct replay: sort by event_id for deterministic ordering
    // (SpendEvent has no created_at field — DB-level replay uses different ordering)
    let mut sorted_events = events.to_vec();
    sorted_events.sort_by(|a, b| {
        a.event_id.cmp(&b.event_id)
    });

    for event in sorted_events {
        // key_id is uuid::Uuid — to_string() creates a String each iteration
        // BTreeMap<String, u64> requires String keys
        let key = event.key_id.to_string();
        let entry = key_spend.entry(key).or_insert(0);
        // saturating_add: in-memory replay uses saturation for best-effort audit.
        // Live quota enforcement (record_spend in RFC-0903 Final) uses checked arithmetic
        // and returns Err on overflow. Overflow here requires >1.8×10^19 micro-units total
        // spend — effectively impossible in practice.
        *entry = entry.saturating_add(event.cost_amount);
    }

    key_spend
}

> **Note:** `replay_events_for_proof` is removed pending a defined consumer. Per-key grouped event detail is not currently required by any verification path in this spec. It may be re-introduced in a future RFC when a concrete use case is specified. See §Audit Proof Generation for the canonical Merkle tree path.

Verification nodes can reconstruct:

- total spend
- quota exhaustion
- billing totals

**Budget Computation Procedure:**

For budget state computation and ledger reconciliation, budget state computation MUST follow this procedure:

```
1. Load all spend_ledger for a key_id
2. Compute current_spend = SUM(events.cost_amount)
3. Verify against ledger-derived balance (not stored counter)
```

**Note:** No ORDER BY is needed for SUM — aggregation is order-independent. The `ORDER BY created_at ASC, event_id ASC` in historical versions was for deterministic replay ordering (ensuring events were processed in insertion order when reconstructing state). It is not needed for aggregate computation.

**Scope:** This procedure computes budget state (aggregate spend totals) for quota enforcement. It is order-independent for aggregate computation — `SUM(cost_amount)` produces the same result regardless of row ordering. It is NOT the Merkle tree construction procedure.

**Merkle tree path:** See §Audit Proof Generation (`build_merkle_tree`). The Merkle tree uses `event_id`-only ordering and a separate code path from this procedure. The two procedures are unrelated and serve different consumers.

### Economic Invariants

The following invariants MUST hold at all times:

```
1. spend_ledger are the authoritative economic record
2. current_spend = SUM(spend_ledger.cost_amount)
3. true_cost - N ≤ current_spend ≤ budget_limit
   where N is the accumulated truncation error (bounded at <2 micro-units per event)
   NOTE: current_spend is the sum of integer-truncated cost_amount values.
   Truncation-based under-billing means current_spend always understates true cost.
   The invariant lower bound (true_cost - N) captures this relationship.
   N accounts for two independent truncation operations per event (prompt_cost
   and completion_cost divisions). For each division, truncation error is bounded
   by the remainder of (tokens * rate) modulo 1000, which is always <1 micro-unit.
   With two divisions per event, the per-event bound is <2 micro-units total.
   IMPORTANT: This invariant applies to recorded events only. If execute_request()
   succeeds but record_spend() fails (BudgetExceeded, storage error), the upstream
   cost is incurred but NOT recorded. In this case, true_cost > current_spend by
   the unrecorded amount — a BudgetExceeded failure violates the lower bound.
   This is an accepted bounded loss (see §Failure Handling).
4. For the same `(key_id, request_id)` pair, `UNIQUE` constraint ensures exactly one INSERT succeeds — duplicate requests are deduplicated at the schema level. Double-charging via different `request_id` values for the same logical event is the caller's responsibility to prevent (see §Known Limitations).
5. pricing_hash ensures deterministic cost calculation
6. token_source MUST be identical across routers for a given request_id
```

**On cost_amount in event_id:** `event_id` is computed from `pricing_hash` + token counts, but NOT from `cost_amount`. This is a deliberate design choice: if cost_amount were included in event_id, a retry that computes a different cost_amount (due to rounding, timing, or pricing table update) would produce a different event_id and NOT be deduplicated by `UNIQUE(key_id, request_id)`. The current design prioritizes idempotency (retries with the same request_id are deduplicated regardless of cost) over cost-bug detection. A pricing table bug would cause the same incorrect cost to be recorded on every retry, rather than being detected as a divergence.

### Rate Limiting Determinism

```
Rate limiting decisions MUST NOT influence spend recording.

If a provider request executed → spend MUST be recorded.
Even if rate limiter would have denied the request locally.
Rate limiting uses non-deterministic clocks (Instant) and is separate from accounting.
```

## Deterministic Pricing Tables

Provider prices must be represented as deterministic tables.

```rust
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Pricing model for a single model
/// Uses BTreeMap for deterministic iteration (RFC-0126)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingModel {
    pub model_name: String,
    /// Cost per 1K prompt tokens (in micro-units)
    pub prompt_cost_per_1k: u64,
    /// Cost per 1K completion tokens (in micro-units)
    pub completion_cost_per_1k: u64,
}

/// Global pricing table using BTreeMap for deterministic serialization
/// Keys are sorted for consistent hash computation
pub struct PricingTable {
    /// Model name → PricingModel lookup
    /// BTreeMap provides deterministic iteration order (RFC-0126)
    models: BTreeMap<String, PricingModel>,
}

impl PricingTable {
    /// Create new pricing table with built-in models
    pub fn new() -> Self {
        let mut models = BTreeMap::new();

        // GPT-4 models
        models.insert(
            "gpt-4".to_string(),
            PricingModel {
                model_name: "gpt-4".to_string(),
                prompt_cost_per_1k: 30_000,  // $0.03 per 1K
                completion_cost_per_1k: 60_000, // $0.06 per 1K
            },
        );
        models.insert(
            "gpt-4o".to_string(),
            PricingModel {
                model_name: "gpt-4o".to_string(),
                prompt_cost_per_1k: 5_000,   // $0.005 per 1K
                completion_cost_per_1k: 15_000, // $0.015 per 1K
            },
        );

        // GPT-3.5 models
        models.insert(
            "gpt-3.5-turbo".to_string(),
            PricingModel {
                model_name: "gpt-3.5-turbo".to_string(),
                prompt_cost_per_1k: 500,    // $0.0005 per 1K
                completion_cost_per_1k: 1_500, // $0.0015 per 1K
            },
        );

        // Claude models (example pricing)
        models.insert(
            "claude-3-opus".to_string(),
            PricingModel {
                model_name: "claude-3-opus".to_string(),
                prompt_cost_per_1k: 15_000,  // $0.015 per 1K
                completion_cost_per_1k: 75_000, // $0.075 per 1K
            },
        );

        Self { models }
    }

    /// Look up pricing for a model
    pub fn get(&self, model: &str) -> Option<&PricingModel> {
        self.models.get(model)
    }

    /// Compute SHA256 pricing hash for this table snapshot
    /// Used in event_id to tie costs to specific pricing version
    ///
    /// Note: For full RFC-0126 determinism, a canonical JSON serializer is required.
    /// BTreeMap guarantees sorted key iteration at the map level, but struct field
    /// ordering in JSON serialization is not guaranteed by serde_json.
    /// A proper canonical JSON implementation (RFC-8785, e.g., `serde_json_raw` crate)
    /// MUST be used — pricing_hash is embedded in event_id, and any serde_json field
    /// ordering divergence between routers produces different event_id values for
    /// identical requests, silently breaking the cross-router determinism guarantee.
    ///
    /// ⚠️  The code below uses `serde_json::to_string` for clarity — this is NOT
    /// production code. serde_json does NOT produce canonical JSON; struct field
    /// ordering may vary across compiler versions. Production implementations MUST
    /// use an RFC 8785 canonical JSON library (e.g., `serde_json_raw`). The test
    /// vectors in the Approval Criteria are computed with a compliant implementation
    /// and MUST be matched exactly.
    pub fn compute_pricing_hash(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};

        // ⚠️  Example only — NOT for production. See comment above.
        let serialized = serde_json::to_string(&self.models)
            .expect("PricingTable serialization must succeed");
        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        hasher.finalize().into()
    }

    /// Get all models (for listing)
    pub fn models(&self) -> impl Iterator<Item = &PricingModel> {
        self.models.values()
    }
}

impl Default for PricingTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute cost deterministically using integer arithmetic.
/// Name aligns with RFC-0903 Final §compute_cost.
///
/// # Parameters
/// - `pricing`: the PricingModel for the model being charged
/// - `input_tokens`: number of prompt tokens consumed
/// - `output_tokens`: number of completion tokens generated
///
/// # Returns
/// Total cost in micro-units (u64). Uses integer division with truncation.
/// Cost is computed as: `(input_tokens * prompt_cost_per_1k / 1000) + (output_tokens * completion_cost_per_1k / 1000)`
///
/// # Truncation Note
/// Integer division truncates toward zero. For micro-unit pricing, truncation
/// error is bounded at <2 micro-units per event (<1 per division step). This is
/// the same truncation bound documented in §Economic Invariants (Invariant #3).
pub fn compute_cost(
    pricing: &PricingModel,
    input_tokens: u32,
    output_tokens: u32,
) -> u64 {
    // Integer math only — no floating point
    // Uses integer division (truncates toward zero). For micro-unit pricing,
    // truncation occurs only when cost < 0.5 micro-units (effectively free).
    // 1000 = TOKEN_SCALE (micro-units per token)
    let prompt_cost = (input_tokens as u64 * pricing.prompt_cost_per_1k) / 1000;
    let completion_cost = (output_tokens as u64 * pricing.completion_cost_per_1k) / 1000;

    // saturating_add: overflow requires >1.8×10^19 micro-units in a single request —
    // effectively impossible in practice. Live record_spend (RFC-0903 Final) uses
    // checked arithmetic on the per-key budget accumulation.
    prompt_cost.saturating_add(completion_cost)
}

All values stored as integer micro-units.

## Canonical Token Accounting

**Critical determinism rule:**

Two routers processing the same request MUST produce identical token counts, otherwise deterministic accounting fails.

**The token drift problem:**

Different routers may measure tokens differently due to:

- Tokenizer version differences
- Whitespace normalization differences
- Streaming chunk boundary differences
- Provider returning different usage metadata

This causes **deterministic accounting failure** where the same request produces different costs.

**Canonical token source rule:**

```
Priority 1: Provider-reported tokens (from response.usage)
Priority 2: Canonical tokenizer (pinned implementation per RFC-0910)
Priority 3: REJECT - cannot account without verifiable source
```

```
Local tokenizer estimation MUST NOT be used for accounting.
```

**Pricing hash determinism:**

```
pricing_hash = SHA256(canonical pricing table JSON)
```

This ensures pricing determinism is defined. RFC-0910 will provide immutable pricing table snapshots.

**CRITICAL invariant:**

```
For a given request_id, ALL routers MUST use the SAME token_source.
token_source MUST be included in event_id hash.
```

**Known limitation:** If two routers process the same `(key_id, request_id)` simultaneously with different `token_source` values (e.g., Router A sees ProviderUsage, Router B sees CanonicalTokenizer on retry), they compute different `event_id` values. However, the `UNIQUE(key_id, request_id)` constraint prevents a second INSERT with the same `(key_id, request_id)` from succeeding — stoolap fully enforces UNIQUE constraints on BLOB columns (verified in stoolap at commit `28e3e513baf2a34d7989ff2c2cdce84f5b6178fb`). The second router receives an idempotent success — no double-insertion occurs.

The actual retry double-charge scenario: if a client retries with a *different* `request_id` (e.g., `"req-abc"` → `"req-abc-retry"`), both INSERTs succeed with different `request_id` BLOBs. Both events record the same token consumption, and the client is charged twice. This is correct idempotency behavior for the schema — the client provided two different idempotency keys, so the schema treats them as two different requests. This is NOT a limitation — it is the defined behavior of idempotency keys.

The genuine application-bug limitation (original RC3 concern): if `record_spend()` produces two rows for the same logical event (different `request_id` BLOBs, same economic content, same tokens) due to an internal bug, `build_merkle_tree` will double-count the cost without error. There is no schema-level enforcement against this class of bug. Preventing it requires correct implementation of `record_spend` — specifically, that callers of `record_spend` are responsible for providing the same `request_id` for the same logical event.

```
For a given request_id, only ONE usage event may exist.
This is enforced by UNIQUE(key_id, request_id) constraint.
```

## Provider Usage Reconciliation

Upstream provider responses may contain usage metadata.

The router must recompute cost using **its own pricing tables**, ignoring provider cost fields.

> **IMPORTANT:** `process_response` below is **PSEUDOCODE** demonstrating how RFC-0903 Final's
> `record_spend` integrates into the request lifecycle. It is NOT a specification of a new function.
> RFC-0903 Final defines `record_spend(db, key_id, event)` and `record_spend_with_team(db, key_id, team_id, event)`.
> The `ProviderResponse` type and tokenizer detection logic are Quota Router implementation concerns,
> not quota accounting specification (see RFC-0910 for tokenizer management).

**Pseudocode — DO NOT COPY AS-IS:**

```rust
/// Process response and record usage (pseudocode per RFC-0903 Final)
///
/// Uses provider-reported tokens and deterministic event_id for cross-router determinism.
/// Calls `record_spend()` from RFC-0903 Final for atomic budget enforcement.
///
/// # Integration Pattern
/// 1. Execute provider request
/// 2. Build SpendEvent from response
/// 3. Call record_spend() (or record_spend_with_team()) atomically
///
/// # Return Value
/// Returns `Ok(())` on success (aligned with RFC-0903 Final's record_spend).
/// On duplicate request_id: silently succeeds (idempotent via UniqueConstraint).
/// On budget exceeded: returns `Err(KeyError::BudgetExceeded)`.
///
/// # Error Handling
/// - `KeyError::BudgetExceeded` → return 429 to client, do NOT return provider response
/// - `KeyError::Storage` → return 500 to client, do NOT return provider response
/// - Duplicate request_id → silently idempotent (safe to retry)
pub async fn process_response(
    db: &Database,
    key_id: &uuid::Uuid,
    team_id: Option<&uuid::Uuid>,
    provider: &str,
    model: &str,
    response: &ProviderResponse,
    pricing_hash: [u8; 32], // obtained by: PRICING_TABLE.get(model).compute_pricing_hash()
) -> Result<(), KeyError> {
    // 1. Determine token source and tokenizer ID
    let (token_source, tokenizer_id) = match response.usage.is_some() {
        true => (TokenSource::ProviderUsage, None),
        false => {
            let version = get_canonical_tokenizer(model);
            let id = tokenizer_version_to_id(version); // BLAKE3 truncated to 16 bytes (see line 352)
            (TokenSource::CanonicalTokenizer, Some(id))
        },
    };

    // 2. Validate request_id (for idempotency integrity)
    validate_request_id(&response.request_id)?;

    // 3. Look up pricing (should be cached singleton in production — see §PricingTable Caching)
    let pricing = PRICING_TABLE.get(model).ok_or(KeyError::NotFound)?;
    let cost_amount = compute_cost(pricing, response.input_tokens, response.output_tokens);

    // 4. Generate deterministic event_id (matches RFC-0903 Final §compute_event_id)
    let event_id = compute_event_id(
        &response.request_id,
        key_id,
        provider,
        model,
        response.input_tokens,
        response.output_tokens,
        &pricing_hash,
        token_source,
    );

    // 5. Build SpendEvent (matches RFC-0903 Final §SpendEvent)
    let event = SpendEvent {
        event_id,
        request_id: response.request_id.clone(),
        key_id: *key_id,
        team_id: team_id.copied(),
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
        cost_amount,
        pricing_hash,
        token_source,
        tokenizer_id,
        provider_usage_json: response.provider_usage_json.clone(),
        timestamp: response.timestamp,
    };

    // 6. Record spend via RFC-0903 Final ledger-based function
    //    - record_spend(db, key_id, &event) for key-level budget
    //    - record_spend_with_team(db, key_id, team_id, &event) for team-level budget
    match team_id {
        Some(tid) => record_spend_with_team(db, key_id, tid, &event)?,
        None => record_spend(db, key_id, &event)?,
    };

    Ok(())
}
```

> **RFC-0910 Concern:** The `get_canonical_tokenizer()` function and tokenizer version
> management are part of RFC-0910 (Pricing Table Registry), not RFC-0909.
> RFC-0909 only specifies that `token_source` must be included in event_id hashing.

**Failure handling note:** The provider request is an external HTTP call outside the database transaction. If the provider succeeds but `process_response` fails, the response has already been consumed. The compensating approach is to use idempotent `request_id` for retries — if a retry arrives with the same `request_id`, the UniqueConstraint error causes the ledger INSERT to be silently skipped, preventing double-billing.

## Overflow Safety

All accounting variables must use:

```rust
u64
```

Maximum supported spend:

```
18,446,744,073,709,551,615 CU
```

Overflow must be treated as a fatal error.

```rust
fn checked_add_spend(current: u64, add: u64) -> Result<u64, KeyError> {
    current
        .checked_add(add)
        .ok_or_else(|| KeyError::Storage("overflow detected".to_string()))
}
```

Note: `KeyError::Storage` is used for overflow errors; a dedicated `KeyError::Overflow` variant may be added in future RFC-0903 revisions.

## Audit Proof Generation

The event ledger can be extended to generate **cryptographic proofs**. The `build_merkle_tree()` function below is fully specified. The "(Future)" designation on this section refers to the full verification system integration (proof relay, challenge protocol, verifier enrollment) — the deterministic Merkle tree construction itself is a specified component.

```rust
use sha2::{Digest, Sha256};

/// Merkle tree node — each node stores its hash and child references.
/// The root node's hash is the published Merkle root.
#[derive(Debug, Clone)]
pub struct MerkleNode {
    /// Hash of this node (leaf: event hash, internal: hash of children)
    pub hash: [u8; 32],
    /// Left child (None for leaf nodes)
    pub left: Option<Box<MerkleNode>>,
    /// Right child (None for leaf nodes)
    pub right: Option<Box<MerkleNode>>,
}

/// Build Merkle tree from usage events.
///
/// Each leaf is the SHA256 hash of: event_id (hex String as ASCII bytes) + cost_amount.
/// Internal nodes are the SHA256 hash of their two child hashes concatenated.
/// The root hash is published for cryptographic proofs.
///
/// Note: In the database, event_id is stored as BLOB(32) (raw binary) per RFC-0903-B1.
/// In the application struct (SpendEvent), event_id is String (hex). This pseudocode
/// uses the application struct's hex String field — routers can compute identical roots
/// from their logs without needing database access. Hashing the raw BLOB would produce
/// different results than what routers can independently derive.
///
/// DB-based routers (reading event_id from storage rather than in-memory structs) MUST
/// convert BLOB(32) → hex string via `blob_32_to_hex()` before computing Merkle leaves.
/// Hashing the raw 32-byte BLOB directly produces a different leaf hash than hashing the
/// 64-char hex string — roots built from different representations will not match.
pub fn build_merkle_tree(events: &[SpendEvent]) -> Option<MerkleNode> {
    let mut sorted = events.to_vec();
    sorted.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    // Empty ledger — return None (no root to publish)
    if sorted.is_empty() {
        return None;
    }

    // Build leaf nodes: hash(event_id_hex_as_bytes || cost_amount)
    let leaves: Vec<MerkleNode> = sorted
        .iter()
        .map(|e| {
            let mut hasher = Sha256::new();
            hasher.update(e.event_id.as_bytes()); // hex string as ASCII bytes
            hasher.update(e.cost_amount.to_le_bytes());
            let result = hasher.finalize();
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&result);
            MerkleNode { hash, left: None, right: None }
        })
        .collect();

    // Recursively build internal nodes from leaf pairs
    fn build_parent_level(children: Vec<MerkleNode>) -> Vec<MerkleNode> {
        if children.is_empty() {
            return Vec::new();
        }
        // Pad with duplicate of last child if odd count (keeps tree balanced and deterministic)
        let mut nodes: Vec<MerkleNode> = children;
        if nodes.len() % 2 == 1 {
            let last = nodes.last().unwrap().clone();
            nodes.push(last);
        }
        let mut parents = Vec::new();
        for pair in nodes.chunks(2) {
            let mut hasher = Sha256::new();
            hasher.update(&pair[0].hash);
            hasher.update(&pair[1].hash);
            let result = hasher.finalize();
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&result);
            parents.push(MerkleNode {
                hash,
                left: Some(Box::new(pair[0].clone())),
                right: Some(Box::new(pair[1].clone())),
            });
        }
        parents
    }

    // Build tree bottom-up until a single root remains
    let mut level = leaves;
    while level.len() > 1 {
        level = build_parent_level(level);
    }

    level.pop()
}

Root hashes can be published periodically.

**Canonical Merkle root for external verification:** The **canonical Merkle root** for verifiable billing (external verifier use case) is built by `build_merkle_tree()` with `event_id`-only ordering (see §Audit Proof Generation). The external verifier has access to the event stream (which contains `event_id` but not `created_at`).

The DB-level ordering (`ORDER BY created_at ASC, event_id ASC`) is for **internal audit** where the insertion timestamp is authoritative. It does NOT produce the published canonical root used for external verification. Implementations that build Merkle trees from database rows for external publication must use the event_id-only ordering.

This enables:

- verifiable billing
- decentralized settlement
- marketplace proofs

## Failure Handling

If accounting fails after request execution:

```
request result must not be returned
```

Accounting must be treated as part of the **transaction boundary**.

**Upstream cost loss (known limitation):** If `record_spend()` fails after `execute_request()` succeeds (budget exceeded, storage error, lock timeout), the provider has already charged the upstream account. The ledger is not updated, but the upstream cost is unrecoverable. This is an accepted bounded loss for the current design — future work may add pre-execution budget reservation (two-phase commit pattern) to eliminate it.

**Pseudocode — calls `process_response` which is also pseudocode. DO NOT COPY AS-IS.**

```rust
/// Process request with accounting as part of transaction (pseudocode)
pub async fn process_request_with_accounting(
    db: &Database,
    request: &Request,
    pricing_hash: [u8; 32],
) -> Result<Response, KeyError> {
    // Execute request to provider first (outside the DB transaction).
    // Budget enforcement (FOR UPDATE row lock + INSERT) happens inside process_response,
    // which uses record_spend() / record_spend_with_team() from RFC-0903 Final.
    // The FOR UPDATE lock makes the check-and-record atomic — concurrent requests
    // all attempt the lock on the same row; only one succeeds until the transaction
    // commits and releases the lock. This is the correct pattern per the Consistency Model.
    //
    // NOTE: A request that would exceed the budget is still executed (execute_request
    // runs first). If the budget check fails inside process_response, the provider
    // has already been called and billed. This is intentional: we record the spend
    // regardless of outcome (Idempotency via UNIQUE constraint prevents double-recording).
    // The caller receives KeyError::BudgetExceeded only after the ledger INSERT attempt.
    let response = execute_request(request).await?;

    // Record spend via ledger (atomic budget enforcement)
    // Note: process_response handles its own transaction internally
    process_response(
        db,
        &request.key_id,
        request.team_id.as_deref(),
        request.provider,
        request.model,
        &response,
        pricing_hash,
    )
    .await?;

    // Return response only after successful accounting
    Ok(response)
}
```

## Constraints

### Multi-Node Clock Synchronization

For horizontal scaling with `ORDER BY created_at ASC, event_id ASC` to produce identical replay ordering across nodes, all router nodes MUST have synchronized clocks via NTP. If clocks differ between nodes by even one second, the insertion ordering (`created_at`) will diverge, producing different event orderings for the same logical event stream.

**This is an operational constraint, not a schema deficiency.** The `event_id` tiebreaker does not resolve cross-node clock skew — if Node A's `created_at` is 1000 and Node B's is 1001 for the same logical event, they will have different orderings regardless of `event_id`. The tiebreaker only resolves same-second ties on the same node.

**Mitigation:** Deploy NTP time synchronization across all router instances. Clock skew between nodes greater than 1 second may cause deterministic replay divergence.

## Known Limitations

The following limitations are inherent to the current design and are NOT resolvable without architectural changes:

### Upstream cost loss on record_spend failure

If `execute_request()` succeeds (upstream provider has charged the account) but `record_spend()` fails (BudgetExceeded, storage error, lock timeout), the ledger is not updated but the upstream cost is unrecoverable. This is an accepted bounded loss for the current design. A future two-phase commit pattern (pre-execution budget reservation) could eliminate this limitation.

### Application bug double-charge (internal record_spend bug)

If `record_spend()` produces two rows for the same logical event (different `request_id` BLOBs, same economic content, same tokens) due to an internal bug, `build_merkle_tree` will double-count the cost without error. There is no schema-level enforcement against this class of bug. Correct implementation of `record_spend` is the caller's responsibility.

### Token source divergence on retry

If two routers process the same `(key_id, request_id)` simultaneously with different `token_source` values (e.g., Router A sees ProviderUsage, Router B sees CanonicalTokenizer on retry), they compute different `event_id` values. The `UNIQUE(key_id, request_id)` constraint prevents a second INSERT — the second router receives idempotent success. No double-insertion occurs, but one router's Merkle root will not include that event in the same position as the other.

### BLAKE3 truncation for tokenizer_id

`tokenizer_version_to_id` truncates BLAKE3 to 16 bytes (line 352). Collision probability becomes non-negligible after ~2^32 versions — acceptable for tokenizer versioning but documented for completeness.

**Note on NTP constraint:** See `## Constraints § Multi-Node Clock Synchronization`. The NTP clock synchronization requirement is fully documented there and is not repeated here.

## Security Considerations

### Replay protection

`request_id` prevents duplicate charging.

### Tamper detection

Ledger entries must be append-only.

### Provider mismatch

Router pricing tables override provider pricing.

## Performance Characteristics

Expected overhead per request:

| Step              | Cost   |
| ----------------- | ------ |
| Cost calculation  | <10µs  |
| Atomic SQL update | ~1ms   |
| Ledger write      | ~0.5ms |

Total accounting overhead:

```
~1–2ms
```

## Ledger-Based Architecture

RFC-0909 follows a **ledger-based architecture** for deterministic quota accounting.

**Core principle:**

```
spend_ledger is the authoritative economic record.
All balances MUST be derived from the ledger.
```

This simplifies the system and makes it more deterministic:

- Single source of truth
- Deterministic replay is trivial
- No counter drift
- Easy audit and verification
- Enables cryptographic proofs later

**Key architectural points:**

1. **Ledger is authoritative** - All economic events are appended to `spend_ledger`
2. **Balances are derived** - `current_spend` is computed from ledger, not stored
3. **Idempotent events** - `UNIQUE(key_id, request_id)` prevents double charging
4. **Deterministic event_id** - SHA256 hex hash ensures same request = same event across routers

**Quota enforcement with row locking:**

CRITICAL: To prevent race conditions in multi-router deployments, quota enforcement MUST use `FOR UPDATE` row locking.

**Lock ordering (critical for team + key transactions):**

```
ALWAYS: team row FIRST, key row SECOND
```

Any deviation risks deadlock.

**record_spend function (per RFC-0903 Final §record_spend):**

```rust
/// Record spend event in ledger with atomic budget enforcement.
/// Uses FOR UPDATE row locking to prevent double-spend in multi-router deployments.
///
/// Implementation: see RFC-0903 Final §record_spend and §record_spend_with_team
///
/// # Key-Level (no team budget)
/// record_spend(db, key_id, &event) → locks only the key row
///
/// # Team-Level (team budget enforcement)
/// record_spend_with_team(db, key_id, team_id, &event) → locks team FIRST, key SECOND
/// (Lock ordering is ONLY relevant for team+key transactions — single-key uses key-only lock)
```

**Deterministic replay:**

```
1. SELECT * FROM spend_ledger ORDER BY created_at ASC, event_id ASC
2. Recompute balances from ledger
3. Verify ledger-derived balance against enforcement check
```

**Long-term enablement:**

Ledger architecture enables:

```
- Merkle root of usage ledger
- Cryptographic spend proofs
- Economic verification
- Verifiable AI infrastructure
```

## Relationship to RFC-0903

RFC-0903 defines:

```
authentication
authorization
rate limits
budgets
spend_ledger table schema (Final v29 + RFC-0903-B1 amendment)
```

RFC-0909 defines:

```
how usage is measured and deducted
```

Together they form the **quota router economic core**.

RFC-0909 adopts RFC-0903's `spend_ledger` schema as the canonical ledger. Both RFCs now share the same data model:

- `SpendEvent` struct (RFC-0909) matches `SpendEvent` struct (RFC-0903 Final)
- `compute_event_id()` aligns exactly with RFC-0903 Final
- `TokenSource` enum with `to_hash_str()` and `to_db_str()` methods
- Lock ordering invariant (team FIRST, key SECOND)

### RFC-0903-B1 Amendment (Schema Optimizations)

RFC-0903-B1 (an amendment to RFC-0903 Final) makes the following changes to the `spend_ledger` schema:

| Field | RFC-0903 Final | RFC-0903-B1 | Reason |
|-------|---------------|-------------|--------|
| `event_id` | `TEXT` (hex, 64 chars) | `BLOB(32)` (raw SHA256) | 50% storage reduction; RFC-0201 |
| `request_id` | `TEXT` (variable) | `BLOB(32)` (raw SHA256 bytes) | Consistent 32-byte storage; RFC-0201 |
| `key_id` | `TEXT` (UUID hex, 36 chars) | `BLOB(16)` (raw UUID bytes) | 56% storage reduction; RFC-0903-B1 |
| `idx_spend_ledger_key_time` | *(none)* | *(unchanged)* | Pre-existing legacy index (not used in deterministic replay path) |
| `idx_spend_ledger_key_created` | *(none)* | Added | Efficient `ORDER BY created_at` queries |
| `idx_spend_ledger_event_id` | *(none)* | Added | Equality lookup on event_id |
| `idx_spend_ledger_pricing_hash` | *(none)* | Added | Pricing verification queries |
| `created_at` | `INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))` | *(unchanged)* | No DEFAULT added per RFC-0903-B1; app provides value at insert |

> **Note:** `event_id` is stored as raw binary (32 bytes) per RFC-0201, not hex-encoded text. For display/debugging, convert using `hex::encode(event_id)`. The `compute_event_id()` function continues to return `String` (hex) for API compatibility; storage uses binary. API key material (key_hash) is already stored as `BLOB(32)` per RFC-0903 Final.

### RFC-0903-C1 Amendment (FK Consistency)

RFC-0903-C1 extends RFC-0903-B1 to amend `api_keys` and `teams` tables, fixing the FK type mismatch created by RFC-0903-B1 (which amended `spend_ledger.key_id` to `BLOB(16)` but left `api_keys.key_id` as `TEXT`).

RFC-0903-C1 changes:

| Field | RFC-0903 Final | RFC-0903-C1 | Reason |
|-------|---------------|-------------|--------|
| `teams.team_id` | `TEXT` (UUID hex, 36 chars) | `BLOB(16)` (raw UUID bytes) | 56% storage reduction; consistent FK |
| `api_keys.key_id` | `TEXT` (UUID hex, 36 chars) | `BLOB(16)` (raw UUID bytes) | 56% storage reduction; consistent FK |
| `api_keys.team_id` | `TEXT` (UUID nullable) | `BLOB(16)` (raw UUID bytes) | 56% storage reduction; consistent FK |
| `idx_teams_team_id` | *(on TEXT)* | *(on BLOB(16))* | Updated index |
| `idx_api_keys_team_id` | *(on TEXT)* | *(on BLOB(16))* | Updated index |

After RFC-0903-C1, all UUID primary keys and their foreign keys are `BLOB(16)`, and all FK relationships are type-consistent.

> **Note:** RFC-0903-B1 defined `team_id TEXT` in `spend_ledger` as unchanged to avoid amending `teams`. RFC-0903-C1 resolves this by also amending `teams.team_id`, allowing `spend_ledger.team_id` to be `BLOB(16)` consistently.

See `rfcs/draft/economics/0903-B1-schema-amendments.md` for the full RFC-0903-B1 amendment text.

## Approval Criteria

This RFC can be approved when:

- [x] deterministic cost units are implemented
- [x] spend_ledger is append-only (per RFC-0903)
- [x] atomic quota deduction is implemented
- [x] idempotent request accounting exists
- [x] types align with RFC-0903 Final v29 + RFC-0903-B1 amendment
- [x] lock ordering invariant is documented
- [x] TokenSource uses lookup tables (no allocation)
- [x] TokenSource hash strings match RFC-0903 Final (`"provider"`/`"tokenizer"`)
- [x] schema adopts RFC-0903-B1/C1 BLOB storage (event_id BLOB(32), request_id BLOB(32), key_id BLOB(16), team_id BLOB(16), tokenizer_id BLOB(16), pricing_hash BYTEA(32))
- [x] test vectors for cross-router event_id determinism (see below)
- [x] BLAKE3 test vector for tokenizer_version_to_id: `"tiktoken-cl100k_base-v1.2.3"` → `"e3c8e8ff724411c6416dd4fb135368e3"` (16 bytes hex, per RFC-0201)
- [x] multi-tenant collision risk documented with two compliant mitigation paths specified: (1) length-prefixed `compute_event_id` variant, or (2) per-tenant event filtering applied by the caller before passing events to `build_merkle_tree` — see §Security Note — No Field Delimiters; deployers MUST implement one path before production use
- [x] NTP clock synchronization deployed across all router instances (required for cross-node replay determinism)

**Test Vectors for Cross-Router Determinism:**

The following test vectors verify that `compute_event_id()` produces identical output across all router implementations. Failure to match these vectors indicates a UUID format, byte ordering, or encoding bug.

| # | request_id | key_id (UUID) | provider | model | input_tokens | output_tokens | pricing_hash (hex) | token_source | expected event_id (hex) |
|---|------------|---------------|----------|-------|-------------|---------------|--------------------|--------------|------------------------|
| TV1 | `"req-001"` | `"550e8400-e29b-41d4-a716-446655440000"` | `"openai"` | `"gpt-4"` | `100` | `50` | `"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"` | ProviderUsage | `8d22792346a0417bb928da0c16f2af5330640678f365d16bc392d400c2aa4ab2` |
| TV2 | `"req-002"` | `"550e8400-e29b-41d4-a716-446655440000"` | `"openai"` | `"gpt-4"` | `100` | `50` | `"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"` | CanonicalTokenizer | `0f26450e1734034b9bc6f999b61586c671dd8249002524dd740a94c51ded3f36` |
| TV3 | `"req-001"` | `"660e8400-e29b-41d4-a716-446655440001"` | `"openai"` | `"gpt-4"` | `100` | `50` | `"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"` | ProviderUsage | `a3e31fbaa4b3bf6fe9d5c1eeb59055cfe4a3389358fc0e38c8820e2c2e6912ed` |
| TV4 | `"req-001"` | `"550e8400-e29b-41d4-a716-446655440000"` | `"openai"` | `"gpt-4"` | `100` | `50` | `"8b48fe37e84565f99285690a835a881fe2d580ec63775aa5f9465ba38a5a2f60"` | ProviderUsage | `06a6eb1c68f8a75287d0ac45b1ede9f00cd770f106c505685c299cf3b593726c` |

**Test vector computation notes:**
- `key_id.to_string()` uses RFC 4122 hyphenated lowercase format (`"550e8400-e29b-41d4-a716-446655440000"`, 36 chars with hyphens)
- `pricing_hash` is 32 raw bytes encoded as 64 hex chars — decoded to raw bytes before hashing
- `token_source.to_hash_str()` returns `"provider"` for ProviderUsage, `"tokenizer"` for CanonicalTokenizer
- input/output tokens use **little-endian** byte order in the hash
- All routers MUST produce identical 64-char lowercase hex output for the same inputs
- TV4's `pricing_hash` (`"8b48fe37e84565f99285690a835a881fe2d580ec63775aa5f9465ba38a5a2f60"`) is a second test value encoding 32 raw bytes — generated by `SHA256(b"pricing-table-v2")` (UTF-8, no trailing newline). An independent verifier can reproduce this by: `SHA256(b"pricing-table-v2")` → 32 raw bytes → hex encode → use as `pricing_hash`. This tests the code path where pricing_hash differs from TV1-TV3's fixed test value.

## Implementation Notes

### Lookup Table Optimization (Implemented)

The RFC uses `const fn` methods for TokenSource string lookup, enabling compile-time evaluation and zero-cost abstraction:

```rust
pub const fn to_hash_str(&self) -> &'static str { ... }
pub const fn to_db_str(&self) -> &'static str { ... }
```

This avoids heap allocation on every hash computation.

### PricingTable BTreeMap (Implemented)

The `PricingTable` struct uses `BTreeMap<String, PricingModel>` for:

- Deterministic iteration order (RFC-0126 compliance)
- Consistent SHA256 hashing across routers
- Efficient O(log n) lookups

### PricingTable Caching (Optimization)

`PricingTable::new()` creates a new `BTreeMap` and inserts all models on every call. For production deployments, cache the `PricingTable` instance:

```rust
// Singleton pattern for production — zero allocation per request
static PRICING_TABLE: once_cell::sync::Lazy<PricingTable> =
    once_cell::sync::Lazy::new(PricingTable::new);
```

This avoids O(n) allocation per request. Usage in pseudocode:

```rust
let pricing = PRICING_TABLE.get(model).ok_or(KeyError::NotFound)?;
```

### Model Family Lookup Optimization (Optimization)

The naive approach uses repeated `model.starts_with()` calls:

```rust
// O(n) string comparisons — inefficient for many prefixes
if model.starts_with("gpt-") || model.starts_with("o1") || model.starts_with("o3") { ... }
else if model.starts_with("claude-") { ... }
```

A faster approach matches on the first character, then does a single comparison:

```rust
/// Canonical tokenizer version for fallback (RFC-0910)
const CANONICAL_TOKENIZER_VERSION: &str = "tiktoken-cl100k_base-v1.2.3";

/// Get canonical tokenizer for a model family — O(1) per call
/// Returns static str reference — zero allocation
///
/// Note: RFC-0910 is the authoritative source for tokenizer assignments.
/// This function is an approximation for quota accounting pseudocode only.
/// Actual implementation MUST use RFC-0910's tokenizer registry at runtime.
pub fn get_canonical_tokenizer(model: &str) -> &'static str {
    // Prefix-based dispatch: O(1) but coarse — RFC-0910 registry is definitive.
    // Known mappings (approximate, RFC-0910 may refine):
    //   gpt-4*, gpt-3.5*  → cl100k_base
    //   o1, o3             → o200k_base (OpenAI o-series vocab)
    //   o1-mini, o1-preview → different vocab (verify with RFC-0910)
    //   claude-*           → cl100k_base (Anthropic BPE, compatible vocab)
    //   gemini-*           → cl100k_base (Google BPE — NOTE: may be wrong for Gemini,
    //                          which uses SentencePiece, not BPE; fallthrough is intentional)
    //   Other prefixes (m, l, etc.) → fall through to CANONICAL_TOKENIZER_VERSION
    //
    // ⚠️ 'g' prefix collision: models starting with 'g' (gpt-*, gemini-*) both hit the
    // 'g' arm. This is an approximation — the 'g' arm targets GPT (most common 'g' prefix
    // in AI APIs). Gemini tokenizer assignment is uncertain (SentencePiece vs BPE) and
    // requires RFC-0910 clarification. The fallthrough path (CANONICAL_TOKENIZER_VERSION)
    // is NOT known to be correct for any 'g' family beyond GPT.
    //
    // ⚠️ Unknown model families (Mistral 'm', Llama 'l', etc.): fall through to the
    // CANONICAL_TOKENIZER_VERSION constant. This is explicitly NOT verified for any
    // family outside OpenAI/Anthropic. RFC-0910 must provide authoritative mappings.
    match model.chars().next() {
        'g' => "tiktoken-cl100k_base",     // gpt-* family ONLY (gemini-* collision noted)
        'o' => "tiktoken-o200k_base",      // o1/o3 — NOT all o* variants
        'c' => "tiktoken-cl100k_base",     // claude-* family
        _ => CANONICAL_TOKENIZER_VERSION,  // UNKNOWN: requires RFC-0910 registry
    }
}
/// WARNING: This function is pseudocode for quota accounting only.
/// Production code MUST use the RFC-0910 tokenizer registry which maps
/// exact model names to tokenizer versions. The prefix-match above
/// is NOT authoritative — RFC-0910 defines the real mapping.
///
/// ⚠️ CRITICAL: For cross-router determinism, this function's output MUST be
/// bit-for-bit identical to RFC-0903 Final's get_canonical_tokenizer().
/// If the two implementations differ (different tokenizer names, different
/// dispatch logic), the same request_id will produce different token_source
/// values on different routers, breaking event_id determinism. Any change to
/// this function MUST be mirrored in RFC-0903 Final simultaneously.
```

The `&'static str` return type eliminates heap allocation on every call.

### Event ID Hashing (Optimization)

`compute_event_id` uses a stack-allocated `Sha256` hasher. This is already near-optimal:
- `Sha256` hasher is stack-allocated; `key_id.to_string()` makes one `String` heap allocation per call (unavoidable given the hyphenated UUID format the hash commits to)
- `Sha256::new()` is a `const fn` on most digest crates
- Each input component is a single `update()` call

For highest throughput in hot paths, consider batching multiple events through a single hasher context, but this does not affect determinism.

### Numeric Tower Integration (Future Enhancement)

The current implementation uses `u64` micro-units for all cost calculations. The CipherOcto Numeric Tower (DQA, BigInt, Decimal — RFC-0105/0110/0111) offers deterministic numeric types designed for this exact domain.

**Why DQA fits quota accounting (RFC-0105):**

DQA represents numbers as `i64 + explicit_scale` (0–18 decimal places), purpose-built for financial/pricing work. Current micro-unit accounting uses an implicit scale=6 convention:
```
$0.03/1K tokens → 30_000 micro-units (scale=6 assumed)
```

DQA makes the scale explicit in the type system:
```
$0.03/1K tokens → DQA(30_000, scale=6)
```

**Benefits when RFC-0903 adopts DQA for cost_amount:**
- Scale is enforced by the type — no implicit convention errors
- DQA_ADD/MUL/DIV are 10-40x faster than DFP for bounded-range arithmetic (RFC-0105 benchmark)
- Scale tracking prevents mixed-scale arithmetic errors across providers
- Natural domain fit: DQA was designed for pricing/financial work exactly like quota accounting

**Current limitation:** `SpendEvent.cost_amount` is `u64` per RFC-0903 Final. A future RFC-0903 revision could adopt `cost_amount: DQA`, enabling the full Numeric Tower stack for quota arithmetic.

**Note:** `key_id` storage is `BLOB(16)` per RFC-0903-B1. The `uuid::Uuid` struct field converts to/from raw bytes at the storage boundary via `uuid_to_blob_16()` / `blob_16_to_uuid()`.

## Changelog

| Version | Date       | Changes |
| ------- | ---------- | ------- |
| v52     | 2026-04-19 | Round 44 fixes: fix C1 (RFC-0910 version updated from Draft v1 to Draft v2 in Dependencies) |
| v51     | 2026-04-19 | Round 43 fixes: fix 909-C2 (remove stale "defined at line 1469" comment from process_response); RFC-0910 v2 cross-reference updates (910-C1: RFC-0909-B1→RFC-0903-B1 in tokenizers schema; 910-C2: SpendReceipt.token_source→TokenSource; 910-C3: request_id encoding clarification; 910-C4: o200k_base version aligned (unversioned); 910-C5: RFC-0909 v49→v50; 910-C6: RFC-0903 refs include B1/C1 amendments; 910-C7: tokenizer_assignments "(future extension)" removed) |
| v50     | 2026-04-19 | Round 42 fixes: fix C1 (RFC-0910 Pricing Table Registry moved to Draft v1, resolving get_canonical_tokenizer MUST requirement); fix C2 (RFC-0126 version-pinned to Accepted v2.5.1 in Dependencies); fix C3 (RFC-0126 and RFC-0201 version-pinned in Status header) |
| v49     | 2026-04-19 | Round 41 fixes: fix H1 (stale replay_events_for_proof reference → build_merkle_tree at line 618); fix L1 (compute_cost doc comment header added); fix L2 (NTP constraint removed from Known Limitations — cross-referenced to §Constraints) |
| v48     | 2026-04-19 | Comprehensive review fixes: fix C1 (Dependencies RFC-0903-B1 v19→v22); fix H1 (process_response uses tokenizer_version_to_id, not untruncated blake3::hash); fix H2 (Known Limitations section added: upstream cost loss, app bug double-charge, NTP sync, token_source divergence, BLAKE3 truncation); fix M1 (NTP sync now in Approval Criteria); fix M2 (get_canonical_tokenizer call site notes "defined at line 1469"); fix M3 (pricing_hash derivation path documented in comment); fix M4 (test vector computation clarification); fix M5 (Approval Criteria: caller, not build_merkle_tree, does tenant filtering) |
| v47     | 2026-04-18 | Round 35 post-review: fix R36M1 (lower bound cross-reference added to §Quota Consistency Model — now in blockquote, not code block); fix R36M2 (multi-tenant MUST added to Approval Criteria); fix R36L1 (changelog labeled "self-review" to distinguish internal passes from adversarial review rounds) |
| v46     | 2026-04-18 | Round 35 fixes applied: fix R35C1 (v45 row added; footer v45→v46); fix R35M1 (tokenizer_id_to_version doc: "When fully implemented" + "Current stub" note); fix R35M2 (separate storage defined: build_merkle_tree requires tenant-filtered events); fix R35L2 (changelog space removed) |
| v45     | 2026-04-18 | Round 35: fix R34H1 (tokenizer_id_to_version pseudocode removed); fix R34M1 ("not client-controlled" → "specified by the client in the API request"); fix R34M2 (either/or multi-tenant MUST framing added); fix R34M3 (stoolap full SHA1 in normative text, not short hash in changelog); fix R34L1 (inaccurate optional() comment removed) |
| v44     | 2026-04-18 | Round 33: fix R32H1 (security note moved into compute_event_id doc comment, before fn signature); fix R32H2 (security note rewritten to accurately describe multi-tenant threat model); fix R32M3 (tokenizer_id_to_version pseudocode updated to show Result<Option<String>, &'static str>); fix R32M4 (detached /// note removed); fix R32M1 (RFC-0914 v8 changelog entry for v6 now complete); fix R32L1 (RFC-0914 Approval Criteria pinned to RFC-0903-B1 v22) |
| v43     | 2026-04-18 | Round 32: fix R31H1 (KeyError::Storage removed from Error Handling doc); fix R31H2 (add compute_event_id security note documenting no-delimiter construction); fix R31M1 (replay_events_for_proof removal converted to > blockquote); fix R31L2 (§Audit Proof Generation "(Future)" designation clarified); fix R31M3 (RFC-0914 v7 version-pin B1 to v22); update request_id round-trip note to clarify stored data cannot re-derive event_id without original gateway text |
| v42     | 2026-04-18 | Round 31: fix R30C1 (TV4 corrected — pricing_hash=SHA256("pricing-table-v2")=8b48fe37, event_id=06a6eb1c); fix R30C2 (KeyError::Unimplemented → &'static str return type); fix R30C3 (§Event Ordering rewritten to resolve contradiction with §Budget Computation Procedure); fix R30H2 (intro "deterministic replay" → "budget state computation"); fix R30H3 (remove replay_events_for_proof pending spec debt); fix R30M3 (Invariant #4 corrected to reflect UNIQUE scope); update provider_usage_json comment to note "format is provider-dependent" (R30M2) |
| v41     | 2026-04-17 | Round 30: correct R29H2 — stoolap fully enforces UNIQUE on BLOB columns; only INTEGER PRIMARY KEY is restricted; RFC-0903-B1 ref updated to v22; also update RFC-0903-B1 ref in known limitation text |
| v40     | 2026-04-17 | Round 29 fixes: fix R29C1 (footer v38→v39); fix R29C2 (tokenizer_id_to_version signature → Result<Option<String>, KeyError>); fix R29H2 (known limitation updated to explicitly acknowledge stoolap UNIQUE/BLOB enforcement gap); fix R29H3 (replay_events_for_proof renamed as pending future API); fix R29M1 (correct retry behavior no longer called a limitation); fix R29M3 (ORDER BY note removed false 'cursor-based pagination' justification); fix R29M4 (Merkle root reproducibility from BLOB storage documented); add BLAKE3 test vector to Approval Criteria (R29H4); add TV4 pricing_hash origin (R29H1); update RFC-0903-B1 ref to v20 |
| v39     | 2026-04-17 | Round 28 fixes: replace malformed test vectors (52-char pricing_hash → correct 64-char hex); fix H4 known limitation (UNIQUE constraint prevents described double-insertion); update RFC-0903-B1 ref to v19; fix RL1 changelog error (BLAKE3 truncation code already in v18); update RFC-0914 ref to v5 |
| v38     | 2026-04-17 | Round 27 fixes: rename §Deterministic Replay Procedure to §Budget Computation Procedure (clarifies scope, not Merkle path); replace tokenizer_id_to_version None-return with unreachable!() (silent wrong answer worse than panic); fix Invariant #3 truncation bound (<2 micro-units per event, not <1); fix request_id description (remove stale "16-256 bytes"); add BLAKE3 test vector for tokenizer_version_to_id; add missing ignore tags to ensure_tokenizer pseudocode; add NL1 clarification (replay_events_for_proof not input to build_merkle_tree); update RFC-0903-B1 ref to v19 |
| v37     | 2026-04-16 | Round 26 fixes: fix BLAKE3 truncation code (Hasher::new+finalize, Hash→[u8;32]→slice); fix tokenizer_id_to_version stub (remove unimplemented!, add DB lookup pseudocode); add computed test vectors for TV1/TV2/TV3 (mechanically verifiable); align footer version to v36; update RFC-0903-B1 reference to v18; clarify two ordering paths (canonical external vs internal DB); add cost_amount tradeoff note; add token_source divergence known limitation; strengthen serde_json warning; raise MAX_REQUEST_ID_LEN to 1024 |
| v36     | 2026-04-15 | Round 25 fixes (continued): update Invariant #3 to note truncation (true_cost - N ≤ current_spend); add upstream cost loss documentation in Failure Handling; designate in-memory event_id-only ordering as canonical Merkle root for external verification; add NTP clock sync constraint for multi-node deployments |
| v35     | 2026-04-15 | Round 25 fixes: mandate UUID format (RFC 4122 hyphenated lowercase) in compute_event_id; change "should" to "MUST" for canonical JSON in compute_pricing_hash; document ordering difference between replay_events (event_id only) and DB replay (created_at+event_id); explain execute-first pattern in process_request_with_accounting; fix tokenizer_id_to_version to unimplemented!(); add round-trip type semantics note to request_id; add cross-router test vectors; note 'g' prefix collision for gemini and unknown family limitations |
| v34     | 2026-04-15 | Round 23 fixes: align RFC-0903-B1 version to v15 and RFC-0903-C1 to v1 throughout (was referencing wrong C1 version); update Approval Criteria to reflect RFC-0903-C1 team_id BLOB(16); add RFC-0903-B1/B1/C1 version references to Dependencies |
| v33     | 2026-04-15 | Round 22 fixes: normalize tokenizer_version to tokenizer_id FK (BLOB(16) via BLAKE3); add tokenizers table to RFC-0903-B1 schema; update SpendEvent to use Option<[u8;16]> tokenizer_id; update RFC-0903-B1 v15 cross-refs |
| v28     | 2026-04-15 | Round 17 fixes: fix get_canonical_tokenizer(model)? compile error (remove ?, add .to_string()); fix pricing_hash schema comment (unchanged from RFC-0903 Final, not changed by RFC-0903-B1); add saturating_add rationale to replay_events; add DB-based router BLOB→hex Merkle note; add RFC-0903-B1 cross-refs for encode_request_id; add pseudocode caveat to process_request_with_accounting; align with RFC-0903-B1 v12 |
| v27     | 2026-04-15 | Round 16 fixes: add replay_events_for_proof() for Merkle proof path, fix stale "(BYTEA storage)" header comment, add cross-RFC get_canonical_tokenizer determinism warning, align with RFC-0903-B1 v11 |
| v26     | 2026-04-15 | Round 15 fixes: remove non-substantive file-existence approval criterion, align with RFC-0903-B1 v10 |
| v25     | 2026-04-15 | Round 14 fixes: update request_id entry in RFC-0903-B1 table to note SHA256 encoding, align with RFC-0903-B1 v9 |
| v24     | 2026-04-15 | Round 13 fixes: fix Merkle tree to build navigable structure (children now populated), mark idx_spend_ledger_key_time as pre-existing legacy index, add RFC-0201 to Related RFCs footer, align with RFC-0903-B1 v8 |
| v23     | 2026-04-15 | Round 12 fixes: fix Merkle tree panic on empty events (returns Option), add event_id vs request_id duality section, align with RFC-0903-B1 v7 |
| v22     | 2026-04-15 | Round 11 fixes: fix stale TEXT→BLOB comment in compute_event_id, fix hex_to_blob_32/blob_32_to_hex comments (only for event_id, not request_id) |
| v21     | 2026-04-15 | Round 10 fixes: fix created_at comment (no DEFAULT added), clarify event_id vs request_id encoding distinction, update event_id example comment |
| v20     | 2026-04-15 | Round 9 fixes: fix request_id comment (hex→raw is wrong; gateway raw text + SHA256), fix approval criteria (BLOB not BYTEA), fix schema comment consistency (dashes + phrasing) |
| v19     | 2026-04-14 | Round 8 fixes: fix created_at DEFAULT UNIXEPOCH() claim, fix compute_event_id TEXT storage comment, fix Merkle tree BLOB vs TEXT comment, fix pricing_hash BLOB→BYTEA(32), fix event_id BLOB vs TEXT comment, fix created_at ordering (struct lacks created_at), add validate_request_id call in process_response, fix step numbering |
| v18     | 2026-04-14 | Round 7 fixes: add hex_to_blob/blob_32_to_hex/uuid_to_blob_16/blob_16_to_uuid helpers, fix event_id comment (BLOB not TEXT), add storage encoding comment to SpendEvent, fix stale key_id TEXT note, add request_id encoding rules to RFC-0903-B1, remove stale created_at DEFAULT entry, remove stale PRIMARY KEY comment, fix get_canonical_tokenizer (add RFC-0910 disclaimer) |
| v17     | 2026-04-14 | Round 6 fixes: fix process_response return type to match record_spend (returns Ok(())), add RFC-0903-B1 amendment section, update schema to BYTEA storage (event_id BLOB(32), request_id BLOB(32), key_id BLOB(16)), mark RFC-0903-B1 ext indexes, add compute_cost truncation note, fix o1/o3 tokenizer (o200k_base), update Merkle tree for BLOB storage, add RFC-0903-B1 approval criteria |
| v16     | 2026-04-14 | Add Numeric Tower (DQA) integration note for future cost_amount enhancement; note key_id TEXT storage per RFC-0903 |
| v15     | 2026-04-14 | Round 5 fixes: replace record_spend_ledger prose refs with record_spend/record_spend_with_team, add ASC to §Ledger-Based replay SQL, add CANONICAL_TOKENIZER_VERSION const, fix Merkle tree odd-leaf comment, add request_id length bound, RFC-8785 crate reference |
| v14     | 2026-04-14 | Round 4 fixes: rename calculate_cost→compute_cost, clarify process_response as pseudocode calling record_spend, fix lock ordering scope, fix replay_events comment, add model lookup O(1) optimization, update Implementation Notes |
| v13     | 2026-04-14 | Round 3 fixes: use KeyError, call record_spend_ledger, fix Error types, add PricingTable caching note, add key_created index, fix TEXT comment, fix Merkle tree comment, clarify TokenSource methods |
| v12     | 2026-04-14 | Round 2 adversarial review fixes: fix event ordering conflicts, remove invalid CHECK constraint, fix schema PRIMARY KEY for stoolap compatibility, fix ON CONFLICT to MySQL-style idempotency, add created_at to INSERT, fix four-backtick code fences |
| v11     | 2026-04-14 | Adversarial review fixes: remove duplicate token_source_lookup module, fix event ordering (created_at, event_id), pricing_hash→[u8;32], add FOR UPDATE locks, add token_source CHECK constraint, fix pricing_hash BLOB not TEXT, canonical JSON note, mark process_response as impl detail, fix replay_events |
| v10     | 2026-04-14 | Full alignment with RFC-0903 Final v29: event_id→String, request_id→String, timestamp ordering, TokenSource lookup tables, lock ordering, BTreeMap pricing |
| v9      | 2026-03-27 | Adopt RFC-0903 `spend_ledger` schema; remove parallel `usage_ledger` table; rename columns |
| v1      | 2026-03-25 | Initial draft |

---

**Draft Date:** 2026-04-19
**Version:** v52
**Related Use Case:** Enhanced Quota Router Gateway
**Related RFCs:** RFC-0903 (Virtual API Key System), RFC-0903-B1 (Schema Amendments), RFC-0903-C1 (Extended Schema Amendments), RFC-0126 (Deterministic Serialization v2.5.1), RFC-0201 (Binary BLOB Type v5.24), RFC-0910 (Pricing Table Registry v2)
