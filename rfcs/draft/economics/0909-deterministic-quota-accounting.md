# RFC-0909 (Economics): Deterministic Quota Accounting

## Status

Draft (v9 - adopts RFC-0903 spend_ledger, removes parallel ledger schema)

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

- RFC-0903: Virtual API Key System

**Optional:**

- RFC-0900: AI Quota Marketplace Protocol
- RFC-0901: Quota Router Agent Specification

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

Each request generates a **Usage Event**.

```rust
use serde::{Deserialize, Serialize};

/// Token source for deterministic accounting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenSource {
    /// Token counts from provider response usage metadata
    ProviderUsage,
    /// Token counts from canonical tokenizer fallback
    CanonicalTokenizer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageEvent {
    /// Deterministic event identifier (SHA256 hash - see compute_event_id)
    /// Stored as binary [u8; 32] for crypto-style storage efficiency
    pub event_id: [u8; 32],
    /// Deterministic request ID (SHA256 of key_id + timestamp + nonce)
    /// Stored as binary [u8; 32] for crypto-style storage efficiency
    pub request_id: [u8; 32],
    /// API key that made the request
    pub key_id: Uuid,
    /// Team ID (if applicable)
    pub team_id: Option<String>,
    /// Unix timestamp (seconds)
    pub timestamp: u64,
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
    /// Pricing hash (SHA256 of pricing table used)
    pub pricing_hash: [u8; 32],
    /// Token source for deterministic accounting (CRITICAL for cross-router determinism)
    pub token_source: TokenSource,
    /// Canonical tokenizer version (if token_source is CanonicalTokenizer)
    pub tokenizer_version: Option<String>,
    /// Raw provider usage JSON for audit
    pub provider_usage_json: Option<String>,
}

/// Generate deterministic event_id from request content
/// Returns binary SHA256 hash for efficient storage
/// This ensures identical event_id across all routers for the same request
fn compute_event_id(
    request_id: &[u8; 32],
    key_id: &Uuid,
    provider: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    pricing_hash: &[u8; 32],
    token_source: TokenSource,
) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(request_id);
    hasher.update(key_id.to_string().as_bytes());
    hasher.update(provider.as_bytes());
    hasher.update(model.as_bytes());
    hasher.update(input_tokens.to_le_bytes());
    hasher.update(output_tokens.to_le_bytes());
    hasher.update(pricing_hash);
    let source_str = match token_source {
        TokenSource::ProviderUsage => "provider",
        TokenSource::CanonicalTokenizer => "tokenizer",
    };
    hasher.update(source_str.as_bytes());
    hasher.finalize().into()
}
```

Events represent the **canonical accounting record**.

Quota state must be derivable from the ordered sequence of events.

## Event Ordering

Events must be processed in deterministic order.

Ordering rule:

```
timestamp ASC, event_id ASC
```

This guarantees deterministic replay.

## Atomic Quota Deduction

Quota deduction must be performed atomically using the ledger-based approach (see Ledger-Based Architecture below). The ledger is the authoritative source of truth.

## Quota Consistency Model

**Critical consistency rule:**

Multiple routers processing requests simultaneously can cause **cross-router double-spend** if quota enforcement is not properly isolated.

**The double-spend problem:**

```
budget_limit = 1000
current_spend = 990

Router A reads:  current_spend = 990
Router B reads:  current_spend = 990

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

**Database constraint for safety:**

```sql
-- Add CHECK constraint to prevent any over-budget state
ALTER TABLE api_keys
ADD CONSTRAINT chk_budget_not_exceeded
CHECK (current_spend <= budget_limit);
```

**Canonical approach:** Use `record_usage()` from the Ledger-Based Architecture section below. This function uses `FOR UPDATE` row locking and derives spend from the ledger, providing deterministic accounting.

**Single-writer principle:**

For deterministic accounting across multiple routers:

```
Router → Primary DB (strong consistency) → Usage Event Recorded
```

## Idempotent Event Recording

To support retries, event recording must be idempotent.

Each request receives a **deterministic request_id**.

```rust
use sha2::{Sha256, Digest};

fn compute_request_id(key_id: &Uuid, timestamp: u64, nonce: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key_id.to_string().as_bytes());
    hasher.update(timestamp.to_le_bytes());
    hasher.update(nonce.as_bytes());
    hasher.finalize().into()
}
```

The database enforces:

```sql
UNIQUE(request_id)
```

Duplicate requests therefore cannot double charge.

## Usage Ledger

All usage events are written to a **ledger table**.

```sql
-- Spend ledger - THE authoritative economic record
-- Adopted from RFC-0903 (Final) spend_ledger schema for consistency
-- Token counts MUST originate from provider when available (see Canonical Token Accounting)
CREATE TABLE spend_ledger (
    event_id TEXT PRIMARY KEY,              -- UUID as text (36 chars with dashes)
    request_id TEXT NOT NULL,               -- UUID as text
    key_id TEXT NOT NULL,
    team_id TEXT,
    provider TEXT NOT NULL,                   -- Provider name
    model TEXT NOT NULL,                     -- Model name
    input_tokens INTEGER NOT NULL,            -- Prompt tokens
    output_tokens INTEGER NOT NULL,           -- Completion tokens
    cost_amount BIGINT NOT NULL,              -- Cost in smallest unit
    pricing_hash BYTEA(32) NOT NULL,         -- SHA256 of pricing table used
    timestamp INTEGER NOT NULL,                -- Unix epoch (authoritative event time)
    token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
    tokenizer_version TEXT,
    provider_usage_json TEXT,                  -- Raw provider usage for audit
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
pub fn replay_events(events: &[UsageEvent]) -> BTreeMap<Uuid, u64> {
    use std::collections::BTreeMap;
    let mut key_spend: BTreeMap<Uuid, u64> = BTreeMap::new();

    // Events must be sorted by timestamp (chronological), then event_id for determinism
    let mut sorted_events = events.to_vec();
    sorted_events.sort_by(|a, b| {
        a.timestamp.cmp(&b.timestamp)
            .then_with(|| a.event_id.cmp(&b.event_id))
    });

    for event in sorted_events {
        let entry = key_spend.entry(event.key_id).or_insert(0);
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingModel {
    pub model_name: String,
    /// Cost per 1K prompt tokens (in micro-units)
    pub prompt_cost_per_1k: u64,
    /// Cost per 1K completion tokens (in micro-units)
    pub completion_cost_per_1k: u64,
}

/// Global pricing table
pub fn get_pricing(model: &str) -> Option<PricingModel> {
    match model {
        "gpt-4" => Some(PricingModel {
            model_name: "gpt-4".to_string(),
            prompt_cost_per_1k: 30_000,  // $0.03 per 1K
            completion_cost_per_1k: 60_000, // $0.06 per 1K
        }),
        "gpt-3.5-turbo" => Some(PricingModel {
            model_name: "gpt-3.5-turbo".to_string(),
            prompt_cost_per_1k: 500,   // $0.0005 per 1K
            completion_cost_per_1k: 1500, // $0.0015 per 1K
        }),
        _ => None,
    }
}

/// Calculate cost deterministically
pub fn calculate_cost(
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
) -> Result<u64, Error> {
    let pricing = get_pricing(model)
        .ok_or_else(|| Error::UnknownModel(model.to_string()))?;

    // Integer math only - no floating point
    let prompt_cost = (input_tokens as u64 * pricing.prompt_cost_per_1k) / 1000;
    let completion_cost = (output_tokens as u64 * pricing.completion_cost_per_1k) / 1000;

    Ok(prompt_cost + completion_cost)
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

**Canonical tokenizer version constant:**

```rust
/// Canonical tokenizer version for deterministic accounting
/// All routers MUST use this exact version when provider usage is unavailable
const CANONICAL_TOKENIZER_VERSION: &str = "tiktoken-cl100k_base-v1.2.3";
```

**Pricing hash determinism:**

```
pricing_hash = SHA256(canonical pricing table JSON)
```

This ensures pricing determinism is defined even before RFC-0910 is implemented.

**CRITICAL invariant:**

```
For a given request_id, ALL routers MUST use the SAME token_source.
token_source MUST be included in event_id hash.
```

**Replay safety invariant:**

```
For a given request_id, only ONE usage event may exist.
This is enforced by UNIQUE(request_id) constraint.
```

## Provider Usage Reconciliation

Upstream provider responses may contain usage metadata.

The router must recompute cost using **its own pricing tables**, ignoring provider cost fields.

```rust
/// Process response and record usage
/// CRITICAL: Uses provider-reported tokens and deterministic event_id for cross-router determinism
/// Note: ProviderResponse.provider_usage_json contains the raw provider usage JSON for audit
pub fn process_response(
    db: &Database,
    key_id: &Uuid,
    team_id: Option<&str>,
    provider: &str,
    model: &str,
    response: &ProviderResponse,  // Contains: usage, timestamp, id, provider_usage_json
    pricing_hash: [u8; 32],
) -> Result<UsageEvent, Error> {
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

    // Calculate cost using deterministic pricing
    let cost_amount = calculate_cost(model, input_tokens, output_tokens)?;

    // Generate deterministic request_id (binary SHA256)
    let request_id = compute_request_id(key_id, response.timestamp, &response.id);

    // Generate deterministic event_id using SHA256 (not random UUID)
    let event_id = compute_event_id(
        &request_id,
        key_id,
        provider,
        model,
        input_tokens,
        output_tokens,
        &pricing_hash,
        token_source,
    );

    // Create usage event with token source for deterministic replay
    let event = UsageEvent {
        event_id,
        request_id,
        key_id: *key_id,
        team_id: team_id.map(String::from),
        timestamp: response.timestamp,
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens,
        output_tokens,
        cost_amount,
        pricing_hash,
        token_source,
        tokenizer_version,
        provider_usage_json: response.provider_usage_json.clone(),
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
            &event.pricing_hash,
            match event.token_source {
                TokenSource::ProviderUsage => "provider_usage",
                TokenSource::CanonicalTokenizer => "canonical_tokenizer",
            },
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
```

This guarantees:

```
deterministic billing
```

**Failure handling note:** The provider request is an external HTTP call outside the database transaction. If the provider succeeds but `record_usage` fails, the response has already been consumed. The compensating approach is to use idempotent `request_id` for retries — if a retry arrives with the same `request_id`, the `ON CONFLICT` will silently succeed, preventing double-billing.

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
fn checked_add_spend(current: u64, add: u64) -> Result<u64, Error> {
    current
        .checked_add(add)
        .ok_or_else(|| Error::OverflowDetected)
}
```

## Audit Proof Generation (Future)

The event ledger can be extended to generate **cryptographic proofs**.

```rust
use sha2::{Sha256, Digest};

/// Merkle tree node
#[derive(Debug, Clone)]
pub struct MerkleNode {
    pub hash: [u8; 32],
    pub left: Option<Box<MerkleNode>>,
    pub right: Option<Box<MerkleNode>>,
}

/// Build Merkle tree from usage events
pub fn build_merkle_tree(events: &[UsageEvent]) -> MerkleNode {
    // Sort events deterministically by event_id (binary comparison)
    let mut sorted = events.to_vec();
    sorted.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    // Build leaf nodes from binary event_id
    let mut leaves: Vec<[u8; 32]> = sorted
        .iter()
        .map(|e| {
            let mut hasher = Sha256::new();
            hasher.update(&e.event_id);  // Binary hash, not hex string
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
3. **Idempotent events** - `request_id UNIQUE` prevents double charging
4. **Deterministic event_id** - SHA256 hash ensures same request = same event across routers

**Quota enforcement with row locking:**

CRITICAL: To prevent race conditions in multi-router deployments, quota enforcement MUST use `FOR UPDATE` row locking.

```rust
/// Check and record spend with atomic row locking
/// CRITICAL: Uses FOR UPDATE to prevent race conditions in multi-router deployments
pub fn record_usage(
    db: &Database,
    key_id: &Uuid,
    event: &UsageEvent,
) -> Result<(), KeyError> {
    let tx = db.transaction()?;

    // 1. Lock the key row to prevent concurrent budget modifications
    // FOR UPDATE ensures only one transaction can modify this key at a time
    let budget: i64 = tx.query_row(
        "SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE",
        params![key_id.to_string()],
        |row| row.get(0),
    )?;

    // 2. Compute current spend from ledger (not a counter)
    let current: i64 = tx.query_row(
        "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
        params![key_id.to_string()],
        |row| row.get(0),
    )?;

    // 3. Check budget with locked row
    if current + event.cost_amount as i64 > budget {
        return Err(KeyError::BudgetExceeded { current: current as u64, limit: budget as u64 });
    }

    // 4. Insert into ledger (idempotent with ON CONFLICT - must match UNIQUE(key_id, request_id))
    tx.execute(
        "INSERT INTO spend_ledger (
            event_id, request_id, key_id, team_id, timestamp,
            provider, model, input_tokens, output_tokens, cost_amount,
            pricing_hash, token_source, tokenizer_version, provider_usage_json
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT(key_id, request_id) DO NOTHING",
        params![...],  // Same params as record_spend above
    )?;

    tx.commit()?;
    Ok(())
}
```

**Why FOR UPDATE is critical:**

Without row locking, two routers can race and overspend. With `FOR UPDATE`, only one transaction can modify a key at a time.

**Deterministic replay:**

```
1. SELECT * FROM spend_ledger ORDER BY timestamp, event_id
2. Recompute balances
3. Verify equality with any cached balances
```

Note: Ordering by `timestamp` (chronology from event itself) then `event_id` (tiebreaker) ensures deterministic replay. `timestamp` is authoritative; `created_at` was removed as it is database-insert-time which is non-deterministic across distributed nodes.

**Long-term enablement:**

Ledger architecture enables:

```
- Merkle root of usage ledger
- Cryptographic spend proofs
- Economic verification
- Verifiable AI infrastructure
```

## Future Extensions

Potential upgrades:

### Distributed accounting

Kafka-based event streams.

### Cryptographic audit

Merkle ledger snapshots.

### On-chain settlement

Publishing usage proofs to settlement layers.

## Relationship to RFC-0903

RFC-0903 defines:

```
authentication
authorization
rate limits
budgets
spend_ledger table schema (Final)
```

RFC-0909 defines:

```
how usage is measured and deducted
```

**Ledger adoption (v9):** RFC-0909 previously defined a parallel `usage_ledger` table with different column names and types. As of v9, RFC-0909 adopts RFC-0903's `spend_ledger` schema as the canonical ledger. Both RFCs now share the same ledger table definition (`spend_ledger` with `input_tokens`/`output_tokens`/`cost_amount`/`provider_usage_json` columns). This eliminates the earlier inconsistency where the two RFCs had conflicting ledger schemas.

Together they form the **quota router economic core**.

## Approval Criteria

This RFC can be approved when:

- deterministic cost units are implemented
- spend_ledger is append-only (per RFC-0903)
- atomic quota deduction is implemented
- idempotent request accounting exists

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| v9 | 2026-03-27 | Adopt RFC-0903 `spend_ledger` schema; remove parallel `usage_ledger` table; rename columns (`prompt_tokens`→`input_tokens`, `completion_tokens`→`output_tokens`, `cost_units`→`cost_amount`); add `provider_usage_json` field; remove `route` column |

---

**Draft Date:** 2026-03-25
**Version:** v9
**Related Use Case:** Enhanced Quota Router Gateway
**Related RFCs:** RFC-0903 (Virtual API Key System)
