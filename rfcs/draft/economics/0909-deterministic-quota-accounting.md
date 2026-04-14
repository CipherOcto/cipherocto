# RFC-0909 (Economics): Deterministic Quota Accounting

## Status

Draft (v10 — aligned with RFC-0903 Final v29, RFC-0126)

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
/// Uses &'static str lookup table to avoid allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenSource {
    /// Token counts from provider response usage metadata
    ProviderUsage,
    /// Token counts from canonical tokenizer fallback
    CanonicalTokenizer,
}

/// Static lookup tables for TokenSource strings (avoids allocation)
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
    /// Stored as TEXT in database for compatibility
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
    /// Pricing hash (32 bytes stored as Vec<u8>, TEXT in DB as hex)
    pub pricing_hash: Vec<u8>,
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

Ordering rule (aligned with RFC-0903 Final):

```

timestamp ASC, event_id ASC

```

**CRITICAL:** For deterministic replay, ordering by `created_at` (database insert time) is NOT used in the query path. Instead, use `timestamp` (event time from provider) for chronological ordering, with `event_id` as tiebreaker. See §Deterministic Replay Procedure.

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

**Database constraint for safety:**

```sql
-- Add CHECK constraint to prevent any over-budget state
ALTER TABLE api_keys
ADD CONSTRAINT chk_budget_not_exceeded
CHECK (current_spend <= budget_limit);
```

**Canonical approach:** Use `record_spend()` from the Ledger-Based Architecture section below. This function uses `FOR UPDATE` row locking and derives spend from the ledger, providing deterministic accounting.

**Single-writer principle:**

For deterministic accounting across multiple routers:

```
Router → Primary DB (strong consistency) → Usage Event Recorded
```

## Lock Ordering Invariant

**CRITICAL for multi-key transactions:**

ALL transactions that lock both `teams` and `api_keys` rows MUST acquire the team lock BEFORE the key lock to prevent deadlocks:

```
1. SELECT ... FROM teams WHERE ... FOR UPDATE
2. SELECT ... FROM api_keys WHERE ... FOR UPDATE
```

This order must be followed consistently across ALL code paths. Any code that violates this order risks deadlock under concurrent load.

See RFC-0903 Final §Lock Ordering Invariant for full specification.

## Idempotent Event Recording

To support retries, event recording must be idempotent.

Each request receives a **deterministic request_id**.

```rust
/// Compute deterministic request_id
/// The request_id is provided by the API gateway, not generated here.
/// It serves as the idempotency key for deduplication.
pub fn validate_request_id(request_id: &str) -> Result<(), KeyError> {
    if request_id.is_empty() {
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
    event_id TEXT PRIMARY KEY,              -- SHA256 hex (36+ chars)
    request_id TEXT NOT NULL,                -- Idempotency key
    key_id TEXT NOT NULL,                    -- UUID as text
    team_id TEXT,                           -- Optional team attribution
    provider TEXT NOT NULL,                  -- Provider name
    model TEXT NOT NULL,                     -- Model name
    input_tokens INTEGER NOT NULL,           -- Prompt tokens
    output_tokens INTEGER NOT NULL,           -- Completion tokens
    cost_amount BIGINT NOT NULL,             -- Cost in smallest unit (u64)
    pricing_hash TEXT NOT NULL,              -- SHA256 hex (64 chars)
    timestamp INTEGER NOT NULL,               -- Unix epoch (authoritative event time)
    token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
    tokenizer_version TEXT,
    provider_usage_json TEXT,                -- Raw provider usage for audit
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    -- Scoped uniqueness: request_id unique per key (idempotency constraint)
    UNIQUE(key_id, request_id),
    -- Foreign keys for integrity
    FOREIGN KEY(key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE,
    FOREIGN KEY(team_id) REFERENCES teams(team_id) ON DELETE SET NULL
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
-- Composite index for efficient quota queries
CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);
-- Index for pricing verification queries
CREATE INDEX idx_spend_ledger_pricing_hash ON spend_ledger(pricing_hash);
```

## Replay and Verification

Quota state must be reproducible via replay.

```rust
/// Reconstruct quota state from events
/// Uses BTreeMap for deterministic iteration ordering
pub fn replay_events(events: &[SpendEvent]) -> std::collections::BTreeMap<String, u64> {
    use std::collections::BTreeMap;

    let mut key_spend: BTreeMap<String, u64> = BTreeMap::new();

    // Events must be sorted by timestamp (chronological), then event_id for determinism
    // This matches the ORDER BY timestamp ASC, event_id ASC rule
    let mut sorted_events = events.to_vec();
    sorted_events.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.event_id.cmp(&b.event_id))
    });

    for event in sorted_events {
        let entry = key_spend.entry(event.key_id.to_string()).or_insert(0);
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
2. Order by timestamp ASC, then event_id ASC (canonical identity)
3. Compute current_spend = SUM(events.cost_amount)
4. Verify equality: computed_spend == stored current_spend
5. If mismatch, trust spend_ledger as authoritative
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
    pub fn compute_pricing_hash(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};

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

/// Calculate cost deterministically using integer arithmetic
pub fn calculate_cost(
    pricing: &PricingModel,
    input_tokens: u32,
    output_tokens: u32,
) -> u64 {
    // Integer math only - no floating point
    let prompt_cost = (input_tokens as u64 * pricing.prompt_cost_per_1k) / 1000;
    let completion_cost = (output_tokens as u64 * pricing.completion_cost_per_1k) / 1000;

    prompt_cost.saturating_add(completion_cost)
}

/// Fast lookup table for token source strings (avoids allocation)
pub mod token_source_lookup {
    use super::TokenSource;

    /// Static string table for token_source.to_hash_str()
    pub const fn to_hash_str(source: TokenSource) -> &'static str {
        match source {
            TokenSource::ProviderUsage => "provider",
            TokenSource::CanonicalTokenizer => "canonical_tokenizer",
        }
    }

    /// Static string table for token_source.to_db_str()
    pub const fn to_db_str(source: TokenSource) -> &'static str {
        match source {
            TokenSource::ProviderUsage => "provider_usage",
            TokenSource::CanonicalTokenizer => "canonical_tokenizer",
        }
    }
}
```

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

```rust
/// Process response and record usage
/// CRITICAL: Uses provider-reported tokens and deterministic event_id for cross-router determinism
/// Note: ProviderResponse.provider_usage_json contains the raw provider usage JSON for audit
pub async fn process_response(
    db: &Database,
    key_id: &uuid::Uuid,
    team_id: Option<&str>,
    provider: &str,
    model: &str,
    response: &ProviderResponse,  // Contains: usage, timestamp, id, provider_usage_json
    pricing_hash: [u8; 32],
) -> Result<SpendEvent, Error> {
    // CRITICAL: Use provider-reported tokens for deterministic accounting
    // This ensures all routers produce identical token counts
    let input_tokens = response.input_tokens;
    let output_tokens = response.output_tokens;

    // Determine token source: check if provider returned usage metadata
    // A provider may legitimately return 0 tokens, so check .is_some() not token count
    let (token_source, tokenizer_version) = if response.usage.is_some() {
        (TokenSource::ProviderUsage, None)
    } else {
        // Provider didn't return usage - must use canonical tokenizer
        (TokenSource::CanonicalTokenizer, Some(get_canonical_tokenizer(model)))
    };

    // Look up pricing and calculate cost using deterministic integer math
    let pricing = PricingTable::new()
        .get(model)
        .ok_or_else(|| Error::UnknownModel(model.to_string()))?;
    let cost_amount = calculate_cost(pricing, input_tokens, output_tokens);

    // Generate deterministic event_id using SHA256 hex (matches RFC-0903 Final)
    let event_id = compute_event_id(
        &response.request_id,
        key_id,
        provider,
        model,
        &pricing_hash,
        token_source,
    );

    // Create spend event with token source for deterministic replay
    let event = SpendEvent {
        event_id,
        request_id: response.request_id.clone(),
        key_id: *key_id,
        team_id: team_id.map(String::from),
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens,
        output_tokens,
        cost_amount,
        pricing_hash: pricing_hash.to_vec(),
        token_source,
        tokenizer_version,
        provider_usage_json: response.provider_usage_json.clone(),
        timestamp: response.timestamp,
    };

    // Wrap in transaction for atomicity - prevents orphan ledger entries
    let tx = db.transaction()?;

    // 1. Lock key row and check budget
    let budget: i64 = tx.query_row(
        "SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE",
        params![key_id.to_string()],
        |row| row.get(0),
    )?;

    // 2. Compute current spend from ledger
    let current: i64 = tx.query_row(
        "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
        params![key_id.to_string()],
        |row| row.get(0),
    )?;

    // 3. Check budget
    if current + cost_amount as i64 > budget {
        return Err(Error::BudgetExceeded { current: current as u64, limit: budget as u64 });
    }

    // 4. Insert into ledger
    tx.execute(
        "INSERT INTO spend_ledger (
            event_id, request_id, key_id, team_id, timestamp,
            provider, model, input_tokens, output_tokens, cost_amount,
            pricing_hash, token_source, tokenizer_version, provider_usage_json
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT(key_id, request_id) DO NOTHING",
        params![
            &event.event_id,
            &event.request_id,
            event.key_id.to_string(),
            event.team_id,
            event.timestamp as i64,
            &event.provider,
            &event.model,
            event.input_tokens as i32,
            event.output_tokens as i32,
            event.cost_amount as i64,
            &hex::encode(&event.pricing_hash),  // Store as hex TEXT
            token_source.to_db_str(),
            event.tokenizer_version,
            &event.provider_usage_json,
        ],
    )?;

    tx.commit()?;
    Ok(event)
}

/// Get canonical tokenizer for a model family
fn get_canonical_tokenizer(model: &str) -> String {
    // Different model families need different tokenizers
    if model.starts_with("gpt-") || model.starts_with("o1") || model.starts_with("o3") {
        "tiktoken-cl100k_base".to_string()
    } else if model.starts_with("claude-") {
        "tiktoken-cl100k_base".to_string() // Anthropic uses BPE
    } else if model.starts_with("gemini-") {
        "tiktoken-cl100k_base".to_string() // Google uses BPE
    } else {
        // Default fallback
        CANONICAL_TOKENIZER_VERSION.to_string()
    }
}

const CANONICAL_TOKENIZER_VERSION: &str = "tiktoken-cl100k_base-v1.2.3";

This guarantees:

```

deterministic billing

````

**Failure handling note:** The provider request is an external HTTP call outside the database transaction. If the provider succeeds but `record_usage` fails, the response has already been consumed. The compensating approach is to use idempotent `request_id` for retries — if a retry arrives with the same `request_id`, the `ON CONFLICT` will silently succeed, preventing double-billing.

## Overflow Safety

All accounting variables must use:

```rust
u64
````

Maximum supported spend:

```
18,446,744,073,709,551,615 CU
```

Overflow must be treated as a fatal error.

```rust
fn checked_add_spend(current: u64, add: u64) -> Result<u64, Error> {
    current
        .checked_add(add)
        .ok_or_else(|| Error::OverflowDetected)
}
```

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
pub fn build_merkle_tree(events: &[SpendEvent]) -> MerkleNode {
    // Sort events deterministically by event_id (binary comparison)
    let mut sorted = events.to_vec();
    sorted.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    // Build leaf nodes from hex event_id (converted to bytes for hashing)
    let mut leaves: Vec<[u8; 32]> = sorted
        .iter()
        .map(|e| {
            let mut hasher = Sha256::new();
            hasher.update(e.event_id.as_bytes());  // Hex string → bytes
            hasher.update(e.cost_amount.to_le_bytes());
            hasher.finalize().into()
        })
        .collect();

    // Build tree bottom-up
    while leaves.len() > 1 {
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
) -> Result<Response, Error> {
    // Start transaction
    let tx = db.transaction()?;

    // Execute request to provider
    let response = execute_request(request).await?;

    // Record usage and deduct budget ATOMICALLY
    let event = record_usage(&tx, &request.key_id, &response)?;

    // Commit transaction (includes accounting)
    tx.commit()?;

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

**Deterministic replay:**

```
1. SELECT * FROM spend_ledger ORDER BY timestamp, event_id
2. Recompute balances
3. Verify equality with any cached balances
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

The RFC uses `const fn` methods for TokenSource string lookup, which enables compile-time evaluation and zero-cost abstraction:

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

## Changelog

| Version | Date       | Changes                                                                                                                                                    |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v10     | 2026-04-14 | Full alignment with RFC-0903 Final v29: event_id→String, request_id→String, timestamp ordering, TokenSource lookup tables, lock ordering, BTreeMap pricing |
| v9      | 2026-03-27 | Adopt RFC-0903 `spend_ledger` schema; remove parallel `usage_ledger` table; rename columns                                                                 |
| v1      | 2026-03-25 | Initial draft                                                                                                                                              |

---

**Draft Date:** 2026-03-25
**Version:** v10
**Related Use Case:** Enhanced Quota Router Gateway
**Related RFCs:** RFC-0903 (Virtual API Key System), RFC-0126 (Deterministic Serialization)
