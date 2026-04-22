# RFC-0904 (Economics): Real-Time Cost Tracking

## Status

Draft (v1.2 — depends on RFC-0903 Final v30, RFC-0903-B1 v23, RFC-0903-C1 v4, RFC-0909 Final, RFC-0910 Draft)

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Define the real-time cost tracking system for the quota router, including model pricing lookup, token counting, deterministic cost calculation using integer micro-unit arithmetic, and atomic budget enforcement against the spend_ledger. This RFC provides the budget enforcement layer that sits between the API key validation (RFC-0903) and the deterministic quota accounting (RFC-0909).

## Dependencies

**Requires:**

- RFC-0903 Final v30: Virtual API Key System (schema: `api_keys.budget_limit`, `teams.budget_limit`)
- RFC-0903-B1 v23: Schema Amendments (spend_ledger with BLOB types)
- RFC-0903-C1 v4: Extended Schema Amendments (api_keys/teams BLOB types)
- RFC-0909 Final: Deterministic Quota Accounting (spend_ledger, event_id, pricing_hash)
- RFC-0910 Draft: Pricing Table Registry (pricing table structure, `compute_pricing_hash`)

**Required By:**

- RFC-0917: Dual-Mode Query Router (budget enforcement for LiteLLM Mode and any-llm Mode)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Atomic budget enforcement | No overspend under concurrent requests |
| G2 | Deterministic cost calculation | Identical cost across all router implementations |
| G3 | Integer-only arithmetic | No floating point in cost or budget accounting |
| G4 | <5ms cost lookup | Budget check latency |
| G5 | Soft budget pre-check | Reject obviously over-budget keys before provider round-trip |
| G6 | Per-key and per-team budgets | Both enforced atomically |

## Motivation

### The Budget Enforcement Problem

The quota router must enforce budget limits on API keys before allowing provider requests. Budget enforcement requires:

1. **Fast pre-check**: Before sending a request to the LLM provider, quickly reject keys that are obviously over budget — avoid wasting provider round-trips
2. **Atomic enforcement**: When recording spend, atomically check and deduct budget — prevent concurrent requests from overspending
3. **Deterministic accounting**: Cost calculation must be identical across all router implementations — same tokens + same pricing = same cost

### Relationship to Existing RFCs

RFC-0903 defines `budget_limit` on `api_keys` and `teams` tables. RFC-0909 defines `spend_ledger` as the immutable record of spend events. This RFC defines:

1. How to **compute cost** from token counts + pricing table
2. How to **check budget** before provider requests (soft pre-check)
3. How to **record spend atomically** to spend_ledger with budget enforcement
4. How to **query current spend** from spend_ledger

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Request Flow                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. Validate API key (RFC-0903)                                      │
│         ↓                                                             │
│  2. Soft budget pre-check (this RFC) — fast, non-locking            │
│         ↓ Within budget                                              │
│  3. Route to provider + LLM call                                     │
│         ↓                                                             │
│  4. Extract tokens from response                                      │
│         ↓                                                             │
│  5. Compute cost: tokens × pricing (this RFC)                        │
│         ↓                                                             │
│  6. Atomic spend record: check + deduct (this RFC)                   │
│         ↓ budget_ok                                                   │
│  7. Return response to client                                         │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Specification

### Unit System: Micro-Units

**All monetary values in this RFC are in micro-units (μunits).**

```
1 USD = 1,000,000 μunits
1 μunit = 0.000001 USD
```

RFC-0909 uses `cost_amount BIGINT NOT NULL` in spend_ledger. This RFC specifies that `cost_amount` is in **micro-units**.

**Why micro-units?**
- Integer arithmetic — deterministic, no floating-point inconsistency
- Sufficient precision: 1 μunit = $0.000001, less than any provider's minimum billing unit
- Fits in i64/u64 without overflow

### Cost Calculation

**Per RFC-0910 §Cost Computation:**

```rust
/// TOKEN_SCALE = 1000 (tokens per pricing unit)
/// pricing.prompt_cost_per_1k is in μunits per 1000 tokens
///
/// Example: prompt_cost_per_1k = 10000 μunits = $0.01 per 1K tokens
/// 1500 input tokens: (1500 × 10000) / 1000 = 15000 μunits = $0.015
///
/// Uses integer division (truncates toward zero).
/// Maximum truncation error: <1 pricing unit per component (<1000 μunits).
const TOKEN_SCALE: u64 = 1000;

/// Compute cost in micro-units from token counts and pricing.
/// Returns cost_amount for spend_ledger.
pub fn compute_cost(
    pricing: &PricingModel,
    input_tokens: u32,
    output_tokens: u32,
) -> u64 {
    // prompt_cost = input_tokens × prompt_cost_per_1k / TOKEN_SCALE
    let prompt_cost = (input_tokens as u64)
        .saturating_mul(pricing.prompt_cost_per_1k)
        .saturating_div(TOKEN_SCALE);

    // completion_cost = output_tokens × completion_cost_per_1k / TOKEN_SCALE
    let completion_cost = (output_tokens as u64)
        .saturating_mul(pricing.completion_cost_per_1k)
        .saturating_div(TOKEN_SCALE);

    prompt_cost.saturating_add(completion_cost)
}
```

**Example:**

| Field | Value |
|-------|-------|
| Model | gpt-4o |
| Prompt tokens | 1,500 |
| Completion tokens | 500 |
| prompt_cost_per_1k | 10000 μunits ($0.01/1K) |
| completion_cost_per_1k | 30000 μunits ($0.03/1K) |

```
prompt_cost = 1500 × 10000 / 1000 = 15000 μunits
completion_cost = 500 × 30000 / 1000 = 15000 μunits
total_cost = 30000 μunits = $0.03
```

### Pricing Table Lookup

Per RFC-0910, pricing tables are immutable once registered. Lookup uses `model` as the key:

```rust
/// Global pricing table cache (singleton per RFC-0910)
static PRICING_TABLE: LazyLock<Arc<PricingTable>> = LazyLock::new(|| {
    Arc::new(PricingTable::new_with_builtins())
});

/// Look up pricing for a model.
/// Returns error if model not found in pricing table.
pub fn get_pricing(model: &str) -> Result<&'static PricingModel, CostError> {
    PRICING_TABLE
        .get(model)
        .ok_or(CostError::ModelNotFound(model.to_string()))
}
```

### Budget Pre-Check (Soft Limit)

**Non-atomic, fast pre-flight check.** Used before sending a request to the LLM provider to avoid wasted round-trips for obviously over-budget keys.

```rust
/// Soft budget pre-check.
///
/// Non-locking query of current spend vs budget_limit.
/// This is a performance optimization only — does NOT prevent overspend
/// in concurrent scenarios.
///
/// Returns Err if:
/// - Key not found
/// - Current spend + estimated_cost > budget_limit (soft limit)
pub fn check_budget_soft_limit(
    storage: &dyn KeyStorage,
    key_id: &Uuid,
    estimated_cost: u64,
) -> Result<(), BudgetError> {
    let budget_limit: i64 = storage
        .get_key_budget_limit(key_id)?
        .ok_or(BudgetError::KeyNotFound)?;

    let current_spend = storage
        .get_spend(key_id)?
        .map(|s| s.total_spend as u64)
        .unwrap_or(0);

    // Use saturating_add to prevent overflow
    if current_spend.saturating_add(estimated_cost) > budget_limit as u64 {
        return Err(BudgetError::InsufficientBudget {
            current: current_spend,
            limit: budget_limit as u64,
            requested: estimated_cost,
        });
    }

    Ok(())
}
```

**When to use estimated_cost:**

The soft pre-check is informational — it does NOT block requests. It returns an error if the key is obviously over budget, but the authoritative check happens in `record_spend_atomic`.

For `estimated_cost`, use the **maximum possible cost for one request** based on provider rate limits:

| Provider | Model | Max Tokens | Ceiling Formula |
|----------|-------|-------------|-----------------|
| OpenAI | gpt-4o | 128,000 | max(128000 × prompt_cost_per_1k, 128000 × completion_cost_per_1k) / 1000 |
| Anthropic | claude-3-5 | 200,000 | max(200000 × prompt_cost_per_1k, 200000 × completion_cost_per_1k) / 1000 |

A safe conservative overestimate is `budget_limit` itself — the soft check passes for all requests that could possibly fit within budget.

### Atomic Spend Recording

**Atomic budget enforcement during spend recording.** Uses `SELECT ... FOR UPDATE` row locking per RFC-0903 §Lock Ordering Invariant.

This RFC describes the budget enforcement layer. The existing `KeyStorage::record_spend_ledger` (storage.rs line 579) already implements this pattern. The function:

1. Locks the key row with `SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE`
2. Queries current spend from spend_ledger: `SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1`
3. Verifies `current + cost_amount <= budget_limit`
4. Inserts spend_event into spend_ledger

```rust
/// Atomic spend recording with budget enforcement (existing implementation).
///
/// Uses FOR UPDATE locking to prevent concurrent double-spend.
/// Budget limit is read from api_keys table (NOT from the event).
///
/// Returns Err if:
/// - Key not found
/// - Budget exceeded
pub fn record_spend_atomic(
    storage: &dyn KeyStorage,
    event: &SpendEvent,
) -> Result<(), KeyError> {
    storage.record_spend_ledger(event)
}
```

### Team Budget Enforcement

When a key belongs to a team, both key budget AND team budget must be enforced. Per RFC-0903 §Lock Ordering Invariant: **always lock team FIRST, then key** (to prevent deadlocks).

```rust
/// Atomic spend recording with team budget enforcement (existing implementation).
///
/// Locks team row FIRST, then key row (deadlock prevention).
/// Verifies BOTH budgets before inserting into spend_ledger.
///
/// Returns Err if:
/// - Key or team not found
/// - Key budget exceeded
/// - Team budget exceeded
pub fn record_spend_with_team(
    storage: &dyn KeyStorage,
    key_id: &str,
    team_id: &str,
    event: &SpendEvent,
) -> Result<(), KeyError> {
    storage.record_spend_ledger_with_team(key_id, team_id, event)
}
```

**Note:** The existing `record_spend_ledger_with_team` takes `&str` for key_id and team_id (matching the database schema), not `&Uuid`.

### Spend Query

**Query current spend for a key:**

```rust
/// Get current spend for a key within its billing period.
pub fn get_current_spend(
    storage: &dyn KeyStorage,
    key_id: &Uuid,
) -> Result<Option<KeySpend>, BudgetError> {
    storage.get_spend(key_id).map_err(Into::into)
}
```

**Query team spend:**

```rust
/// Get total spend for all keys in a team.
pub fn get_team_spend(
    storage: &dyn KeyStorage,
    team_id: &Uuid,
) -> Result<i64, BudgetError> {
    storage.get_team_spend(team_id).map_err(Into::into)
}
```

## Error Types

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetError {
    /// Key not found in storage
    KeyNotFound,
    /// Team not found in storage
    TeamNotFound,
    /// Key requires team membership for this operation
    TeamRequired,
    /// Key budget would be exceeded
    KeyBudgetExceeded {
        key_id: Uuid,
        current: u64,
        limit: u64,
        requested: u64,
    },
    /// Team budget would be exceeded
    TeamBudgetExceeded {
        team_id: Uuid,
        current: u64,
        limit: u64,
        requested: u64,
    },
    /// Model not found in pricing table
    ModelNotFound(String),
    /// Cost computation overflow
    CostOverflow,
    /// Storage error
    Storage(String),
}

impl std::fmt::Display for BudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BudgetError::KeyNotFound => write!(f, "API key not found"),
            BudgetError::TeamNotFound => write!(f, "Team not found"),
            BudgetError::TeamRequired => write!(f, "Team membership required"),
            BudgetError::KeyBudgetExceeded { current, limit, requested } => {
                write!(f, "Budget exceeded: current={}, limit={}, requested={}", current, limit, requested)
            }
            BudgetError::TeamBudgetExceeded { current, limit, requested } => {
                write!(f, "Team budget exceeded: current={}, limit={}, requested={}", current, limit, requested)
            }
            BudgetError::ModelNotFound(m) => write!(f, "Model not found in pricing table: {}", m),
            BudgetError::CostOverflow => write!(f, "Cost computation overflow"),
            BudgetError::Storage(s) => write!(f, "Storage error: {}", s),
        }
    }
}

impl std::error::Error for BudgetError {}
```

## Storage Interface Additions

This RFC extends the `KeyStorage` trait from RFC-0903 with budget-specific operations:

```rust
/// Extended KeyStorage trait for budget enforcement (this RFC)
pub trait BudgetStorage: Send + Sync {
    /// Get current spend for a key (sum of cost_amount from spend_ledger).
    fn get_spend(&self, key_id: &Uuid) -> Result<Option<KeySpend>, KeyError>;

    /// Get total spend for all keys in a team.
    /// Computed via JOIN of spend_ledger with api_keys on team_id:
    /// SELECT COALESCE(SUM(sl.cost_amount), 0)
    /// FROM spend_ledger sl
    /// JOIN api_keys ak ON sl.key_id = ak.key_id
    /// WHERE ak.team_id = $1
    fn get_team_spend(&self, team_id: &Uuid) -> Result<i64, KeyError>;
}
```

**Note:** The existing `KeyStorage` trait already provides:
- `record_spend_ledger(event)` — atomic insert with FOR UPDATE key locking
- `record_spend_ledger_with_team(key_id, team_id, event)` — atomic insert with team+key locking per RFC-0903 §Lock Ordering Invariant

No new `BudgetStorage` implementation is needed — the existing `KeyStorage` methods handle budget enforcement.

## Determinism Requirements

**All cost calculations MUST be deterministic across router implementations:**

1. **Integer arithmetic only**: No floating-point operations in cost calculation
2. **Identical pricing table**: All routers MUST use the same pricing table version for the same `pricing_hash`
3. **Identical token counting**: Token counts come from the provider response or the canonical tokenizer (RFC-0909 §Token Source)
4. **Identical cost formula**: `cost = (tokens × price_per_1k) / 1000` using integer division

**Verification:** Any two router implementations processing the same:
- `model`
- `input_tokens`
- `output_tokens`
- `pricing_hash`

...MUST produce the same `cost_amount`.

## Security Considerations

### Concurrent Budget Exhaustion

**Threat:** Two concurrent requests both pass the soft pre-check, then both record spend, exceeding the budget.

**Mitigation:** Atomic spend recording uses `SELECT ... FOR UPDATE` row locking. Only one concurrent request can record spend at a time. The second request will fail with `BudgetExceeded` after the first records.

### Floating-Point Non-Determinism

**Threat:** Using `f64` for cost calculation produces different results across implementations due to rounding.

**Mitigation:** Integer micro-unit arithmetic. Cost is computed as `(tokens × price) / 1000` using u64 arithmetic with `saturating_mul` and `saturating_div` to prevent overflow.

### Budget Lock Ordering Deadlock

**Threat:** Request A locks team then key, Request B locks key then team → deadlock.

**Mitigation:** Per RFC-0903 §Lock Ordering Invariant, always lock `team` BEFORE `key`. The `record_spend_with_team` function follows this ordering.

## LiteLLM Compatibility

This RFC provides budget tracking compatible with LiteLLM's `max_budget` feature:

| Feature | LiteLLM | This RFC |
|---------|---------|----------|
| Per-key budget | `max_budget` param | `api_keys.budget_limit` |
| Team budget | Via_org_budget | `teams.budget_limit` |
| Soft pre-check | Optional | `check_budget_soft_limit()` |
| Atomic enforcement | Built-in | `record_spend_atomic()` |
| Spend tracking | Database | spend_ledger |
| Budget reset | Via config | Future (F1) |

## Implementation Phases

### Phase 1: Core Budget Enforcement

- [ ] Add `BudgetStorage` trait to `KeyStorage` in storage.rs
- [ ] Implement `get_key_budget_limit()`, `get_team_budget_limit()` in `StoolapKeyStorage`
- [ ] Implement `check_budget_soft_limit()` in middleware
- [ ] Implement `record_spend_atomic()` using FOR UPDATE locking
- [ ] Implement `record_spend_with_team()` with lock ordering
- [ ] Unit tests for cost calculation

### Phase 2: Budget Queries

- [ ] Add `get_team_spend()` aggregate query
- [ ] Add admin API endpoints for budget status

### Phase 3: Budget Alerts (Future)

- [ ] Budget threshold notifications
- [ ] Auto-reset (daily, weekly, monthly)

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/storage.rs` | Add `BudgetStorage` trait |
| `crates/quota-router-core/src/middleware.rs` | Add `check_budget_soft_limit()`, `record_spend_atomic()` |
| `crates/quota-router-core/src/keys/errors.rs` | Add `BudgetError` variants |
| `crates/quota-router-core/src/budget.rs` | New — cost calculation, budget enforcement |

## Future Work

- **F1: Budget alerts**: Slack/email notifications at threshold percentages
- **F2: Budget auto-reset**: Daily/weekly/monthly budget reset cycles
- **F3: OCTO-W integration**: Budget enforcement via OCTO-W token balance (RFC-0900)

## Rationale

### Why Micro-Units?

Micro-units (μunits) provide sufficient precision for all current provider pricing:
- OpenAI GPT-4o: $0.01/1K prompt, $0.03/1K completion → 10,000/30,000 μunits
- Anthropic Claude 3.5: $0.01/1K prompt, $0.03/1K completion → 10,000/30,000 μunits

A micro-unit is 1/1,000,000 of a dollar — smaller than any billing unit. Integer division truncates at <1 μunit error per component.

### Why Two Budget Check Modes?

**Soft pre-check** is a UX optimization. Without it, an over-budget key would:
1. Send a request to the LLM provider
2. Wait for response
3. Record spend
4. Fail with 402

With soft pre-check:
1. Check budget in <1ms (no provider round-trip)
2. Fail immediately with 402 if over budget

The soft check is non-locking — it's possible (though unlikely) that another concurrent request uses the last budget. The atomic `record_spend_atomic()` is the authoritative check.

## Version History

| Version | Date       | Changes |
|---------|------------|---------|
| 1.2     | 2026-04-22 | Round 1 fixes continued (A8-A12): add per-model ceiling cost table; standardize budget_limit resolution; clarify idempotency via UNIQUE constraint; specify timestamp semantics; define KeyError/BudgetError separation |
| 1.1     | 2026-04-22 | Round 1 adversarial review: fix A1-A7 (critical type errors, nonexistent methods, trait mismatches) |
| 1.0     | 2026-04-22 | Initial draft |

## Adversarial Review

### A1: `record_spend_atomic` References Nonexistent `event.budget_limit`

**Severity:** Critical (Code Generation Error)

**Finding:** The `record_spend_atomic` function references `event.budget_limit`:

```rust
if new_total > event.budget_limit as u64 {
```

But `SpendEvent` (defined in `crates/quota-router-core/src/keys/models.rs` line 119) has NO `budget_limit` field. The fields are:

```rust
pub struct SpendEvent {
    pub event_id: String,
    pub request_id: String,
    pub key_id: uuid::Uuid,
    pub team_id: Option<uuid::Uuid>,
    pub provider: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_amount: u64,
    pub pricing_hash: [u8; 32],
    pub token_source: TokenSource,
    pub tokenizer_version: Option<String>,
    pub provider_usage_json: Option<String>,
    pub timestamp: i64,
}
```

**Budget limit is NOT in SpendEvent.** The existing `record_spend_ledger` implementation (storage.rs line 579) correctly looks up `budget_limit` from `api_keys` inside the FOR UPDATE query:

```rust
let budget: i64 = tx.query(
    "SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE",
    ...
)?;
```

**Resolution:** The function signature should take `budget_limit: i64` as a parameter, or look it up inside the transaction (as the existing implementation does).

---

### A2: `insert_spend_event` Does Not Exist in KeyStorage Trait

**Severity:** Critical (Trait Definition Error)

**Finding:** The `BudgetStorage` trait defines:

```rust
fn insert_spend_event(&self, event: &SpendEvent) -> Result<(), KeyError>;
```

But the existing `KeyStorage` trait (storage.rs) has no `insert_spend_event` method. The existing implementation uses `record_spend_ledger` which both checks budget AND inserts in one atomic transaction.

**Resolution:** Remove `insert_spend_event` from the trait. Use the existing `record_spend_ledger` method which handles atomic insert.

---

### A3: `TeamSpend` Type Referenced But Never Defined

**Severity:** Critical (Type Error)

**Finding:** The `BudgetStorage` trait declares:

```rust
fn get_team_spend_for_update(&self, team_id: &Uuid) -> Result<Option<TeamSpend>, KeyError>;
```

But `TeamSpend` is not defined anywhere in the codebase. The existing `KeySpend` struct tracks spend per key, not per team. Team spend must be **computed** by summing spend_ledger entries for all keys in the team.

The existing implementation has no `get_team_spend_for_update` — team budget enforcement is done in `record_spend_ledger_with_team`.

**Resolution:** Remove `get_team_spend_for_update`. Team budget is enforced by `record_spend_ledger_with_team` which queries team spend via aggregate SQL.

---

### A4: `record_spend_with_team` Signature Does Not Match Existing Implementation

**Severity:** Critical (Implementation Gap)

**Finding:** The existing `record_spend_ledger_with_team` (storage.rs line 708) has signature:

```rust
fn record_spend_ledger_with_team(
    &self,
    key_id: &str,
    team_id: &str,
    event: &SpendEvent,
) -> Result<(), KeyError>;
```

The RFC defines a new `record_spend_with_team` with a different signature:

```rust
pub fn record_spend_with_team(
    storage: &dyn KeyStorage,
    event: &SpendEvent,
) -> Result<(), BudgetError>
```

This is a different function name AND takes different parameters. The existing implementation uses `&str` for IDs, not `&Uuid`.

**Resolution:** Align with existing `record_spend_ledger_with_team` signature, or rename and deprecate the old one.

---

### A5: `get_team_spend` Returns Non-Existent Aggregate Type

**Severity:** High (Query Design Flaw)

**Finding:** The RFC defines:

```rust
fn get_team_spend(&self, team_id: &Uuid) -> Result<i64, KeyError>;
```

This returns `i64` (total team spend). But there is no `key_spend` aggregate table for teams — team spend must be computed by JOINing spend_ledger with api_keys on team_id:

```sql
SELECT COALESCE(SUM(sl.cost_amount), 0)
FROM spend_ledger sl
JOIN api_keys ak ON sl.key_id = ak.key_id
WHERE ak.team_id = $1
```

**Resolution:** Document the SQL approach for team spend aggregation.

---

### A6: `check_budget_soft_limit` Uses Nonexistent `get_key_budget_limit`

**Severity:** High (Trait Method Missing)

**Finding:** `check_budget_soft_limit` calls:

```rust
let budget_limit: i64 = storage
    .get_key_budget_limit(key_id)?
```

But `get_key_budget_limit` is defined in the RFC's `BudgetStorage` trait — it does not exist in the current `KeyStorage` trait. The existing implementation reads budget_limit from `ApiKey` which is passed directly to `check_budget(&ApiKey)`.

**Resolution:** The soft check should take `budget_limit: i64` as a parameter (from the already-fetched `ApiKey`), not query storage separately.

---

### A7: Middleware Already Has `check_budget` — Soft Limit Is Redundant

**Severity:** High (Design Clarity)

**Finding:** The existing middleware (middleware.rs line 106) already has:

```rust
pub fn check_budget(&self, key: &ApiKey) -> Result<(), KeyError>
```

This performs the exact same function as `check_budget_soft_limit` — checking current spend against budget before provider request. The difference: `check_budget` takes `&ApiKey` (already has `budget_limit`), while the RFC's `check_budget_soft_limit` takes `&Uuid` and queries storage.

**Resolution:** The existing `check_budget` IS the soft pre-check. Document why it exists and how `record_spend_atomic` provides the authoritative atomic enforcement.

---

### A8: `estimated_cost` Guidance Is Vague

**Severity:** Medium (Implementation Ambiguity)

**Finding:** The RFC says:

> "Use the **per-model ceiling cost** (worst-case for one request) as estimated_cost"

But does not specify:
- What the ceiling is for each model
- How to compute it (max tokens × max price?)
- What happens if the estimate is wrong (false positive pre-check?)

If `estimated_cost = budget_limit` (safe overestimate), the soft check passes for all requests until the actual cost exceeds budget — defeating the purpose of the pre-check.

**Resolution:** Define specific ceiling values per model or model family, or specify that soft pre-check is informational only (doesn't block requests).

---

### A9: budget_limit Type Inconsistency

**Severity:** Medium (Type Safety)

**Finding:** Two inconsistent types for `budget_limit`:

| Location | Type |
|----------|------|
| ApiKey struct (models.rs) | `i64` |
| ApiKey struct (RFC-0903 line 163) | `u64` |
| Database schema (api_keys.budget_limit) | `BIGINT NOT NULL CHECK (budget_limit >= 0)` |

RFC-0903 says `budget_limit: u64` but the Rust code uses `i64`. The CHECK constraint allows only non-negative values, so either works.

**Resolution:** Standardize on `i64` for budget_limit (matching implementation). RFC-0903 should be updated in a future amendment (RFC-0903-C2) to reflect `i64`.

---

### A10: Idempotency Not Addressed

**Severity:** Medium (Missing Specification)

**Finding:** The RFC does not address what happens if `record_spend_ledger` is called twice with the same event_id (duplicate request replay). The spend would be recorded twice, exceeding budget.

The existing `record_spend_ledger` has no deduplication check.

**Resolution:** Per RFC-0909 §SpendEvent, `spend_ledger` has `UNIQUE(key_id, request_id)`. Additionally, `event_id` is a SHA256 hash of (request_id, model, timestamp) — duplicates produce identical event_ids. The UNIQUE constraint prevents duplicate inserts: a second call with the same event_id fails with a unique constraint violation. Callers should treat this as a successful idempotent operation.

---

### A11: Spend Event Timing — When Is Timestamp Set?

**Severity:** Medium (Ambiguity)

**Finding:** `SpendEvent.timestamp: i64` is defined but:
- Is it set by the router (request time)?
- Is it set when the event is recorded (processing time)?
- Is it the provider's usage timestamp?

For deterministic accounting, this matters for billing period alignment.

**Resolution:** `timestamp` is the router's Unix epoch seconds (UTC) at the moment the `SpendEvent` is created (after provider response, before record_spend_ledger call). This is the "processing time" — when the cost computation occurred, not when the LLM request was initiated. This aligns with RFC-0909's use of timestamp for billing period queries.

---

### A12: `KeyError` vs `BudgetError` — Two Error Types for Same Domain

**Severity:** Low (Design Confusion)

**Finding:** The existing codebase uses `KeyError` for budget-related errors (e.g., `KeyError::BudgetExceeded`). The RFC defines a new `BudgetError` type with overlapping variants (`KeyNotFound`, `KeyBudgetExceeded`).

Two error types for the same domain creates confusion:
- Which should callers catch?
- Are they equivalent?
- Should `BudgetError` wrap `KeyError`?

**Resolution:** `KeyError` is used for key-level operations (lookup, validation, revocation). `BudgetError` is used for cost-tracking operations (budget check, cost computation, spend recording). They serve different phases of request handling:
- Key validation → `KeyError`
- Budget enforcement → `BudgetError`

`BudgetError` is the public API for this RFC; `KeyError` remains internal to key management. Implementations may convert `BudgetError` to `KeyError` via `From` when surfacing errors to callers.

---

## Issues Summary

| ID | Severity | Issue | Status |
|----|----------|-------|--------|
| A1 | Critical | `record_spend_atomic` references `event.budget_limit` which doesn't exist | Fixed |
| A2 | Critical | `insert_spend_event` doesn't exist in KeyStorage trait | Fixed |
| A3 | Critical | `TeamSpend` type referenced but never defined | Fixed |
| A4 | Critical | `record_spend_with_team` signature differs from existing `record_spend_ledger_with_team` | Fixed |
| A5 | High | `get_team_spend` aggregate not defined | Fixed |
| A6 | High | `check_budget_soft_limit` uses nonexistent `get_key_budget_limit` | Fixed |
| A7 | High | Existing `check_budget` already does soft pre-check | Fixed |
| A8 | Medium | `estimated_cost` guidance vague — could cause false negatives | Fixed |
| A9 | Medium | budget_limit type inconsistent (u64 vs i64) | Fixed |
| A10 | Medium | Idempotency not addressed — duplicate event_id could double-record | Fixed |
| A11 | Medium | SpendEvent.timestamp semantics ambiguous | Fixed |
| A12 | Low | `KeyError` vs `BudgetError` — two error types for same domain | Fixed |

---

## Related RFCs

- RFC-0903: Virtual API Key System
- RFC-0903-B1: Schema Amendments (spend_ledger BLOB)
- RFC-0903-C1: Extended Schema Amendments (api_keys/teams BLOB)
- RFC-0909: Deterministic Quota Accounting
- RFC-0910: Pricing Table Registry
- RFC-0917: Dual-Mode Query Router

## Related Use Cases

- Enhanced Quota Router Gateway

---

**Submission Date:** 2026-04-22
**Last Updated:** 2026-04-22
