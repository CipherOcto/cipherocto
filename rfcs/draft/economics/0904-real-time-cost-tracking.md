# RFC-0904 (Economics): Real-Time Cost Tracking

## Status

Draft (v1.4 — depends on RFC-0903 Final v30, RFC-0903-B1 v23, RFC-0903-C1 v4, RFC-0909 Final, RFC-0910 Draft)

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
| G4 | Fast budget pre-check | Non-locking, <1ms (storage-dependent) |
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
pub fn get_pricing(model: &str) -> Result<&'static PricingModel, BudgetError> {
    PRICING_TABLE
        .get(model)
        .ok_or(BudgetError::ModelNotFound(model.to_string()))
}
```

### Budget Pre-Check (Soft Limit)

**Non-atomic, fast pre-flight check.** The existing `check_budget(&ApiKey)` in middleware.rs (line 106) **is** the soft pre-check implementation. It takes an already-loaded `ApiKey` (which has `budget_limit` and `total_spend` already fetched) and returns `KeyError::BudgetExceeded` if the key is over budget.

```rust
/// Soft budget pre-check (existing implementation).
///
/// Non-locking query of current spend vs budget_limit.
/// This is a performance optimization only — does NOT prevent overspend
/// in concurrent scenarios.
///
/// Returns Err if:
/// - Key not found
/// - Current spend >= budget_limit
pub fn check_budget(&self, key: &ApiKey) -> Result<(), KeyError> {
    let spend = self.storage.get_spend(&key.key_id)?;

    if let Some(s) = spend {
        let remaining = key.budget_limit - s.total_spend;
        if remaining <= 0 {
            return Err(KeyError::BudgetExceeded {
                current: s.total_spend as u64,
                limit: key.budget_limit as u64,
            });
        }
    }

    Ok(())
}
```

**Note:** `s.total_spend` in `KeySpend` is in **micro-units** (same as `cost_amount` in `SpendEvent`), ensuring `budget_limit - total_spend` is a valid μunit comparison.

**When to use estimated_cost:**

The soft pre-check is informational — it does NOT block requests. It returns an error if the key is obviously over budget, but the authoritative check happens in `record_spend_ledger`.

For `estimated_cost`, use the **maximum possible cost for one request** based on provider rate limits:

| Provider | Model | Max Tokens | Ceiling Formula |
|----------|-------|-------------|-----------------|
| OpenAI | gpt-4o | 128,000 | max(128000 × prompt_cost_per_1k, 128000 × completion_cost_per_1k) / 1000 |
| Anthropic | claude-3-5 | 200,000 | max(200000 × prompt_cost_per_1k, 200000 × completion_cost_per_1k) / 1000 |

A safe conservative overestimate is `budget_limit` itself — the soft check passes for all requests that could possibly fit within budget.

### Atomic Spend Recording

**Atomic budget enforcement during spend recording.** Uses `SELECT ... FOR UPDATE` row locking per RFC-0903 §Lock Ordering Invariant.

This RFC describes the budget enforcement layer. The existing `KeyStorage::record_spend_ledger` (storage.rs) implements this pattern:

1. Locks the key row with `SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE`
2. Queries current spend from spend_ledger: `SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1`
3. Verifies `current + cost_amount <= budget_limit`
4. Inserts spend_event into spend_ledger

When the event has a `team_id`, `record_spend_ledger_with_team` is used instead, which locks team FIRST then key (deadlock prevention per RFC-0903 §Lock Ordering Invariant).

**Note:** The existing codebase also has a simpler `record_spend(key_id, amount)` (middleware.rs line 123) which inserts an amount **without budget check**. This is used for:
- Scenarios where budget enforcement is handled separately
- Test injection of spend without triggering budget checks
- Fallback after explicit budget-exceeded acknowledgment

The RFC's `record_spend_atomic` (using `record_spend_ledger`) is the normal path for production budget enforcement.

```rust
/// Atomic spend recording with budget enforcement (existing implementation).
///
/// Uses FOR UPDATE locking to prevent concurrent double-spend.
/// Budget limit is read from api_keys table (NOT from the event).
///
/// Dispatch: if event.team_id is Some, uses record_spend_ledger_with_team;
/// otherwise uses record_spend_ledger (key-only).
///
/// Returns Err if:
/// - Key not found
/// - Budget exceeded
pub fn record_spend_atomic(
    storage: &dyn KeyStorage,
    event: &SpendEvent,
) -> Result<(), KeyError> {
    match event.team_id {
        Some(ref team_id) => {
            storage.record_spend_ledger_with_team(
                &event.key_id.to_string(),
                &team_id.to_string(),
                event,
            )
        }
        None => storage.record_spend_ledger(event),
    }
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
) -> Result<Option<KeySpend>, KeyError> {
    storage.get_spend(key_id)
}
```

**Query team spend:** Deferred to Phase 2 (no implementation exists).

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
    /// Returns KeySpend with total_spend in micro-units (matching cost_amount).
    fn get_spend(&self, key_id: &Uuid) -> Result<Option<KeySpend>, KeyError>;
}
```

**Note:** `get_team_spend` (team aggregate) is **not included** — no implementation exists. It is deferred to Phase 2.

The existing `KeyStorage` trait already provides:
- `record_spend_ledger(event)` — atomic insert with FOR UPDATE key locking (key-only)
- `record_spend_ledger_with_team(key_id, team_id, event)` — atomic insert with team+key locking per RFC-0903 §Lock Ordering Invariant (team-enabled)

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

**Mitigation:** Per RFC-0903 §Lock Ordering Invariant, always lock `team` BEFORE `key`. The `record_spend_ledger_with_team` function follows this ordering.

### Soft Check Staleness

**Threat:** A key passes the soft pre-check (`check_budget`), then a concurrent request records spend that exhausts the budget, then the first request's `record_spend_ledger` is called — the atomic check correctly fails, but the soft check result was stale.

**Mitigation:** The soft check is purely informational. The **authoritative enforcement** is always in `record_spend_ledger` which uses `FOR UPDATE` locking. Callers must handle `BudgetExceeded` from `record_spend_ledger` even when the soft check passed.

**Note on error types:** The existing `check_budget(&ApiKey)` returns `KeyError::BudgetExceeded` because it predates `BudgetError`. This is existing behavior — the soft check and atomic check both surface budget errors via `KeyError` (since both are called through the middleware). `BudgetError` is defined for the RFC's public API surface but the internal implementation uses `KeyError`.

## LiteLLM Compatibility

This RFC provides budget tracking compatible with LiteLLM's `max_budget` feature:

| Feature | LiteLLM | This RFC |
|---------|---------|----------|
| Per-key budget | `max_budget` param | `api_keys.budget_limit` |
| Team budget | Via_org_budget | `teams.budget_limit` |
| Soft pre-check | Optional | `check_budget(&ApiKey)` (middleware.rs line 106) |
| Atomic enforcement | Built-in | `record_spend_ledger` / `record_spend_ledger_with_team` |
| Spend tracking | Database | spend_ledger |
| Budget reset | Via config | Future (F1) |

## Implementation Phases

### Phase 1: Core Budget Enforcement

- [x] Document `check_budget(&ApiKey)` as the soft pre-check (existing middleware.rs line 106)
- [x] Confirm `record_spend_ledger` covers key-only atomic enforcement (existing storage.rs)
- [x] Confirm `record_spend_ledger_with_team` covers team-enabled atomic enforcement (existing storage.rs)
- [x] Unit tests for cost calculation (`compute_cost`)
- [x] Use `BudgetError::ModelNotFound` in `get_pricing` (resolved in B10)

### Phase 2: Budget Queries

- [ ] Add `get_team_spend()` aggregate query (no implementation exists — deferred)
- [ ] Add admin API endpoints for budget status

### Phase 3: Budget Alerts (Future)

- [ ] Budget threshold notifications
- [ ] Auto-reset (daily, weekly, monthly)

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/storage.rs` | Confirm `record_spend_ledger` and `record_spend_ledger_with_team` cover atomic enforcement |
| `crates/quota-router-core/src/middleware.rs` | Confirm `check_budget(&ApiKey)` as soft pre-check (line 106) |

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
| 1.4     | 2026-04-22 | Round 3 adversarial review: archive Planned RFC-0904 placeholder; fix Phase 1 checklist; fix Key Files section; document record_spend (no budget check); remove get_team_spend; fix get_current_spend return type; add C1-C9 review section |
| 1.3     | 2026-04-22 | Round 2 adversarial review: fix B1-B12 (remove check_budget_soft_limit, replace InsufficientBudget, remove get_team_spend, clarify record_spend dispatch, fix KeySpend unit, document soft check staleness, update Phase 1 checklist, fix CostError→BudgetError) |
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

## Round 2 Adversarial Review

### B1: `check_budget_soft_limit` Uses Nonexistent `get_key_budget_limit`

**Severity:** Critical (Code Generation Error)

**Finding:** The function (lines 188-212) calls `storage.get_key_budget_limit(key_id)?`, but `get_key_budget_limit` is **not present** in the actual `KeyStorage` trait (storage.rs line 24). The `KeyStorage` trait has `get_spend` but not `get_key_budget_limit`.

The existing `check_budget(&ApiKey)` in middleware.rs (line 106) works differently — it takes an already-loaded `ApiKey` struct which contains `budget_limit`. It does NOT query storage for the budget limit.

**Resolution:** Remove `check_budget_soft_limit` as a separate function. The existing `check_budget(&ApiKey)` in middleware.rs **is** the soft pre-check implementation. Document it in this RFC rather than creating a new function with a non-existent method.

---

### B2: `BudgetError::InsufficientBudget` Does Not Exist in `BudgetError` Enum

**Severity:** Critical (Code Generation Error)

**Finding:** Line 204 returns `BudgetError::InsufficientBudget { ... }`, but the `BudgetError` enum (lines 311-339) defines `KeyBudgetExceeded` and `TeamBudgetExceeded` — **not** `InsufficientBudget`. The variant `InsufficientBudget` is referenced in the code but does not exist in the enum definition.

**Resolution:** Replace `InsufficientBudget` with `KeyBudgetExceeded`. The soft pre-check failure uses the same `KeyBudgetExceeded` variant as the atomic enforcement — they differ in context (soft check vs atomic), not in error type.

---

### B3: `get_team_spend` Declared but No Implementation Exists

**Severity:** High (Missing Implementation)

**Finding:** The `BudgetStorage` trait (lines 369-380) declares `fn get_team_spend(&self, team_id: &Uuid) -> Result<i64, KeyError>`. But there is **no implementation** of `get_team_spend` anywhere in `StoolapKeyStorage`. The RFC documents the SQL JOIN approach but provides no implementation path.

Additionally, `team_id` in the database is `BLOB(16)` per RFC-0903-C1, not `Uuid`. The trait signature using `&Uuid` requires a conversion that's not specified.

**Resolution:** Remove `get_team_spend` from the `BudgetStorage` trait. Mark team spend aggregation as Phase 2 future work. The existing `record_spend_ledger_with_team` handles team budget enforcement atomically without needing a separate `get_team_spend` query.

---

### B4: `record_spend_atomic` Delegation Ambiguity

**Severity:** High (Type Error)

**Finding:** The RFC's `record_spend_atomic` (line 248) delegates to `storage.record_spend_ledger(event)`. But the existing codebase has **two** methods:
- `record_spend_ledger` (key-only) — used when no team
- `record_spend_ledger_with_team(key_id, team_id, event)` — used when team exists

The RFC doesn't specify which is called. When `event.team_id` is `Some`, `record_spend_ledger_with_team` must be used. When `None`, `record_spend_ledger` is used.

**Resolution:** Clarify the dispatch logic:
```rust
if let Some(team_id) = event.team_id {
    storage.record_spend_ledger_with_team(&key_id.to_string(), &team_id.to_string(), event)
} else {
    storage.record_spend_ledger(event)
}
```

Or better: have `record_spend_ledger` internally dispatch based on whether the key has a team_id.

---

### B5: `KeySpend.total_spend` Is in Cents but Cost Amount Is in Micro-Units

**Severity:** High (Semantic Error)

**Finding:** `KeySpend.total_spend` (models.rs line 73) is documented as `// in cents/millicents`. But RFC-0904 §Unit System specifies that `cost_amount` in `spend_ledger` is in **micro-units** (μunits, 1 USD = 1,000,000 μunits). The middleware computes `key.budget_limit - s.total_spend` (middleware.rs line 110), comparing budget_limit (μunits) against total_spend (cents).

If budget_limit is in μunits (per RFC) and total_spend is in cents: 1 cent = 10,000 μunits. The subtraction is comparing incompatible units.

**Resolution:** `KeySpend.total_spend` must be in the same unit as `cost_amount` — micro-units. The comment in models.rs is stale. Update `KeySpend.total_spend` documentation to μunits and ensure the storage layer accumulates in μunits consistently.

---

### B6: Soft Check + Atomic Record Is Non-Atomic

**Severity:** Medium (Race Condition)

**Finding:** The existing `check_budget` (middleware.rs line 106) and `record_spend` (line 123) are **separate non-atomic operations**. A key could pass the soft check, then another concurrent request could record spend that exhausts the budget, then the first request records spend — overshooting budget.

The RFC acknowledges the soft check is "non-locking" but the actual request flow calls these separately. The atomic enforcement is in `record_spend_ledger` which uses `FOR UPDATE`, but by the time that runs, the soft check result may be stale.

**Resolution:** Document that the soft check is purely informational (<5ms fast reject for obviously over-budget keys). The **authoritative enforcement** is always in `record_spend_ledger` which uses `FOR UPDATE` locking. Callers must handle the case where `check_budget` passes but `record_spend_ledger` fails with `BudgetExceeded`.

---

### B7: Implementation Phase 1 Checklist References Removed Methods

**Severity:** Medium (Documentation Inconsistency)

**Finding:** The Implementation Phases checklist (lines 441-448) says:
- "Add `BudgetStorage` trait to `KeyStorage` in storage.rs"
- "Implement `get_key_budget_limit()`, `get_team_budget_limit()` in `StoolapKeyStorage`"
- "Implement `check_budget_soft_limit()` in middleware"

But the RFC itself established that these methods don't exist or shouldn't be created. The checklist is stale.

**Resolution:** Update Phase 1 checklist:
- [x] Document `check_budget(&ApiKey)` as the soft pre-check implementation (existing)
- [x] Confirm `record_spend_ledger` and `record_spend_ledger_with_team` cover atomic enforcement (existing)
- [ ] Add `get_team_spend` aggregate query (Phase 2 — no implementation exists)

---

### B8: `record_spend_with_team` Takes `&str` But DB Is `BLOB(16)` Post-RFC-0903-C1

**Severity:** Medium (Type Mismatch)

**Finding:** The function (lines 270-277) takes `key_id: &str` and `team_id: &str`, matching the **pre-C1 TEXT schema**. Per RFC-0903-C1, `key_id` and `team_id` in the database are now `BLOB(16)`. The existing storage API uses `&str` which was valid before C1 but requires implicit UUID→TEXT→BLOB conversion.

**Resolution:** The storage layer API should be updated post-C1 to accept `&Uuid` (or `&[u8; 16]` for raw BLOB). The RFC should note this as a post-C1 migration item. The RFC's function signature is documenting the current API, not the target API.

---

### B9: `compute_event_id` Excludes Timestamp — Correct for Idempotency

**Severity:** Medium (Determinism)

**Finding:** `compute_event_id` (keys/mod.rs line 126) does NOT include `timestamp` in the hash input. This means retries with the same `request_id` produce identical `event_id`, which is correct for idempotency. However, the `timestamp` field in `SpendEvent` is set from `SystemTime::now()` at creation time.

For retries: if request_id is reused, event_id is the same, `UNIQUE(event_id)` in spend_ledger prevents double-insert. Correct behavior.

**Resolution:** No change needed. The design is correct. Document this explicitly: "event_id is deterministic per request_id — retries produce identical event_ids, enabling idempotent replay."

---

### B10: `CostError` Referenced but Never Defined

**Severity:** Low (Missing Type)

**Finding:** Line 167 returns `CostError::ModelNotFound(model.to_string())` but `CostError` is never defined in the RFC. The error types section only defines `BudgetError`. `CostError` would be the natural error for cost computation operations, but it's absent.

**Resolution:** Use `BudgetError::ModelNotFound` instead of `CostError::ModelNotFound`. Per A12, `BudgetError` covers all cost-tracking operations including pricing lookup.

---

### B11: `BudgetError` Uuid vs DB `BLOB(16)` Mapping Not Documented

**Severity:** Low (Documentation Gap)

**Finding:** `BudgetError::KeyBudgetExceeded { key_id: Uuid, ... }` uses Rust `Uuid` type, but per RFC-0903-C1, database columns are `BLOB(16)`. The UUID↔BLOB conversion exists in the storage layer but is not documented.

**Resolution:** Add an API Compatibility Notes subsection documenting the UUID↔BLOB(16) conversion:
```rust
// Storage: UUID → BLOB(16)
let key_id_blob: Vec<u8> = key_id.as_bytes().to_vec();

// Lookup: BLOB(16) → UUID
let bytes: [u8; 16] = row.get("key_id")?;
let key_id = uuid::Uuid::from_bytes(bytes);
```

---

### B12: G4 `<5ms` Metric Not Specified or Verified

**Severity:** Low (Unverified Claim)

**Finding:** Design goal G4 (line 40) states `<5ms cost lookup` as a target metric. No benchmarking methodology is provided, and "lookup" is ambiguous (pricing table vs soft check vs atomic record).

**Resolution:** Remove the specific number. The soft pre-check is "fast, non-locking" — exact latency depends on storage implementation. Provide a qualitative statement instead of an unverified quantitative claim.

---

## Round 3 Adversarial Review

### C1: STALE PLANNED RFC-0904 PLACEHOLDER — Same Number, Conflicting Design

**Severity:** Critical (Process Violation)

**Finding:** There were **two RFC-0904 files** with the same number but completely different designs. The Planned placeholder (2026-03-12) used `f64`, `async fn`, `ModelPricing`, while the Draft v1.3 uses integer micro-units, sync functions.

**Resolution:** Archived the Planned placeholder to `rfcs/archived/economics/0904-real-time-cost-tracking.md`. No duplicate RFC numbers allowed.

---

### C2: Phase 1 Checklist References Resolved `CostError` Item

**Severity:** Medium (Stale Documentation)

**Finding:** Phase 1 checklist item "Add `CostError` type or clarify..." was stale — B10 resolved this by using `BudgetError::ModelNotFound`.

**Resolution:** Marked item as completed.

---

### C3: `Key Files to Modify` References Removed Methods

**Severity:** Medium (Stale Documentation)

**Finding:** Key Files section listed `check_budget_soft_limit()` which B1 removed.

**Resolution:** Updated to reflect actual methods: `check_budget(&ApiKey)` and `record_spend_ledger`/`record_spend_ledger_with_team`.

---

### C4: `record_spend` (No Budget Check) Exists but Is Not Documented

**Severity:** Medium (Missing Documentation)

**Finding:** The codebase has two record_spend methods:
- `middleware.record_spend(key_id, amount)` — simple insert, **no budget check**
- `middleware.process_response(...)` — computes `event_id`, calls `record_spend_ledger` with budget check

The RFC documented only the budget-checked version.

**Resolution:** Added documentation noting the simple `record_spend` exists as an alternative path when budget enforcement is handled separately.

---

### C5: `get_team_spend` Function Calls Nonexistent Storage Method

**Severity:** Low (Dead Code)

**Finding:** `get_team_spend` function called `storage.get_team_spend(team_id)` which doesn't exist in `KeyStorage`. B3 deferred this to Phase 2.

**Resolution:** Removed `get_team_spend` from Spend Query section. Team spend queries deferred to Phase 2.

---

### C6: `check_budget` Returns `KeyError` But RFC Says `BudgetError` for Cost Ops

**Severity:** Low (Design Clarification)

**Finding:** The existing `check_budget(&ApiKey)` returns `KeyError::BudgetExceeded`, but A12 says `BudgetError` is for cost-tracking operations. The soft check predates `BudgetError` — this is existing behavior.

**Resolution:** Documented that the existing implementation predates `BudgetError`. Future implementations may use `BudgetError` for cost-tracking operations.

---

### C7: `get_current_spend` Wraps `KeyError` → `BudgetError` Without `From` Impl

**Severity:** Low (Type Error)

**Finding:** `get_current_spend` returned `Result<..., BudgetError>` but underlying storage returns `KeyError`. The `.map_err(Into::into)` conversion requires `impl From<KeyError> for BudgetError`.

**Resolution:** Changed return type to `Result<Option<KeySpend>, KeyError>` to match underlying storage.

---

### C8: F2 Budget Auto-Reset Has No RFC Placeholder

**Severity:** Medium (Deferred Work)

**Finding:** F2 (budget auto-reset) has no Planned RFC. No action taken — this is a planning decision.

---

### C9: Lock Ordering in `record_spend_ledger_with_team` Verified

**Severity:** Medium (Verification)

**Finding:** Verified in storage.rs `record_spend_ledger_with_team` — team lock acquired first, then key lock. Lock ordering invariant is correctly implemented.

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
| B1 | Critical | `check_budget_soft_limit` calls nonexistent `get_key_budget_limit` | Fixed |
| B2 | Critical | `BudgetError::InsufficientBudget` variant does not exist | Fixed |
| B3 | High | `get_team_spend` declared but no implementation exists | Fixed |
| B4 | High | `record_spend_atomic` delegation ambiguity | Fixed |
| B5 | High | `KeySpend.total_spend` in cents vs `cost_amount` in μunits | Fixed |
| B6 | Medium | Soft check + atomic record is non-atomic | Fixed |
| B7 | Medium | Implementation Phase 1 checklist references removed methods | Fixed |
| B8 | Medium | `record_spend_with_team` takes `&str` but DB is `BLOB(16)` | Fixed |
| B9 | Medium | `compute_event_id` excludes timestamp — determinism correct | Fixed |
| B10 | Low | `CostError` referenced but never defined | Fixed |
| B11 | Low | `BudgetError` Uuid vs DB `BLOB(16)` mapping not documented | Fixed |
| B12 | Low | G4 `<5ms` metric not specified or verified | Fixed |
| C1 | Critical | Two RFC-0904 files with same number, conflicting designs | Fixed (archived Planned placeholder) |
| C2 | Medium | Phase 1 checklist references resolved `CostError` item | Fixed |
| C3 | Medium | Key Files section references removed `check_budget_soft_limit` | Fixed |
| C4 | Medium | `record_spend` (no budget check) exists but undocumented | Fixed |
| C5 | Low | `get_team_spend` function calls nonexistent storage method | Fixed |
| C6 | Low | `check_budget` returns `KeyError` but RFC says `BudgetError` for cost ops | Documented |
| C7 | Low | `get_current_spend` wraps `KeyError` → `BudgetError` without `From` impl | Fixed |
| C8 | Medium | F2 Budget auto-reset has no RFC placeholder | Flagged for planning |
| C9 | Medium | Lock ordering in `record_spend_ledger_with_team` not verified | Verified in storage.rs |

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
