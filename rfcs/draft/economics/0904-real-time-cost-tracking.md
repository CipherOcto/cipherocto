# RFC-0904 (Economics): Real-Time Cost Tracking

## Status

Draft (v1 — depends on RFC-0903 Final v30, RFC-0903-B1 v23, RFC-0903-C1 v4, RFC-0909 Final, RFC-0910 Draft)

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
- Use the **per-model ceiling cost** (worst-case for one request) as estimated_cost
- Or use `budget_limit` itself as a safe overestimate
- The actual cost will be computed after the LLM response

### Atomic Spend Recording

**Atomic budget enforcement during spend recording.** Uses `SELECT ... FOR UPDATE` row locking per RFC-0903 §Lock Ordering Invariant.

```rust
/// Record spend atomically with budget enforcement.
///
/// 1. Locks the key row (FOR UPDATE)
/// 2. Queries current spend from spend_ledger
/// 3. Verifies budget not exceeded
/// 4. Inserts spend_event into spend_ledger
///
/// Returns Err if:
/// - Key not found
/// - Budget exceeded
/// - Storage error
pub fn record_spend_atomic(
    storage: &dyn KeyStorage,
    event: &SpendEvent,
) -> Result<(), BudgetError> {
    // Step 1: Lock key row + get current spend in one query
    let current = storage.get_spend_for_update(&event.key_id)?;

    // Step 2: Check budget
    let new_total = current
        .map(|s| s.total_spend as u64)
        .unwrap_or(0)
        .saturating_add(event.cost_amount);

    if new_total > event.budget_limit as u64 {
        return Err(BudgetError::InsufficientBudget {
            current: current.map(|s| s.total_spend as u64).unwrap_or(0),
            limit: event.budget_limit as u64,
            requested: event.cost_amount,
        });
    }

    // Step 3: Insert spend event (budget already verified)
    storage.insert_spend_event(event)?;

    Ok(())
}
```

### Team Budget Enforcement

When a key belongs to a team, both key budget AND team budget must be enforced. Per RFC-0903 §Lock Ordering Invariant: **always lock team FIRST, then key** (to prevent deadlocks).

```rust
/// Record spend atomically with team budget enforcement.
///
/// 1. Locks team row (FOR UPDATE)
/// 2. Locks key row (FOR UPDATE)
/// 3. Queries current team spend + key spend from spend_ledger
/// 4. Verifies BOTH budgets not exceeded
/// 5. Inserts spend_event into spend_ledger
///
/// Returns Err if:
/// - Key or team not found
/// - Key budget exceeded
/// - Team budget exceeded
pub fn record_spend_with_team(
    storage: &dyn KeyStorage,
    event: &SpendEvent,
) -> Result<(), BudgetError> {
    let team_id = event.team_id
        .ok_or(BudgetError::TeamRequired)?;

    // Step 1: Lock team row + get team spend
    let team_spend = storage.get_team_spend_for_update(&team_id)?;

    // Step 2: Lock key row + get key spend
    let key_spend = storage.get_spend_for_update(&event.key_id)?;

    // Step 3: Check team budget
    let new_team_total = team_spend
        .map(|s| s.total_spend as u64)
        .unwrap_or(0)
        .saturating_add(event.cost_amount);

    let team_budget_limit = storage.get_team_budget_limit(&team_id)?
        .ok_or(BudgetError::TeamNotFound)?;

    if new_team_total > team_budget_limit as u64 {
        return Err(BudgetError::TeamBudgetExceeded {
            team_id,
            current: new_team_total,
            limit: team_budget_limit as u64,
            requested: event.cost_amount,
        });
    }

    // Step 4: Check key budget
    let new_key_total = key_spend
        .map(|s| s.total_spend as u64)
        .unwrap_or(0)
        .saturating_add(event.cost_amount);

    let key_budget_limit = storage.get_key_budget_limit(&event.key_id)?
        .ok_or(BudgetError::KeyNotFound)?;

    if new_key_total > key_budget_limit as u64 {
        return Err(BudgetError::KeyBudgetExceeded {
            key_id: event.key_id,
            current: new_key_total,
            limit: key_budget_limit as u64,
            requested: event.cost_amount,
        });
    }

    // Step 5: Insert spend event (both budgets already verified)
    storage.insert_spend_event(event)?;

    Ok(())
}
```

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
    /// Get key's budget_limit from api_keys table.
    fn get_key_budget_limit(&self, key_id: &Uuid) -> Result<Option<i64>, KeyError>;

    /// Get team's budget_limit from teams table.
    fn get_team_budget_limit(&self, team_id: &Uuid) -> Result<Option<i64>, KeyError>;

    /// Get current spend for a key (sum of cost_amount from spend_ledger).
    fn get_spend(&self, key_id: &Uuid) -> Result<Option<KeySpend>, KeyError>;

    /// Lock key row and get current spend (FOR UPDATE).
    /// Used in atomic spend recording.
    fn get_spend_for_update(&self, key_id: &Uuid) -> Result<Option<KeySpend>, KeyError>;

    /// Lock team row and get current team spend (FOR UPDATE).
    /// Used in atomic team spend recording.
    fn get_team_spend_for_update(&self, team_id: &Uuid) -> Result<Option<TeamSpend>, KeyError>;

    /// Get total spend for all keys in a team.
    fn get_team_spend(&self, team_id: &Uuid) -> Result<i64, KeyError>;

    /// Insert a spend event into spend_ledger.
    /// Called AFTER budget verification passes.
    fn insert_spend_event(&self, event: &SpendEvent) -> Result<(), KeyError>;
}
```

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
| 1.0     | 2026-04-22 | Initial draft |

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
