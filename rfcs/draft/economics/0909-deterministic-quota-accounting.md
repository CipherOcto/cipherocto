# RFC-0909 (Economics): Deterministic Quota Accounting

## Status

Draft (v16 — aligned with RFC-0903 Final v29, RFC-0126)

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

- RFC-0903: Virtual API Key System (Final)
- RFC-0126: Deterministic Serialization (for canonical JSON serialization)

**Optional:**

- RFC-0900: AI Quota Marketplace Protocol
- RFC-0901: Quota Router Agent Specification
- RFC-0910: Pricing Table Registry (for immutable pricing tables)

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
// Simple cost: just tokens
let cost = input_tokens + output_tokens;

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
/// Aligns with RFC-0903 Final §SpendEvent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendEvent {
    /// Deterministic event identifier (SHA256 hex string - same as RFC-0903 Final)
    /// event_id is stored as TEXT (hex-encoded) in the database
    pub event_id: String,
    /// Request identifier for idempotency (UNIQUE constraint)
    pub request_id: String,
    /// API key that made the request
    pub key_id: uuid::Uuid,
    /// Team ID (if applicable)
    pub team_id: Option<String>,
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
    /// Canonical tokenizer version (if token_source is CanonicalTokenizer)
    pub tokenizer_version: Option<String>,
    /// Raw provider usage JSON for audit
    pub provider_usage_json: Option<String>,
    /// Event timestamp (epoch seconds - from provider response, NOT insert time)
    pub timestamp: i64,
}

/// Generate deterministic event_id from request content
/// Aligns with RFC-0903 Final §compute_event_id
/// Returns hex-encoded SHA256 string for storage as TEXT
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
    // Return hex string (TEXT storage compatible with RFC-0903 Final)
    format!("{:x}", hasher.finalize())
}

Events represent the **canonical accounting record**.

Quota state must be derivable from the ordered sequence of events.

## Event Ordering

Events must be processed in deterministic order.

Ordering rule (per RFC-0903 Final §Deterministic Replay Procedure):

```

created_at ASC, event_id ASC

```

- `created_at` is the database insert timestamp — provides chronological ordering
- `event_id` serves as tiebreaker for deterministic replay
- `timestamp` (provider event time) is metadata only and does NOT participate in event ordering

This matches RFC-0903 Final which uses `created_at` (chronological) not `timestamp` (event time) for deterministic replay ordering.

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

```rust
/// Validate request_id format and bounds
/// The request_id is provided by the API gateway, not generated here.
/// It serves as the idempotency key for deduplication.
pub fn validate_request_id(request_id: &str) -> Result<(), KeyError> {
    if request_id.is_empty() {
        return Err(KeyError::InvalidFormat);
    }
    // Reject unreasonably long request_ids to prevent storage abuse.
    // Typical provider request_ids are 16–64 bytes.
    const MAX_REQUEST_ID_LEN: usize = 256;
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

```sql
-- Spend ledger - THE authoritative economic record
-- Aligns exactly with RFC-0903 Final §spend_ledger schema
-- Token counts MUST originate from provider when available (see Canonical Token Accounting)
CREATE TABLE spend_ledger (
    event_id TEXT NOT NULL,                 -- SHA256 hex (deterministic identity)
    request_id TEXT NOT NULL,                -- Idempotency key
    key_id TEXT NOT NULL,                    -- UUID as text
    team_id TEXT,                           -- Optional team attribution
    provider TEXT NOT NULL,                  -- Provider name
    model TEXT NOT NULL,                     -- Model name
    input_tokens INTEGER NOT NULL,           -- Prompt tokens
    output_tokens INTEGER NOT NULL,           -- Completion tokens
    cost_amount BIGINT NOT NULL,             -- Cost in smallest unit (u64)
    pricing_hash BLOB NOT NULL,              -- SHA256 binary (32 bytes) — matches RFC-0903 Final BYTEA(32)
    timestamp INTEGER NOT NULL,               -- Unix epoch (authoritative event time)
    token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
    tokenizer_version TEXT,
    provider_usage_json TEXT,                -- Raw provider usage for audit
    created_at INTEGER NOT NULL,             -- Insert timestamp (seconds) — provided by application
    -- Idempotency: UNIQUE constraint prevents duplicate request_id per key
    -- Note: event_id is deterministic (SHA256) but NOT PRIMARY KEY due to stoolap
    -- limitation (only INTEGER PRIMARY KEY supported). Index on event_id for lookup.
    UNIQUE(key_id, request_id),
    -- Foreign keys for integrity
    FOREIGN KEY(key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE,
    FOREIGN KEY(team_id) REFERENCES teams(team_id) ON DELETE SET NULL
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
CREATE INDEX idx_spend_ledger_event_id ON spend_ledger(event_id);
-- Composite index for efficient quota queries
CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);
-- Composite index for efficient replay with ORDER BY created_at
CREATE INDEX idx_spend_ledger_key_created ON spend_ledger(key_id, created_at);
-- Index for pricing verification queries
CREATE INDEX idx_spend_ledger_pricing_hash ON spend_ledger(pricing_hash);
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
        *entry = entry.saturating_add(event.cost_amount);
    }

    key_spend
}
```

Verification nodes can reconstruct:

- total spend
- quota exhaustion
- billing totals

**Deterministic Replay Procedure:**

For audit and verification, deterministic replay MUST follow this procedure:

```
1. Load all spend_ledger for a key_id
2. Order by created_at ASC, event_id ASC (chronological + tiebreaker)
3. Compute current_spend = SUM(events.cost_amount)
4. Verify against ledger-derived balance (not stored counter)
```

This ensures economic audit can always reconcile the ledger.

### Economic Invariants

The following invariants MUST hold at all times:

```
1. spend_ledger are the authoritative economic record
2. current_spend = SUM(spend_ledger.cost_amount)
3. 0 ≤ current_spend ≤ budget_limit
4. request_id uniqueness prevents double charging
5. pricing_hash ensures deterministic cost calculation
6. token_source MUST be identical across routers for a given request_id
```

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
    /// should be used in production to ensure cross-router hash consistency.
    pub fn compute_pricing_hash(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};

        // BTreeMap provides sorted key ordering, but field order within structs
        // is not guaranteed by serde_json. For production, use RFC-8785 canonical JSON.
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

/// Compute cost deterministically using integer arithmetic
/// Name aligns with RFC-0903 Final §compute_cost
pub fn compute_cost(
    pricing: &PricingModel,
    input_tokens: u32,
    output_tokens: u32,
) -> u64 {
    // Integer math only - no floating point
    let prompt_cost = (input_tokens as u64 * pricing.prompt_cost_per_1k) / 1000;
    let completion_cost = (output_tokens as u64 * pricing.completion_cost_per_1k) / 1000;

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

**Replay safety invariant:**

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
/// # Error Handling
/// - `KeyError::BudgetExceeded` → return 429 to client, do NOT return provider response
/// - `KeyError::Storage` → return 500 to client, do NOT return provider response
/// - Duplicate request_id → silently idempotent (safe to retry)
pub async fn process_response(
    db: &Database,
    key_id: &uuid::Uuid,
    team_id: Option<&str>,
    provider: &str,
    model: &str,
    response: &ProviderResponse,
    pricing_hash: [u8; 32],
) -> Result<SpendEvent, KeyError> {
    // 1. Determine token source (provider usage vs canonical tokenizer)
    let (token_source, tokenizer_version) = match response.usage.is_some() {
        true => (TokenSource::ProviderUsage, None),
        false => (TokenSource::CanonicalTokenizer, Some(get_canonical_tokenizer(model)?)),
    };

    // 2. Look up pricing (should be cached singleton in production — see §PricingTable Caching)
    let pricing = PRICING_TABLE.get(model).ok_or(KeyError::NotFound)?;
    let cost_amount = compute_cost(pricing, response.input_tokens, response.output_tokens);

    // 3. Generate deterministic event_id (matches RFC-0903 Final §compute_event_id)
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

    // 4. Build SpendEvent (matches RFC-0903 Final §SpendEvent)
    let event = SpendEvent {
        event_id,
        request_id: response.request_id.clone(),
        key_id: *key_id,
        team_id: team_id.map(String::from),
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
        cost_amount,
        pricing_hash,
        token_source,
        tokenizer_version,
        provider_usage_json: response.provider_usage_json.clone(),
        timestamp: response.timestamp,
    };

    // 5. Record spend via RFC-0903 Final ledger-based function
    //    - record_spend(db, key_id, &event) for key-level budget
    //    - record_spend_with_team(db, key_id, team_id, &event) for team-level budget
    match team_id {
        Some(tid) => record_spend_with_team(db, key_id, tid, &event)?,
        None => record_spend(db, key_id, &event)?,
    };

    Ok(event)
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

## Audit Proof Generation (Future)

The event ledger can be extended to generate **cryptographic proofs**.

```rust
use sha2::{Digest, Sha256};

/// Merkle tree node
#[derive(Debug, Clone)]
pub struct MerkleNode {
    pub hash: [u8; 32],
    pub left: Option<Box<MerkleNode>>,
    pub right: Option<Box<MerkleNode>>,
}

/// Build Merkle tree from usage events
///
/// Note: This hashes the ASCII bytes of the hex-encoded event_id string (not the raw
/// binary SHA256). This is deterministic but produces a different root than hashing
/// the original binary. This approach is used because event_id is stored as hex
/// string and the hex representation is what appears in audit logs.
pub fn build_merkle_tree(events: &[SpendEvent]) -> MerkleNode {
    // Sort events deterministically by event_id (lexicographic comparison of hex string)
    let mut sorted = events.to_vec();
    sorted.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    // Build leaf nodes from hex event_id (hashed as ASCII bytes of hex string)
    let mut leaves: Vec<[u8; 32]> = sorted
        .iter()
        .map(|e| {
            let mut hasher = Sha256::new();
            hasher.update(e.event_id.as_bytes());  // ASCII bytes of hex string
            hasher.update(e.cost_amount.to_le_bytes());
            hasher.finalize().into()
        })
        .collect();

    // Build tree bottom-up
    while leaves.len() > 1 {
        // Duplicate the last leaf if odd count (keeps tree balanced and deterministic)
        if leaves.len() % 2 == 1 {
            leaves.push(leaves.last().unwrap().clone());
        }

        let mut parents = Vec::new();
        for pair in leaves.chunks(2) {
            let mut hasher = Sha256::new();
            hasher.update(&pair[0]);
            hasher.update(&pair[1]);
            let result = hasher.finalize();
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&result);
            parents.push(hash);
        }
        leaves = parents;
    }

    MerkleNode {
        hash: leaves[0],
        left: None,
        right: None,
    }
}
```

Root hashes can be published periodically.

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

```rust
/// Process request with accounting as part of transaction
pub async fn process_request_with_accounting(
    db: &Database,
    request: &Request,
    pricing_hash: [u8; 32],
) -> Result<Response, KeyError> {
    // Execute request to provider
    let response = execute_request(request).await?;

    // Record spend via ledger (atomic budget enforcement)
    // Note: process_response handles its own transaction internally
    let _event = process_response(
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
spend_ledger table schema (Final v29)
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

## Approval Criteria

This RFC can be approved when:

- [x] deterministic cost units are implemented
- [x] spend_ledger is append-only (per RFC-0903)
- [x] atomic quota deduction is implemented
- [x] idempotent request accounting exists
- [x] types align with RFC-0903 Final v29
- [x] lock ordering invariant is documented
- [x] TokenSource uses lookup tables (no allocation)
- [x] TokenSource hash strings match RFC-0903 Final (`"provider"`/`"tokenizer"`)

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
// Singleton pattern for production
static PRICING_TABLE: once_cell::sync::Lazy<PricingTable> =
    once_cell::sync::Lazy::new(PricingTable::new);
```

This avoids O(n) allocation per request.

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
/// Note: RFC-0910 implementation concern; defined here for completeness.
pub fn get_canonical_tokenizer(model: &str) -> &'static str {
    match model.chars().next() {
        // GPT models: gpt-*, o1-*, o3-* — all use cl100k_base
        'g' | 'o' => "tiktoken-cl100k_base",
        // claude-* models — Anthropic uses BPE
        'c' => "tiktoken-cl100k_base",
        // Default fallback — all currently use the same tokenizer
        _ => CANONICAL_TOKENIZER_VERSION,
    }
}
```

The `&'static str` return type eliminates heap allocation on every call.

### Event ID Hashing (Optimization)

`compute_event_id` uses a stack-allocated `Sha256` hasher. This is already near-optimal:
- No heap allocation per call
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

**Note:** The `key_id` field is `TEXT NOT NULL` per RFC-0903 Final schema. Storing as binary `[u8; 16]` would reduce storage but requires a breaking schema change to RFC-0903.

## Changelog

| Version | Date       | Changes |
| ------- | ---------- | ------- |
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

**Draft Date:** 2026-04-14
**Version:** v16
**Related Use Case:** Enhanced Quota Router Gateway
**Related RFCs:** RFC-0903 (Virtual API Key System), RFC-0126 (Deterministic Serialization)
