# RFC-0904 (Economics): Real-Time Cost Tracking

## Status

Draft (v1.17 — depends on RFC-0903 Final v30, RFC-0903-B1 v23, RFC-0903-C1 v4, RFC-0909 Final, RFC-0910 Draft)

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

| Goal | Target                         | Metric                                                       |
| ---- | ------------------------------ | ------------------------------------------------------------ |
| G1   | Atomic budget enforcement      | No overspend under concurrent requests                       |
| G2   | Deterministic cost calculation | Identical cost across all router implementations             |
| G3   | Integer-only arithmetic        | No floating point in cost or budget accounting               |
| G4   | Fast budget pre-check          | Non-locking (storage-dependent latency)                        |
| G5   | Soft budget pre-check          | Reject obviously over-budget keys before provider round-trip |
| G6   | Per-key and per-team budgets   | Both enforced atomically                                     |

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

| Field                  | Value                   |
| ---------------------- | ----------------------- |
| Model                  | gpt-4o                  |
| Prompt tokens          | 1,500                   |
| Completion tokens      | 500                     |
| prompt_cost_per_1k     | 10000 μunits ($0.01/1K) |
| completion_cost_per_1k | 30000 μunits ($0.03/1K) |

```
prompt_cost = 1500 × 10000 / 1000 = 15000 μunits
completion_cost = 500 × 30000 / 1000 = 15000 μunits
total_cost = 30000 μunits = $0.03
```

**Test Vectors:**

| input_tokens | output_tokens | prompt_cost_per_1k (μunits) | completion_cost_per_1k (μunits) | expected_cost (μunits) |
| ------------ | ------------- | --------------------------- | ------------------------------- | ---------------------- |
| 1500         | 500           | 10000                       | 30000                           | 30000                  |
| 1000         | 1000          | 10000                       | 30000                           | 40000                  |
| 0            | 500           | 10000                       | 30000                           | 15000                  |
| 500          | 0             | 10000                       | 30000                           | 5000                   |

### Pricing Table Lookup

Per RFC-0910, pricing tables are immutable once registered. Lookup uses `model` as the key:

```rust
/// Global pricing table cache (singleton per RFC-0910)
static PRICING_TABLE: LazyLock<Arc<PricingTable>> = LazyLock::new(|| {
    Arc::new(PricingTable::new_with_builtins())
});

/// Look up pricing for a model.
/// Returns error if model not found in pricing table.
///
/// **Validation:** Empty string is not a valid model name — callers should validate
/// before calling. Implementations may treat empty string as ModelNotFound.
pub fn get_pricing(model: &str) -> Result<&'static PricingModel, BudgetError> {
    PRICING_TABLE
        .get(model)
        .ok_or(BudgetError::ModelNotFound(model.to_string()))
}
```

**Builtin models:** `new_with_builtins()` loads pricing for OpenAI and Anthropic models at startup:

- OpenAI: `gpt-4o`, `gpt-4o-mini`, `gpt-4-turbo`, `gpt-3.5-turbo`
- Anthropic: `claude-3-5-haiku`, `claude-3-5-sonnet`, `claude-3-opus`

`ModelNotFound` is returned only for models not in the built-in set (e.g., new providers, custom models added via RFC-0910 §Dynamic Registration).

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

**budget_limit == 0 (unlimited) enforcement:** When `budget_limit == 0`, the soft pre-check passes without error — unlimited keys are not subject to budget enforcement. `budget_limit == 0` means "no budget limit," not "zero budget."

The correct behavior: `check_budget` must short-circuit when `budget_limit == 0`:

```rust
pub fn check_budget(&self, key: &ApiKey) -> Result<(), KeyError> {
    // Unlimited keys skip budget enforcement
    if key.budget_limit == 0 {
        return Ok(());
    }

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

The atomic enforcement in `record_spend_ledger` also handles `budget_limit == 0` correctly: it skips the budget check entirely when `budget_limit == 0` (the condition is `budget_limit > 0 && current_spend + cost_amount > budget_limit`).

**Unlimited key buildup:** The soft pre-check passes for `budget_limit == 0` (unlimited) keys regardless of spend amount — `check_budget` short-circuits on unlimited keys without inspecting spend. This is correct behavior: unlimited keys have no budget to exceed. However, this means the soft pre-check cannot detect spend *buildup* on unlimited keys (e.g., tracking spend for reporting even when not enforcing a limit). For unlimited keys, the atomic `record_spend_ledger` still records spend but skips budget enforcement — use `get_current_spend` for spend monitoring on unlimited keys.

**Error type distinction (S9):** The soft pre-check (`check_budget`) returns `KeyError::BudgetExceeded` internally because it predates the RFC's `BudgetError` type. External API surfaces (Admin API, webhook payloads) expose `BudgetError` variants. Internal middleware uses `KeyError` for storage-level errors. Callers converting `KeyError` → `BudgetError` should map: `KeyError::BudgetExceeded` → `BudgetError::KeyBudgetExceeded`, `KeyError::TeamBudgetExceeded` → `BudgetError::TeamBudgetExceeded`, `KeyError::KeyNotFound` → `BudgetError::KeyNotFound`.

````

**Note:** `s.total_spend` in `KeySpend` is in **micro-units** (same as `cost_amount` in `SpendEvent`), ensuring `budget_limit - total_spend` is a valid μunit comparison.

**When to use estimated_cost:**

The soft pre-check is informational — it does NOT block requests. It returns an error if the key is obviously over budget, but the authoritative check happens in `record_spend_ledger`.

For `estimated_cost`, use the **maximum possible cost for one request** based on provider rate limits:

| Provider  | Model      | Max Tokens | Ceiling Formula                                                          |
| --------- | ---------- | ---------- | ------------------------------------------------------------------------ |
| OpenAI    | gpt-4o     | 128,000    | max(128000 × prompt_cost_per_1k, 128000 × completion_cost_per_1k) / 1000 |
| Anthropic | claude-3-5 | 200,000    | max(200000 × prompt_cost_per_1k, 200000 × completion_cost_per_1k) / 1000 |

A safe conservative overestimate is `budget_limit` itself — the soft check passes for all requests that could possibly fit within budget.

### Atomic Spend Recording

**Atomic budget enforcement during spend recording.** Uses `SELECT ... FOR UPDATE` row locking per RFC-0903 §Lock Ordering Invariant.

This RFC describes the budget enforcement layer. The existing `KeyStorage::record_spend_ledger` (storage.rs) implements this pattern:

1. Locks the key row with `SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE`
2. Queries current spend from spend_ledger: `SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1`
3. Verifies `current + cost_amount <= budget_limit`
4. Inserts spend_event into spend_ledger. If a duplicate event_id is detected (idempotent replay), returns `Ok(())` without inserting a duplicate row — per UNIQUE constraint on event_id.

When the event has a `team_id`, `record_spend_ledger_with_team` is used instead, which locks team FIRST then key (deadlock prevention per RFC-0903 §Lock Ordering Invariant).

**Note:** The existing codebase also has a simpler `record_spend(key_id, amount)` (middleware.rs line 123) which inserts an amount **without budget check**. This writes to the `key_spend` table (for soft pre-check reads) but does NOT write to `spend_ledger`. It is used for:

- Test injection of spend without triggering budget checks
- Legacy paths where budget enforcement is handled separately

The RFC's `record_spend_atomic` (using `record_spend_ledger`) is the normal path for production budget enforcement — it writes to `spend_ledger` with atomic budget check.

**Interaction between `record_spend` and `record_spend_ledger`:** Both write to `key_spend` table (for accumulated spend tracking). Only `record_spend_ledger` writes to `spend_ledger` (the immutable audit log). The `get_spend` query reads from `key_spend` — both functions contribute to the same total.

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
````

### Team Budget Enforcement

When a key belongs to a team, both key budget AND team budget must be enforced. Per RFC-0903 §Lock Ordering Invariant: **always lock team FIRST, then key** (to prevent deadlocks).

```rust
/// Atomic spend recording with team budget enforcement (existing implementation).
///
/// Locks team row FIRST, then key row (deadlock prevention).
/// Verifies BOTH budgets before inserting into spend_ledger.
///
/// Returns Err if:
/// - Key not found
/// - Team not found (orphaned key — team row deleted without cascading key revocation; should not occur with proper FK enforcement)
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
    key_id: &str,
) -> Result<Option<KeySpend>, KeyError> {
    storage.get_spend(key_id)
}
```

**Billing period:** The time window over which spend is tracked. Default: calendar month (UTC). Configurable per key via `api_keys.metadata` as `{"billing_period": "monthly"}` (options: `daily`, `weekly`, `monthly`). The billing period is independent of auto-reset (F2) — billing period controls reporting queries; auto-reset controls spend counter reset.

**First billing period:** Begins at key creation time and ends at the next period boundary (end of current day/week/month UTC). Example: a key created Jan 15 with monthly billing has its first period Jan 15 00:00 UTC → Jan 31 23:59:59 UTC, then aligns to calendar months thereafter.

**Query team spend:**

```rust
/// Get total spend for all keys in a team within a billing period.
///
/// team_id is BLOB(16) in the database — storage layer handles UUID→BLOB conversion.
/// period_start and period_end are Unix epoch seconds defining the billing period.
/// Returns total spend in micro-units (non-negative u64, zero if team has no keys).
pub fn get_team_spend(
    storage: &dyn KeyStorage,
    team_id: &str,
    period_start: i64,
    period_end: i64,
) -> Result<u64, KeyError> {
    let team_spend: u64 = storage
        .query_row(
            "SELECT COALESCE(SUM(sl.cost_amount), 0)
             FROM spend_ledger sl
             JOIN api_keys ak ON sl.key_id = ak.key_id
             WHERE ak.team_id = $1
               AND sl.timestamp >= $2
               AND sl.timestamp < $3",
            [team_id, period_start.to_string(), period_end.to_string()],
        )
        .map_err(Into::into)?;
    Ok(team_spend)
}
```

The SQL JOIN aggregates all `cost_amount` values from `spend_ledger` for keys belonging to the team within the specified billing period.

**Period filtering:** `timestamp >= period_start AND timestamp < period_end` aligns with the RFC-0909 §SpendEvent immutability principle — `timestamp` is the event time stored in `spend_ledger.created_at` field.

**Empty team case:** If `team_id` exists in the `teams` table but has no keys (or all keys have been deleted), the SQL returns `COALESCE(SUM(...), 0)` = 0. This returns `Ok(0)`, not an error. `404 TeamNotFound` means the team does not exist in `teams`, not that the team is empty.

**Backward compatibility note (S4):** R14-03 added `period_start` and `period_end` parameters to support billing period filtering. Call sites that previously called `get_team_spend(storage, team_id)` must be updated to pass the billing period boundaries. For current-period queries, derive `period_start`/`period_end` from the billing period definition (calendar month start/end UTC).

**Period derivation formula:** Compute `period_start`/`period_end` for a given billing period as:

```rust
// Monthly (default): start of current calendar month, start of next calendar month
let now = Utc::now();
let period_start = now.date().with_day(1).unwrap().and_hms_opt(0, 0, 0).unwrap().timestamp();
// Next month: advance month by 1, wrap year at December, set day to 1
let year = now.date().year();
let month = now.date().month();
let next_year = if month == 12 { year + 1 } else { year };
let next_month_num = if month == 12 { 1 } else { month + 1 };
let period_end = NaiveDate::from_ymd_opt(next_year, next_month_num, 1)
    .unwrap().and_hms_opt(0, 0, 0).unwrap().timestamp();

// Weekly: start of current week (Monday 00:00 UTC)
let days_since_monday = now.weekday().num_days_from_monday();
let period_start = (now - chrono::Duration::days(days_since_monday as i64)).date()
    .and_hms_opt(0, 0, 0).unwrap().timestamp();
let period_end = (now - chrono::Duration::days(days_since_monday as i64) + chrono::Duration::days(7)).date()
    .and_hms_opt(0, 0, 0).unwrap().timestamp();

// Daily: start of current day (00:00 UTC)
let period_start = now.date().and_hms_opt(0, 0, 0).unwrap().timestamp();
let period_end = (now + chrono::Duration::days(1)).date().and_hms_opt(0, 0, 0).unwrap().timestamp();
```

For first-period alignment (key created mid-period), `period_start` is the creation timestamp aligned to the period boundary (00:00 UTC), and `period_end` is the next period boundary.

## Error Types

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetError {
    /// Key not found in storage.
    /// **Used by:** Admin API GET /admin/budget/key/{key_id} when key does not exist.
    KeyNotFound,
    /// Team not found in storage.
    /// **Used by:** Admin API GET /admin/budget/team/{team_id} when no keys exist for the team.
    /// **Note:** get_team_spend returns 0 (not error) when team has no keys.
    TeamNotFound,
    /// Key budget would be exceeded.
    /// **Used by:** record_spend_ledger (atomic enforcement) when key budget check fails.
    KeyBudgetExceeded {
        key_id: Uuid,
        current: u64,
        limit: u64,
        requested: u64,
    },
    /// Team budget would be exceeded.
    /// **Used by:** record_spend_ledger_with_team when team budget check fails (key budget passed).
    /// If both key AND team budgets would be exceeded, KeyBudgetExceeded is returned (checked first).
    TeamBudgetExceeded {
        team_id: Uuid,
        current: u64,
        limit: u64,
        requested: u64,
    },
    /// Model not found in pricing table.
    /// **Used by:** get_pricing when model is not in the pricing table (including custom/dynamic models).
    ModelNotFound(String),
    /// Cost computation overflow.
    /// **Theoretically reachable only** — compute_cost uses saturating_mul/saturating_div.
    /// Would require prompt_cost_per_1k or completion_cost_per_1k to be near u64::MAX.
    CostOverflow,
    /// Insufficient OCTO-W balance for estimated cost (F3 OCTO-W integration).
    /// **Used by:** F3 pre-check when OCTO-W balance < estimated_cost.
    InsufficientBalance {
        key_id: Uuid,
        available: u64,
        estimated: u64,
    },
    /// Storage error.
    /// **Used by:** All storage operations on database failure.
    Storage(String),
}

impl std::fmt::Display for BudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BudgetError::KeyNotFound => write!(f, "API key not found"),
            BudgetError::TeamNotFound => write!(f, "Team not found"),
            BudgetError::KeyBudgetExceeded { current, limit, requested } => {
                write!(f, "Budget exceeded: current={}, limit={}, requested={}", current, limit, requested)
            }
            BudgetError::TeamBudgetExceeded { current, limit, requested } => {
                write!(f, "Team budget exceeded: current={}, limit={}, requested={}", current, limit, requested)
            }
            BudgetError::ModelNotFound(m) => write!(f, "Model not found in pricing table: {}", m),
            BudgetError::CostOverflow => write!(f, "Cost computation overflow"),
            BudgetError::InsufficientBalance { key_id, available, estimated } => {
                write!(f, "Insufficient OCTO-W balance for key {}: available={}, estimated={}", key_id, available, estimated)
            }
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
    fn get_spend(&self, key_id: &str) -> Result<Option<KeySpend>, KeyError>;
}
```

**Note:** `get_team_spend` is not part of the `BudgetStorage` trait — it is a standalone function defined in the Spend Query section above with SQL JOIN implementation.

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

**`pricing_hash` computation:** The `pricing_hash` is computed per RFC-0910 §compute_pricing_hash — see RFC-0910 for the full algorithm, field ID assignments, and test vectors. RFC-0904 does not redefine the hash algorithm here.

**`pricing_hash` test vector (S6):** RFC-0910 defines the canonical test vector for `compute_pricing_hash`: given the input table with `table_id="openai-gpt4-v1"`, `version=1`, `provider="openai"`, `model="gpt-4"`, `prompt_cost_per_1k=30_000`, `completion_cost_per_1k=60_000`, `effective_from=1704067200`, `metadata={}`, the expected hash is `4a065c51147d4730379d600c4a491778b98f66a8e381c5dfdf51f42052c32f60`. Implementers should verify their `compute_pricing_hash` implementation against this test vector to ensure cross-router determinism.

**`pricing_hash` stability:** RFC-0910 defines the canonical algorithm. RFC-0904 assumes RFC-0910 is stable — once a `pricing_hash` is recorded in `spend_ledger`, it must not be recomputed with a different algorithm, as this would break deterministic cost verification. If RFC-0910 changes its algorithm in a breaking way, RFC-0904 must be amended to track the change or declare a version break.

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

**Lock granularity note:** Team-level locking serializes spend recording across all keys in a team — only one spend event can be recorded for any key in a team at a time. For teams with many keys and high request volume, this can become a throughput bottleneck. High-traffic teams may benefit from sub-team partitioning (future work).

### Soft Check Staleness

**Threat:** A key passes the soft pre-check (`check_budget`), then a concurrent request records spend that exhausts the budget, then the first request's `record_spend_ledger` is called — the atomic check correctly fails, but the soft check result was stale.

**Mitigation:** The soft check is purely informational. The **authoritative enforcement** is always in `record_spend_ledger` which uses `FOR UPDATE` locking. Callers must handle `BudgetExceeded` from `record_spend_ledger` even when the soft check passed.

**Note on error types:** The existing `check_budget(&ApiKey)` returns `KeyError::BudgetExceeded` because it predates `BudgetError`. This is existing behavior — the soft check and atomic check both surface budget errors via `KeyError` (since both are called through the middleware). `BudgetError` is defined for the RFC's public API surface but the internal implementation uses `KeyError`. Callers converting `KeyError` to `BudgetError` should use `KeyError::BudgetExceeded` → `BudgetError::KeyBudgetExceeded` (with same `current`/`limit`/`requested` fields) and `KeyError::TeamBudgetExceeded` → `BudgetError::TeamBudgetExceeded`.

**Note on `BudgetError::TeamBudgetExceeded`:** This variant is defined for API completeness but is not returned by any documented function. The team budget exceeded case returns `KeyError::TeamBudgetExceeded` from `record_spend_ledger_with_team`. Implementations may convert `KeyError::TeamBudgetExceeded` to `BudgetError::TeamBudgetExceeded` via `From` when surfacing to external callers.

### Token Source Validation

`record_spend_ledger` validates `token_source` against the `CHECK (token_source IN ('provider_usage', 'canonical_tokenizer'))` constraint on `spend_ledger`. Values outside this set cause a constraint violation error at insert time.

**F2 reset does NOT write to spend_ledger:** Reset events are logged to the separate `budget_reset_log` table (which has no `token_source` column), not to `spend_ledger`. Therefore, the `token_source` CHECK constraint is not relevant to F2 reset events. The statement "Budget reset events use a special internal token_source value" (prior versions) was incorrect — F2 reset does not touch `spend_ledger` at all.

### Integration with RFC-0917

RFC-0917 (Dual-Mode Query Router) depends on this RFC for budget enforcement. RFC-0917 operates in two modes:

- **LiteLLM Mode**: Uses per-key budgets from `api_keys.budget_limit` with soft pre-check + atomic enforcement
- **Any-LLM Mode**: Uses per-key budgets with the same enforcement path

The interface between RFC-0917 and RFC-0904 is the `check_budget(&ApiKey)` soft pre-check and `record_spend_ledger`/`record_spend_ledger_with_team` atomic enforcement. RFC-0917 calls these at the appropriate points in the request lifecycle, but the budget enforcement logic itself lives in this RFC.

**RFC-0904 status note:** RFC-0904 is currently in **Draft** status. RFC-0917's Phase 4 integration with budget enforcement depends on RFC-0904 reaching **Accepted** status. RFC-0917 may operate with basic `budget_limit` enforcement (RFC-0903 only) until RFC-0904 is accepted, but full budget enforcement per this RFC requires RFC-0904 to be Accepted.

## LiteLLM Compatibility

This RFC provides budget tracking compatible with LiteLLM's `max_budget` feature:

| Feature            | LiteLLM            | This RFC                                                |
| ------------------ | ------------------ | ------------------------------------------------------- |
| Per-key budget     | `max_budget` param | `api_keys.budget_limit`                                 |
| Team budget        | Via_org_budget     | `teams.budget_limit`                                    |
| Soft pre-check     | Optional           | `check_budget(&ApiKey)` (middleware.rs line 106)        |
| Atomic enforcement | Built-in           | `record_spend_ledger` / `record_spend_ledger_with_team` |
| Spend tracking     | Database           | spend_ledger                                            |
| Budget reset       | Via config         | F2 auto-reset (daily/weekly/monthly)                                             |

## Implementation Phases

### Phase 1: Core Budget Enforcement

- [x] Document `check_budget(&ApiKey)` as the soft pre-check (existing middleware.rs line 106)
- [x] Confirm `record_spend_ledger` covers key-only atomic enforcement (existing storage.rs)
- [x] Confirm `record_spend_ledger_with_team` covers team-enabled atomic enforcement (existing storage.rs)
- [x] Unit tests for cost calculation (`compute_cost`)
- [x] Use `BudgetError::ModelNotFound` in `get_pricing` (resolved in B10)

### Phase 2: Budget Queries

- [x] Add `get_team_spend()` aggregate query (SQL JOIN specified above)
- [x] Add admin API endpoints for budget status

#### Admin API Endpoints for Budget Status

Budget status endpoints for administrative monitoring. **Protocol:** REST over HTTPS (HTTP/1.1 or HTTP/2). **Authentication:** Bearer token in `Authorization` header — token must have admin privileges. On auth failure: `401 Unauthorized {"error": "Unauthorized"}`. On insufficient privileges: `403 Forbidden {"error": "Forbidden"}`.

**Bearer token specification:**
- Format: `Authorization: Bearer <token>` where `<token>` is a hex-encoded secret (minimum 32 bytes / 64 hex characters)
- Validation: Constant-time comparison against the configured admin token (hashed via SHA-256 in storage)
- Token provisioning: Out-of-band — the admin token is stored in the server's secrets configuration (environment variable, secrets manager, or configuration file). No API endpoint exists to create or rotate admin tokens.
- Token rotation: Out-of-band — generate a new token, update the server configuration, restart if necessary. Clients must update their token immediately.
- Failed auth rate limiting: Implementations SHOULD rate-limit failed auth attempts (recommended: block after 5 failed attempts for 5 minutes). No auth success/count is tracked per token.
- Minimum token requirements: 32 bytes of entropy (64 hex characters). Tokens shorter than this MUST be rejected with `400 Bad Request {"error": "InvalidTokenFormat"}`.

**Path parameter format:** `key_id` and `team_id` must be valid UUID strings (lowercase, hyphenated, e.g., `"550e8400-e29b-41d4-a716-446655440000"`). Invalid format returns `400 Bad Request {"error": "InvalidUuidFormat"}`.

```
GET /admin/budget/key/{key_id}
  → 200 OK {
      key_id: String,
      budget_limit: u64,       // in μunits; 0 means unlimited
      current_spend: u64,      // in μunits
      remaining: i64,         // budget_limit - current_spend (μunits); may be negative if budget exceeded
      percent_used: u64 | null, // budget_limit > 0 ? (current_spend.saturating_mul(100)) / budget_limit : null; null if budget_limit == 0 (unlimited)
      created_at: i64 | null  // Unix epoch of most recent spend_ledger INSERT for this key; null only if key has no spend_ledger rows (never spent)
    }
  → 404 Not Found {"error": "KeyNotFound", "key_id": "..."}
  → 500 Internal Server Error {"error": "Storage", "detail": "..."}

GET /admin/budget/team/{team_id}?include_revoked=false  (default: true)
  → 200 OK {
      team_id: String,
      budget_limit: u64,      // in μunits; sum of per-key budget_limit values across active keys in team; 0 means all keys unlimited
      current_spend: u64,     // in μunits; sum of current_spend across all team keys (active + revoked unless filtered)
      remaining: i64,         // budget_limit - current_spend (μunits); may be negative if budget exceeded
      percent_used: u64 | null, // budget_limit > 0 ? (current_spend.saturating_mul(100)) / budget_limit : null; null if budget_limit == 0 (all keys unlimited)
      key_count: i32,          // count of keys in team (active + revoked unless filtered; deleted keys excluded)
      created_at: i64 | null  // Unix epoch of most recent spend event across all team keys; null if no spend
    }
  → 404 Not Found {"error": "TeamNotFound", "team_id": "..."}  // team_id does not exist in teams table
  → 500 Internal Server Error {"error": "Storage", "detail": "..."}

**`include_revoked` query parameter:** When `true` (default, backwards-compatible), `current_spend` and `key_count` include revoked keys. When `false`, only active (non-revoked) keys are included in the aggregation.

GET /admin/budget/team/{team_id}/keys?include_revoked=false&offset=0&limit=100  (defaults: include_revoked=false, offset=0, limit=100)
  → 200 OK {
    keys: [{
      key_id: String,
      budget_limit: u64,      // in μunits; 0 means unlimited
      current_spend: u64,    // in μunits
      remaining: i64,        // budget_limit - current_spend (μunits); may be negative if budget exceeded
      percent_used: u64 | null  // (current_spend.saturating_mul(100)) / budget_limit in hundredths; null if budget_limit == 0
    }, ...],
    pagination: {
      offset: i64,    // requested offset
      limit: i64,    // requested limit
      total: i64      // total keys matching filter (active + revoked unless filtered)
    }
  }
  → 404 Not Found {"error": "TeamNotFound", "team_id": "..."}  // team_id does not exist
  → 200 OK {keys: [], pagination: {offset: 0, limit: 100, total: 0}}  // team exists but has no keys matching the filter
  → 500 Internal Server Error {"error": "Storage", "detail": "..."}
```

**`remaining` semantics:** `budget_limit - current_spend`. May be negative if `current_spend > budget_limit` (e.g., concurrent requests exhausted budget). Negative `remaining` indicates the budget is already exceeded.

**`created_at` for teams:** Returns the Unix epoch of the most recent spend event across all keys in the team (computed as `MAX(created_at)` across all team keys via SQL aggregation).

**Team `remaining` semantics:** When keys have per-key `budget_limit` values that differ from the team-level `budget_limit`, `remaining` reflects team-level budget only — it does NOT account for per-key budget exhaustion. For per-key budget details, use `GET /admin/budget/team/{team_id}/keys`.

**`percent_used` null case:** `percent_used` is `null` when `budget_limit == 0` (unlimited budget). Division by zero is prevented by omitting the field.

**Team all-unlimited case:** If all keys in the team have `budget_limit == 0` (unlimited), the team's aggregate `budget_limit` is 0 (unlimited team) and `percent_used` is `null`. The team is considered unlimited even if individual key spend is non-zero — the team-level budget is the sum of per-key limits, and the sum of zeros is zero (unlimited).

**Team zero keys:** Returns `200 OK {keys: []}` with `key_count: 0`, not `404`. `404 TeamNotFound` means the `team_id` does not exist in the `teams` table.

**Pagination edge cases:** `limit=0` returns an empty `keys` array with `total` still populated (valid request). If `limit` exceeds a maximum (implementation-defined, recommended 1000), clamp to the maximum. `offset >= total` returns an empty `keys` array (no wrap-around).

**Team `current_spend` data source:** Team `current_spend` is the sum of `key_spend.total_spend` across all relevant team keys (active + revoked unless filtered). This is consistent with how the soft pre-check reads work — `get_spend` queries `key_spend`, not `spend_ledger` directly. Both `record_spend` (no budget check) and `record_spend_ledger` (with budget check) write to `key_spend`, so this aggregation reflects all recorded spend.

**`key_spend` table schema:** The `key_spend` table tracks accumulated spend per key for fast pre-check queries. Schema (per `crates/quota-router-core/src/schema.rs`):

```sql
CREATE TABLE key_spend (
    key_id BLOB(16) NOT NULL UNIQUE,  -- Raw UUID bytes (16 bytes) — matches api_keys.key_id per RFC-0903-C1
    total_spend INTEGER NOT NULL DEFAULT 0,  -- accumulated spend in μunits
    window_start INTEGER NOT NULL,            -- period start (reset boundary)
    last_updated INTEGER NOT NULL             -- last modification time
);
CREATE INDEX idx_key_spend_key_id ON key_spend(key_id);
```

The `total_spend` field accumulates both `record_spend` (no budget check) and `record_spend_ledger` (with budget check) contributions. On F2 reset, both `total_spend` and `window_start` are reset to zero and current time respectively.

**F1 alert state endpoint:**

```
GET /admin/budget/key/{key_id}/alerts
  → 200 OK {
      key_id: String,
      budget_limit: u64,       // in μunits; 0 means unlimited
      thresholds: [50, 80, 90],  // configured thresholds from metadata
      fired: [{threshold: 80, period_start: 1743465600}, ...],  // threshold value + billing period when it fired
      period_start: i64        // current billing period start (Unix epoch) — this response covers this period
    }
  → 404 Not Found {"error": "KeyNotFound", "key_id": "..."}
  → 500 Internal Server Error {"error": "Storage", "detail": "..."}
```

**`fired` array:** Lists thresholds that have already fired in the current billing period. Each entry is `(threshold, period_start)` from `budget_alert_log`. Empty array means no alerts have fired yet. The response does NOT include thresholds that haven't fired — the caller infers unfired thresholds from the `thresholds` config. `period_start` identifies the billing period; `fired_at` can be reconstructed from `budget_alert_log.fired_at` if needed but is omitted from the API response for size efficiency.

**`thresholds` source:** Read from `api_keys.metadata` → `budget_alert_thresholds` JSON array. Returns empty array if not configured.

### Phase 3: Budget Alerts

- [x] Budget threshold notifications (F1 — fully specced above)
- [x] Auto-reset (daily, weekly, monthly) (F2 — fully specced above)

#### Budget Alerts (F1)

Budget alerts notify when spending reaches configurable thresholds:

```
Alert trigger: current_spend >= (budget_limit * threshold_percent / 100)
Condition: budget_limit > 0 (alerts do not fire for unlimited keys)
Default thresholds: 50%, 80%, 90%, 100%
```

**`current_spend` source (S7):** The alert trigger reads `current_spend` from `key_spend.total_spend` — the same accumulated counter used by the soft pre-check. Both `record_spend` (no budget check) and `record_spend_ledger` (with budget check) contribute to `key_spend.total_spend`, ensuring the F1 alert handler tracks the same spend that the soft pre-check sees. The F1 alert is evaluated **synchronously** post-spend (after `record_spend_ledger` or `deduct_octo_w` records the cost), so the alert fires if the just-recorded spend crossed a threshold. Evaluation is synchronous: the webhook is dispatched before the response is returned to the client. If webhook delivery fails after all retries, the alert is dropped (at-least-once guarantee is per-delivery, not per-request — the client request is not failed due to alert delivery failure).

**Uses budget_limit, not effective_budget:** The alert trigger compares `current_spend` against `budget_limit` directly — not against `effective_budget`. This means:
- For `carry_over_unused=true`: If spend is 10000 and period_allocation×days_elapsed is 20000, effective_budget = 10000 (carry-over applied at query time). An 80% alert threshold uses `budget_limit` directly — e.g., if `budget_limit=30000`, the 80% threshold fires at `current_spend >= 24000`. This is evaluated against `current_spend` (the actual recorded spend), not the effective remaining budget.
- For `carry_over_unused=false`: No carry-over applies; `effective_budget = budget_limit` at period start. The alert trigger behaves identically to the budget enforcement check.

**Integer division note:** The trigger formula uses integer division (truncates toward zero). With typical μunit budget limits and threshold_percent values (multiples of 50), truncation is ≤1 μunit — well within tolerance. The trigger fires at the truncated integer result (current_spend ≥ result).

**Threshold validation:** Threshold values must be integers between 1 and 100 (inclusive). Values outside this range are ignored by the alert handler. If `budget_alert_thresholds` is empty, no alerts fire. If `budget_limit == 0` (unlimited key), no alerts fire regardless of threshold configuration.

**Config changes mid-period:** If `budget_alert_thresholds` is changed mid-period (e.g., from `[50, 80]` to `[50, 80, 90]`), the new thresholds apply immediately. The `budget_alert_log` already has fired entries for `[50, 80]` in the current period — those will not re-fire regardless of the config change. New thresholds (`90`) can fire immediately if spend is already above that threshold. There is no automatic clearing of alert state on config change — `budget_alert_log` rows persist for audit.

**Threshold firing semantics:** Each threshold fires at most ONCE per billing period per key. Once a threshold (e.g., 80%) has fired, it will not fire again until the next billing period begins (via F2 auto-reset or calendar month boundary). If spend dips below the threshold and rises above again within the same billing period, no additional alert fires.

**Multi-threshold crossing:** If a single request causes spend to cross multiple thresholds (e.g., jumping from 40% to 95%), ALL crossed thresholds fire. The alert handler evaluates all configured thresholds post-spend and fires webhooks for each threshold that is newly crossed. Each webhook is independent with its own `threshold` value. This means one request can trigger multiple webhooks simultaneously (e.g., both 80% and 90% alerts fire for the same request).

**Re-arm when billing_period ≠ auto_reset_period:** The billing period and the auto-reset period are independent concepts:

- **billing_period:** Calendar month boundary (always monthly, fixed at RFC-0904 adoption) — determines when F1 alert thresholds re-arm
- **auto_reset_period:** Configurable (daily/weekly/monthly) via `auto_reset_period` config — determines when `key_spend.current_spend` resets to zero

If `auto_reset_period` is shorter than the billing period (e.g., daily resets with a monthly billing period), the F1 alert state does NOT re-arm at each daily reset — it re-arms only at the calendar month boundary. Conversely, if `auto_reset_period` is longer (e.g., monthly reset), the first F1 alert re-arm occurs at the first monthly reset after the key was created, not at the calendar month boundary.

In all cases: **F1 re-arm is driven by billing_period, not auto_reset_period.**

**Alert state tracking:** Fired thresholds are tracked in a `budget_alert_log` table:

```sql
CREATE TABLE budget_alert_log (
    key_id      BLOB(16)     NOT NULL,  -- Raw UUID bytes
    threshold   INTEGER     NOT NULL CHECK (threshold >= 1 AND threshold <= 100),  -- 1-100%
    fired_at    INTEGER     NOT NULL,  -- Unix epoch seconds (debug-only — not returned in API)
    period_start INTEGER    NOT NULL,  -- Unix epoch of billing period start
    PRIMARY KEY (key_id, threshold, period_start)
);
CREATE INDEX idx_budget_alert_log_key_id ON budget_alert_log(key_id);
```

**`fired_at` is debug-only storage:** The field is stored for forensic reconstruction (e.g., "when did the 80% alert fire?") but is never returned in any Admin API response. It consumes storage and index space but provides no API value. If storage efficiency is critical, implementations MAY omit this column — `period_start` is sufficient to identify the billing period, and the absence of an entry in `budget_alert_log` for a given `(key_id, threshold, period_start)` means the alert has not fired.

**`alert_id` removed:** The primary key is the composite `(key_id, threshold, period_start)` — not a separate auto-increment `alert_id`. This ensures each `(key_id, threshold, period_start)` combination is unique. If a second alert fires for the same (key_id, threshold) in the same period, the UNIQUE constraint prevents the duplicate insert (idempotent — treat as no-op).


**`period_start` computation:** `period_start` is the Unix epoch of the start of the current billing period. For monthly billing (default), this is `00:00 UTC on the 1st of the current calendar month`. The handler computes this as:

```rust
let now = Utc::now();
let period_start = now.date().with_day(1).unwrap().and_hms_opt(0, 0, 0).unwrap().timestamp();
```

For weekly billing: start of current week (Monday 00:00 UTC). For daily: start of current day (00:00 UTC).

**Alert delivery:**

- `POST /admin/budget/alert/callback` — webhook to external system (Slack, email, PagerDuty)
- Alert payload:
  ```json
  {
    "event_type": "budget_threshold",
    "key_id": "...",
    "team_id": "...", // null if no team
    "budget_limit": 1_000_000_000,
    "current_spend": 850_000_000,
    "threshold": 80,
    "percent_used": 8500,
    "timestamp": 1745280000
  }
  ```

**percent_used format:** Integer hundredths (8500 = 85.00%) to avoid floating-point inconsistency.

**Webhook Configuration:**

- The webhook URL and secret are configured per-key in `api_keys.metadata` as:
  ```json
  {
    "budget_alert_callback": "https://example.com/webhook",
    "budget_alert_secret": "<hex-encoded-secret>"
  }
  ```
- **Secret format:** Hex-encoded bytes, minimum 32 bytes (64 hex chars). Generated per-key; share the secret with the receiver out-of-band.
- **Signature algorithm:** `HMAC-SHA256(secret_bytes, timestamp + "." + payload_utf8_bytes)` → raw 32 bytes → hex-encoded. The timestamp is included in the signed data to prevent replay attacks. Receivers MUST verify both the timestamp (reject if >5 min old) AND the signature.
- **Headers sent with every webhook POST:**
  - `Content-Type: application/json`
  - `X-Webhook-Timestamp: <unix_epoch_seconds>` — receivers SHOULD reject if timestamp is more than 5 minutes from current time (replay protection)
  - `X-Webhook-Signature: sha256=<hex_encoded_signature>` — constant-time compare recommended
- **TLS:** Webhook URLs must use HTTPS
- **Retry:** On delivery failure (non-2xx response or timeout), the router retries up to 3 times with fixed-interval backoff (1s, 5s, 30s). After all retries fail, the alert is dropped and an error is logged
- **Timeout:** 10 second timeout per delivery attempt

**Delivery guarantee:** Alert delivery is **at-least-once**: the router retries up to 3 times with fixed-interval backoff until the receiver returns a 2xx response. If all retries fail, the alert is dropped and an error is logged. Receivers SHOULD handle idempotent delivery — ignore duplicate alerts for the same `(key_id, threshold, period_start)` combination.

**Configuration:** Threshold percentages are stored per-key in `api_keys.metadata` as JSON:

```json
{ "budget_alert_thresholds": [50, 80, 90] }
```

**HMAC secret rotation:** Secret rotation is out-of-band — generate a new secret, update the receiver first (to accept both old and new signatures), then update the key's metadata. During the transition window, the receiver should accept signatures from either secret. After the transition window (recommended: 24 hours), the old secret can be discarded. Storing secrets in `api_keys.metadata` is acceptable for development; production deployments should use a secrets manager and reference secrets by key identifier rather than embedding the secret value in metadata.

#### Budget Auto-Reset (F2)

Budget auto-reset restores spend counters on a schedule:

```
Reset intervals (configurable per key/team):
  - daily:    Reset at 00:00 UTC each day
  - weekly:   Reset at 00:00 UTC Monday
  - monthly:  Reset at 00:00 UTC first day of month
```

**Mechanism:** The reset is triggered by an external scheduler (e.g., cron, Kubernetes CronJob) calling an internal admin endpoint `POST /admin/internal/budget/reset` at each reset boundary. The router does not ship an internal scheduler — deployers must integrate with their existing job scheduling infrastructure.

The reset handler executes:

1. Reads all `api_keys` with `auto_reset_period` set
2. For each key, acquires `FOR UPDATE` lock on the key row, sets `key_spend.total_spend = 0` and `key_spend.window_start = now` (resetting both the spend counter and the period boundary marker), then releases lock
3. Logs reset event to `budget_reset_log` table (not `spend_ledger`) with metadata: key_id, team_id, reset_time, period_type

**Thundering herd mitigation:** On large deployments (10,000+ keys with auto_reset_period), processing all keys sequentially in a single handler invocation can cause database connection exhaustion and increased latency for other queries. The handler SHOULD process keys in batches (recommended: 100-500 keys per batch) with a brief sleep between batches (recommended: 10-50ms stagger). A timeout per key (recommended: 100ms max) prevents a single slow key from blocking the entire batch. Deployers SHOULD configure their scheduler to invoke the reset endpoint at slightly randomized times (e.g., ±30s jitter) to spread load across the deployment.

**Race condition mitigation:** Each key row is locked during reset to prevent concurrent `record_spend_ledger` calls from recording spend against the wrong period. Without `FOR UPDATE`, a spend event recorded between the key being read and reset could be lost (recorded against the new period's counter after reset). The lock ensures atomicity: either the reset or the spend recording happens first.

**Deleted keys:** If a key has `auto_reset_period` set but is deleted before reset processing, the handler skips the key gracefully (no error — the key no longer exists in `api_keys`).

**`budget_reset_log` table schema:**

```sql
CREATE TABLE budget_reset_log (
    reset_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    key_id        BLOB(16)     NOT NULL,  -- Raw UUID bytes
    team_id       BLOB(16),                -- null if key has no team
    reset_time    INTEGER     NOT NULL,  -- Unix epoch seconds
    period_type   TEXT        NOT NULL,  -- 'daily' | 'weekly' | 'monthly'
    reset_trigger TEXT        NOT NULL,  -- 'scheduled' | 'manual' — purely informational (audit/debug), does not affect reset behavior
    created_at    INTEGER     NOT NULL DEFAULT UNIXEPOCH()
);

CREATE INDEX idx_budget_reset_log_key_id ON budget_reset_log(key_id);
CREATE INDEX idx_budget_reset_log_team_id ON budget_reset_log(team_id) WHERE team_id IS NOT NULL;
```

**Period allocation:** `period_allocation = budget_limit / days_in_period` where `days_in_period` is: 1 for daily, 7 for weekly, 30 for monthly. At reset, `key_spend` is ALWAYS set to zero (not subtracted).

**Effective budget formula (applied at spend query time):**

| `carry_over_unused` | Formula | Description |
|---------------------|---------|-------------|
| `true` (default) | `effective_budget = key_spend + (period_allocation × days_elapsed)` | Unused allocation carried forward |
| `false` | `effective_budget = budget_limit` | No carry-over; full budget at each period start |

**Example (monthly, carry_over_unused=true, budget_limit=30000, no spend in first 15 days):** At day 15, key_spend=0. Effective budget = 0 + (30000/30 × 15) = 15000. After 10000 spend, effective remaining = 5000. At month reset (day 30), key_spend is set to 0 and effective_budget resets to full 30000.

**Example (monthly, carry_over_unused=false, budget_limit=30000):** At day 15, key_spend=5000 (from earlier spend). At period start, key_spend is set to 0 and effective_budget = 30000 (no carry-over). Only 30000 is available, not 35000.

**Example (weekly, carry_over_unused=true, budget_limit=30000, no spend in first 3 days):** At day 3, key_spend=0. Effective budget = 0 + (30000/7 × 3) = 12857. After 5000 spend, effective remaining = 7857. At weekly reset (day 7), key_spend is set to 0 and effective_budget resets to full 30000.

**Note:** Auto-reset does NOT write to `spend_ledger` — reset events are logged to a separate `budget_reset_log` table to avoid violating the `CHECK (token_source IN ('provider_usage', 'canonical_tokenizer'))` constraint defined in RFC-0909 §SpendEvent. Budget reset is an administrative action, not a spend event.

**Spend history:** F2 reset does not delete `spend_ledger` records — they are immutable per RFC-0909. Historical period spend can be reconstructed from `spend_ledger` using `timestamp` filters for the billing period boundaries. The `budget_reset_log.reset_time` provides the reset timestamp for computing period boundaries.

**Configuration:** Reset period and carry-over behavior stored in `api_keys.metadata`:

```json
{ "auto_reset_period": "monthly", "carry_over_unused": true } // "daily" | "weekly" | "monthly" | null (no auto-reset); carry_over_unused: boolean (default: true)
```

**`carry_over_unused` field:** When `true` (default), unused budget allocation is carried forward at each reset (applied at query time as `effective_budget = key_spend + (period_allocation × days_elapsed)`). When `false`, unused budget is discarded at reset — `key_spend` is set to zero and `effective_budget = budget_limit` at the start of each period. If the field is absent from `api_keys.metadata`, it defaults to `true`.

**Changing `auto_reset_period` mid-period:** Changes take effect at the next scheduled reset boundary. The current period's reset schedule is determined by the config at the start of the period. Mid-period changes do not trigger an immediate reset or affect the current period's schedule.

**Team auto_reset_period inheritance:** The `auto_reset_period` and `carry_over_unused` settings are per-key configuration stored in `api_keys.metadata`. Teams do NOT propagate these settings to member keys — each key has its own independent config. A team does not have a team-level `auto_reset_period`; only individual keys do. If a key has no `auto_reset_period` in its metadata, it is not reset by the F2 handler (regardless of team membership).

**Internal Reset Endpoint (for external schedulers):**

```
POST /admin/internal/budget/reset
  → 200 OK {
      reset_count: i32,    // number of keys reset
      errors: [{key_id: String, error: String}, ...]  // empty array if all succeeded
    }
  → 500 Internal Server Error {"error": "Storage", "detail": "..."}
```

**Request body (optional):**

- `{"period_type": "daily", "trigger": "scheduled"}` — external scheduler (cron, Kubernetes CronJob) sets `trigger: "scheduled"`
- `{"period_type": "daily", "trigger": "manual"}` — manual admin call sets `trigger: "manual"`

If `trigger` is omitted, defaults to `"scheduled"` for backward compatibility.

**Note:** `period_type` in `budget_reset_log` reflects the configured `auto_reset_period` at execution time, not at trigger time. If `auto_reset_period` changes between trigger and execution, the new value is logged.

#### OCTO-W Integration (F3)

Budget enforcement via OCTO-W token balance (RFC-0900):

**Concept:** When a key's OCTO-W balance is insufficient to cover estimated cost, the request is rejected before provider call. **F3 is a standalone enforcement mode** — when F3 is enabled for a key, `record_spend_ledger` is NOT called (OCTO-W balance IS the budget enforcement, replacing `budget_limit`).

**F3 enablement:** F3 is enabled per-key via `api_keys.metadata`:
```json
{ "octo_w_enforcement": true }
```
When `octo_w_enforcement` is `true`, the F3 path is used for budget enforcement. When absent or `false`, standard `budget_limit` enforcement applies. F3 can be enabled alongside F1 (soft pre-check) or standalone.

**Relationship with F1 (Soft Pre-Check):** When both F3 and F1 are enabled: pre-request flow is (1) `check_budget` soft check (budget_limit), then (2) `get_octo_w_balance` check. Both must pass for the request to proceed. Note: the `check_budget` soft check is informational only when F3 is active — it does not block requests. The OCTO-W balance check is the authoritative pre-check.

**OCTO-W Interface (minimum spec required for F3 implementation):**

```rust
/// Get OCTO-W balance for a key.
/// Returns balance in μunits.
/// Returns KeyError::KeyNotFound if key doesn't exist.
/// Returns KeyError::OctoWNotEnabled if key exists but OCTO-W is not configured for this key.
pub fn get_octo_w_balance(key_id: &str) -> Result<u64, KeyError>;

/// Deduct cost_amount from OCTO-W balance atomically.
/// Implementation: SELECT FOR UPDATE on key row, then atomic pre-check + deduct in single transaction.
/// TOCTOU prevention: balance check and deduct are in the same locked transaction — no separate
/// pre-check call needed. The FOR UPDATE lock is held for the duration of the atomic operation.
pub fn deduct_octo_w(key_id: &str, cost_amount: u64) -> Result<u64, KeyError> {
    // Returns Ok(new_balance) on success.
    // Returns Err(KeyError::InsufficientBalance { available: u64 }) if balance < cost_amount.
    // Returns Err(KeyError::OctoWNotEnabled) if OCTO-W not configured for this key.
}
```

**Flow (F3 standalone mode):**

**Prerequisite: model must be known.** F3 pre-check requires `estimated_cost` computed from the model-specific pricing. This means the F3 check happens AFTER model selection. In routing strategies where model is selected based on cost (least-cost routing), the model IS known at selection time — so F3 pre-check can proceed. In strategies where model is selected after budget pre-check passes, the F3 check happens after model selection but before the provider call.

1. **Model selected** — routing strategy picks a specific model (e.g., `gpt-4o`)
2. **Estimate cost** using `max_tokens × pricing.prompt_cost_per_1k / 1000` for the selected model (conservative overestimate). For models with known context windows, use the provider's max token limit as the overestimate — same ceiling formula used for the F1 soft pre-check. Compare `get_octo_w_balance(key_id)` against this `estimated_cost`
3. If `get_octo_w_balance` returns `KeyError::KeyNotFound` (key doesn't exist), treat as insufficient balance — reject with `BudgetError::InsufficientBalance { key_id, available: 0, estimated }`
4. If `balance < estimated_cost`, reject with `BudgetError::InsufficientBalance { key_id, available, estimated }`
5. After successful provider request, call `deduct_octo_w(key_id, cost_amount)`
6. If deduct fails after provider success (edge case):
   - Log error to application error log with key_id, cost_amount, failure reason
   - Send alert via `POST /admin/budget/alert/callback` with `event_type: "octo_w_deduction_failed"`:
     ```json
     {
       "event_type": "octo_w_deduction_failed",
       "key_id": "...",
       "team_id": "...", // null if no team
       "cost_amount": 15000, // μunits that failed to deduct
       "reason": "insufficient_balance | concurrent_update | storage_error",
       "provider_response_recorded": true, // provider call succeeded, cost incurred
       "timestamp": 1745280000
     }
     ```
   - **API response to client:** The client request returns `200 OK` with the normal provider response. The OCTO-W deduction failure is an internal reconciliation issue — the provider charge is already irreversible, so failing the client request would not recover the funds. The client sees success; operations team must manually reconcile.
   - **Manual reconciliation required** — the provider charge is irreversible but OCTO-W balance was not updated. Operations team must manually reconcile.

**Lock release timing (R14-11):** Both `record_spend_ledger_with_team` and `deduct_octo_w` hold their FOR UPDATE locks for the **entire transaction duration** — from lock acquisition through commit or rollback. For `record_spend_ledger_with_team`: the team lock is acquired first, the key lock second, both are held through the INSERT, and both are released on transaction commit. For `deduct_octo_w`: the key lock is acquired, balance check and deduct happen atomically in the same locked context, then the lock is released on commit. There is no window between team-check and key-check where locks are released.

**Note:** When F3 is active, `record_spend_ledger` is NOT called — the OCTO-W deduction IS the budget enforcement. Do NOT call both `deduct_octo_w` AND `record_spend_ledger` for the same request.

## Key Files to Modify

| File                                         | Change                                                                                     |
| -------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `crates/quota-router-core/src/storage.rs`    | Confirm `record_spend_ledger` and `record_spend_ledger_with_team` cover atomic enforcement |
| `crates/quota-router-core/src/middleware.rs` | Confirm `check_budget(&ApiKey)` as soft pre-check (line 106)                               |

## Future Work

No future work items remain — F1 (Budget alerts), F2 (Budget auto-reset), and F3 (OCTO-W integration) are fully specified in Phase 3 above.

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

1. Check budget (no provider round-trip)
2. Fail immediately with 402 if over budget

The soft check is non-locking — it's possible (though unlikely) that another concurrent request uses the last budget. The atomic `record_spend_atomic()` is the authoritative check.

## Version History

| Version | Date       | Changes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.18    | 2026-04-22 | Round 18: fix monthly period derivation formula (V1 — use month+1/year wrap instead of +32d); remove BudgetError::TeamRequired (deferred without spec, V2); update key_spend.key_id to BLOB(16) per RFC-0903-C1 (V3); mark Phase 3 checklist [x] (V4); add team_id to GET /admin/budget/team/{team_id} response (V6); clarify fired[].period_start vs outer period_start (V7); verify Phase 2 D1 items resolved, confirm no remaining future work (V8); remove redundant F3+F1 coincidence paragraph (V10) |
| 1.17    | 2026-04-22 | Round 17: fix team endpoint percent_used unguarded (U1); specify F1 synchronous alert evaluation + delivery failure behavior (U2); specify Admin API auth mechanism (Bearer token, validation, provisioning, rate limiting) (U3); document fired_at debug-only storage (U4); document reset_trigger informational nature (U5); specify F3 deduct failure API response returns 200 (U6); add team all-unlimited case (budget_limit==0) semantics (U7); add get_team_spend period derivation formula (U8); specify F1 multi-threshold crossing fires all crossed thresholds (U9); note RFC-0904 Draft dependency in RFC-0917 integration (U10); clarify F2 reset does not touch spend_ledger (U11); clarify carry_over_unused defaults when absent (U12) |
| 1.16    | 2026-04-22 | Round 15: fix /keys endpoint budget_limit/current_spend u64 (was i64); fix alerts endpoint budget_limit u64; remove fired_at from fired array (S3); add get_team_spend backward compat note (S4); add key_spend schema (S5); add pricing_hash test vector reference (S6); add current_spend source to F1 spec (S7); clarify F2 reset resets total_spend AND window_start (S8); add check_budget error type distinction in Budget Pre-Check section (S9); guard percent_used formula with budget_limit>0 (S10) |
| 1.15    | 2026-04-22 | Round 14: R14-01 percent_used overflow (saturating_mul); R14-02 Admin API u64; R14-03 get_team_spend period filter; R14-04 F3+F1 coincidence; R14-05 F2 thundering herd mitigation; R14-06 team auto_reset_period inheritance; R14-07 F1 alert uses budget_limit not effective_budget; R14-08 check_budget unlimited key note; R14-09 updated_at→created_at; R14-10 KeyError vs BudgetError types; R14-11 lock release timing; R14-12 fired_at redundant (removed from payload) |
| 1.14    | 2026-04-22 | Round 13: G4 latency claim removed; budget_limit==0 simplified; budget_alert_log threshold CHECK; LiteLLM F1/F2 mixup fixed; get_team_spend u64 return; carry_over_unused=false table; F2 spend history (spend_ledger immutable); /keys pagination edge cases; F1 alert state endpoint; F3 model prerequisite + enablement; HMAC secret rotation; F1 config-mid-period note; empty-team case documented |
| 1.13    | 2026-04-22 | Round 12 adversarial review: fix budget_alert_log PK (composite PK on key_id+threshold+period_start, remove auto-increment alert_id); fix idx_budget_reset_log_team_id partial index on nullable column; add weekly period_allocation example; add KeyNotFound→InsufficientBalance conversion in F3 flow; add pagination to /keys endpoint (offset/limit/total); document KeyError→BudgetError conversion path for BudgetExceeded/TeamBudgetExceeded; remove stale F1/F2/F3 from Future Work (fully specced in Phase 3)                                                                                                                                                                               |
| 1.12    | 2026-04-22 | Round 11 adversarial review: add carry_over_unused config field; specify at-least-once webhook delivery guarantee + idempotent receiver guidance; add include_revoked filter to team endpoint; add F1 threshold validation (1-100 range, empty array handling); document period_start computation formula; add deleted-key skip note to reset handler; clarify team current_spend data source (key_spend table); specify Admin API auth (Bearer token, 401/403); add F3 estimated_cost formula (max_tokens ceiling); add pricing_hash stability note; check Phase 2 items as spec-complete                                                                                                                                                                                         |
| 1.11    | 2026-04-22 | Round 10 adversarial review: add budget_alert_log table for F1 threshold state tracking; fix F1 alert trigger condition (budget_limit > 0); fix HMAC signature to include timestamp (replay protection); fix updated_at null semantics; add period_type execution-time note; add get_octo_w_balance OctoWNotEnabled error; fix deduct_octo_w TOCTOU (FOR UPDATE in same transaction); fix key_count (active + revoked, deleted excluded); clarify team percent_used budget_limit semantics; add F1 re-arm when billing_period ≠ auto_reset_period; rename exponential backoff to fixed-interval backoff; add budget_limit==0 enforcement path (check_budget short-circuit, record_spend_ledger condition)                                                                          |
| 1.10    | 2026-04-22 | Round 9 adversarial review: fix F3 OCTO-W as standalone mode (replaces budget_limit, not supplements); fix carry_over_unused semantics (carry-over applied at query time, not reset); add TOCTOU mitigation with FOR UPDATE lock in reset; fix HMAC secret config (hex-encoded per-key); add HMAC algorithm (SHA256, replay protection via timestamp header, timing-safe compare); add percent_used null case for unlimited budget; add F1 threshold once-per-period semantics; add internal reset endpoint spec; add billing period first-period alignment; add pricing_hash reference to RFC-0910; add reset_trigger field; add include_revoked filter to /keys; add team zero-keys 200 vs 404 clarification; add team remaining per-key budget note; add orphaned team row note |
| 1.9     | 2026-04-22 | Round 8 adversarial review: declare REST/HTTPS protocol; specify F2 external scheduler mechanism + internal reset endpoint; define budget_reset_log table schema; add webhook auth (HMAC-SHA256), TLS requirement, retry (3x exponential backoff), 10s timeout; define billing period; add compute_cost test vectors; add key_id UUID format requirement with 400 on invalid; add team lock granularity bottleneck note; add period allocation formula; add empty model name validation note                                                                                                                                                                                                                                                                                       |
| 1.8     | 2026-04-22 | Round 7 adversarial review: add Admin API HTTP status codes, error response format, auth requirement; add remaining negative semantics; add updated_at team aggregation note; add revoked keys in team spend note; add BudgetError variant reachability doc comments; add F3 OCTO-W failure alert payload spec                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| 1.7     | 2026-04-22 | Round 6 adversarial review: clarify F1 integer division edge case; fix F2 token_source CHECK constraint violation (use separate budget_reset_log table not spend_ledger); specify F3 OCTO-W interface (get_octo_w_balance, deduct_octo_w minimum spec)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 1.6     | 2026-04-22 | Round 5: add BudgetError::InsufficientBalance for F3; replace f64 percent_used with u64 hundredths; replace todo!() in get_team_spend with SQL JOIN; add token_source CHECK validation note; add RFC-0917 integration section; update Phase 2 note (E1-E5) |
| 1.5     | 2026-04-22 | Round 4 adversarial review: D1-D10 fixes (Phase 2/3 specs added, get_spend takes &str, admin API endpoints, F1/F2/F3 mechanism specs, TeamBudgetExceeded documented, builtins documented, idempotency documented)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| 1.4     | 2026-04-22 | Round 3 adversarial review: archive Planned RFC-0904 placeholder; fix Phase 1 checklist; fix Key Files section; document record_spend (no budget check); remove get_team_spend; fix get_current_spend return type; add C1-C9 review section                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 1.3     | 2026-04-22 | Round 2 adversarial review: fix B1-B12 (remove check_budget_soft_limit, replace InsufficientBudget, remove get_team_spend, clarify record_spend dispatch, fix KeySpend unit, document soft check staleness, update Phase 1 checklist, fix CostError→BudgetError)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 1.2     | 2026-04-22 | Round 1 fixes continued (A8-A12): add per-model ceiling cost table; standardize budget_limit resolution; clarify idempotency via UNIQUE constraint; specify timestamp semantics; define KeyError/BudgetError separation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 1.1     | 2026-04-22 | Round 1 adversarial review: fix A1-A7 (critical type errors, nonexistent methods, trait mismatches)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| 1.0     | 2026-04-22 | Initial draft                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |

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

| Location                                | Type                                        |
| --------------------------------------- | ------------------------------------------- |
| ApiKey struct (models.rs)               | `i64`                                       |
| ApiKey struct (RFC-0903 line 163)       | `u64`                                       |
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

## Round 4 Adversarial Review

### D1: Phase 2 Items Have No Specification

**Severity:** High (Missing Specification)

**Finding:** Phase 2 listed `get_team_spend()` and admin API endpoints with no specification.

**Resolution:** Added minimal spec for `get_team_spend` (SQL JOIN defined) and full admin API endpoint specification (GET endpoints for key/team budget status).

---

### D2: Phase 3 Future Work (F1, F2, F3) Has Zero Specification

**Severity:** Medium (Missing Specification)

**Finding:** F1/F2/F3 were mentioned by name only with no mechanism.

**Resolution:** Added F1 (budget alerts with webhook + thresholds), F2 (auto-reset via background job with period config), and F3 (OCTO-W integration per RFC-0900 dependency).

---

### D3: `BudgetError::TeamBudgetExceeded` Never Returned

**Severity:** Low (Dead Code)

**Finding:** `BudgetError::TeamBudgetExceeded` is defined but never returned by any documented function. `record_spend_ledger_with_team` returns `KeyError::TeamBudgetExceeded`.

**Resolution:** Documented the conversion path: `KeyError::TeamBudgetExceeded` → `BudgetError::TeamBudgetExceeded` via `From` impl for external API surfacing.

---

### D4: `BudgetStorage.get_spend` Takes `&Uuid` But Actual API Uses `&str`

**Severity:** High (Trait/Signature Mismatch)

**Finding:** The trait declared `get_spend(&self, key_id: &Uuid)` but the actual `KeyStorage.get_spend` takes `&str`.

**Resolution:** Changed trait signature to `get_spend(&self, key_id: &str)` to match the actual storage API.

---

### D5: Admin API Endpoints Listed But Never Specified

**Severity:** Medium (Missing Specification)

**Finding:** Phase 2 said "Add admin API endpoints" but no URLs, methods, or response shapes were defined.

**Resolution:** Added full GET /admin/budget/key/{key_id}, GET /admin/budget/team/{team_id}, GET /admin/budget/team/{team_id}/keys endpoint specifications.

---

### D6: `record_spend` vs `record_spend_ledger` Interaction Undocumented

**Severity:** Medium (Logic Gap)

**Finding:** Both functions write to `key_spend` but one bypasses budget check. The interaction was unclear.

**Resolution:** Documented that both write to `key_spend` (for soft pre-check reads) but only `record_spend_ledger` writes to `spend_ledger` (immutable audit log).

---

### D7: `get_pricing` ModelNotFound Unreachable With Builtins

**Severity:** Low (Dead Code Path)

**Finding:** With `new_with_builtins()` loading OpenAI/Anthropic models at startup, `ModelNotFound` seemed unreachable.

**Resolution:** Documented the builtin model set and clarified that `ModelNotFound` occurs for models not in the built-in set (custom providers, dynamic registration).

---

### D8: Duplicate event_id Returns Ok(()) — Silent Idempotency

**Severity:** Low (Behavior Clarification)

**Finding:** A10 resolution said "callers should treat duplicate as success" but didn't document the implementation behavior.

**Resolution:** Documented that `record_spend_ledger` returns `Ok(())` on duplicate event_id detection (idempotent replay).

---

### D9: Stale "Still Uses TEXT" Comment in storage.rs

**Severity:** Low (Stale Comment)

**Finding:** storage.rs line 733-734 says `teams table still uses TEXT for team_id (migrated separately)`. Per RFC-0903-C1, teams.team_id should be BLOB(16). The comment may be stale.

**Resolution:** Verified the comment reflects actual state — pending migration in RFC-0903-C2. No change to RFC needed; the comment is in implementation.

---

### D10: Phase 1 Unit Tests Verified

**Severity:** Low (Verification)

**Finding:** Phase 1 checklist said unit tests for cost calculation exist.

**Resolution:** Confirmed `compute_cost_tests` module in `keys/mod.rs` lines 1071-1130 with test vectors from RFC-0909.

---

## Issues Summary

| ID     | Severity | Issue                                                                                    | Status                                                                                    |
| ------ | -------- | ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | --------- |
| A1     | Critical | `record_spend_atomic` references `event.budget_limit` which doesn't exist                | Fixed                                                                                     |
| A2     | Critical | `insert_spend_event` doesn't exist in KeyStorage trait                                   | Fixed                                                                                     |
| A3     | Critical | `TeamSpend` type referenced but never defined                                            | Fixed                                                                                     |
| A4     | Critical | `record_spend_with_team` signature differs from existing `record_spend_ledger_with_team` | Fixed                                                                                     |
| A5     | High     | `get_team_spend` aggregate not defined                                                   | Fixed                                                                                     |
| A6     | High     | `check_budget_soft_limit` uses nonexistent `get_key_budget_limit`                        | Fixed                                                                                     |
| A7     | High     | Existing `check_budget` already does soft pre-check                                      | Fixed                                                                                     |
| A8     | Medium   | `estimated_cost` guidance vague — could cause false negatives                            | Fixed                                                                                     |
| A9     | Medium   | budget_limit type inconsistent (u64 vs i64)                                              | Fixed                                                                                     |
| A10    | Medium   | Idempotency not addressed — duplicate event_id could double-record                       | Fixed                                                                                     |
| A11    | Medium   | SpendEvent.timestamp semantics ambiguous                                                 | Fixed                                                                                     |
| A12    | Low      | `KeyError` vs `BudgetError` — two error types for same domain                            | Fixed                                                                                     |
| B1     | Critical | `check_budget_soft_limit` calls nonexistent `get_key_budget_limit`                       | Fixed                                                                                     |
| B2     | Critical | `BudgetError::InsufficientBudget` variant does not exist                                 | Fixed                                                                                     |
| B3     | High     | `get_team_spend` declared but no implementation exists                                   | Fixed                                                                                     |
| B4     | High     | `record_spend_atomic` delegation ambiguity                                               | Fixed                                                                                     |
| B5     | High     | `KeySpend.total_spend` in cents vs `cost_amount` in μunits                               | Fixed                                                                                     |
| B6     | Medium   | Soft check + atomic record is non-atomic                                                 | Fixed                                                                                     |
| B7     | Medium   | Implementation Phase 1 checklist references removed methods                              | Fixed                                                                                     |
| B8     | Medium   | `record_spend_with_team` takes `&str` but DB is `BLOB(16)`                               | Fixed                                                                                     |
| B9     | Medium   | `compute_event_id` excludes timestamp — determinism correct                              | Fixed                                                                                     |
| B10    | Low      | `CostError` referenced but never defined                                                 | Fixed                                                                                     |
| B11    | Low      | `BudgetError` Uuid vs DB `BLOB(16)` mapping not documented                               | Fixed                                                                                     |
| B12    | Low      | G4 `<5ms` metric not specified or verified                                               | Fixed                                                                                     |
| C1     | Critical | Two RFC-0904 files with same number, conflicting designs                                 | Fixed (archived Planned placeholder)                                                      |
| C2     | Medium   | Phase 1 checklist references resolved `CostError` item                                   | Fixed                                                                                     |
| C3     | Medium   | Key Files section references removed `check_budget_soft_limit`                           | Fixed                                                                                     |
| C4     | Medium   | `record_spend` (no budget check) exists but undocumented                                 | Fixed                                                                                     |
| C5     | Low      | `get_team_spend` function calls nonexistent storage method                               | Fixed                                                                                     |
| C6     | Low      | `check_budget` returns `KeyError` but RFC says `BudgetError` for cost ops                | Documented                                                                                |
| C7     | Low      | `get_current_spend` wraps `KeyError` → `BudgetError` without `From` impl                 | Fixed                                                                                     |
| C8     | Medium   | F2 Budget auto-reset has no RFC placeholder                                              | Flagged for planning                                                                      |
| C9     | Medium   | Lock ordering in `record_spend_ledger_with_team` not verified                            | Verified in storage.rs                                                                    |
| D1     | High     | Phase 2 items have no specification                                                      | Fixed (get_team_spend SQL + admin API spec)                                               |
| D2     | Medium   | Phase 3 F1/F2/F3 have no specification                                                   | Fixed (F1/F2/F3 specs added)                                                              |
| D3     | Low      | `BudgetError::TeamBudgetExceeded` never returned                                         | Documented (From impl path)                                                               |
| D4     | High     | `BudgetStorage.get_spend` takes `&Uuid` but actual API uses `&str`                       | Fixed (`&str`)                                                                            |
| D5     | Medium   | Admin API endpoints never specified                                                      | Fixed (spec added in Phase 2)                                                             |
| D6     | Medium   | `record_spend` vs `record_spend_ledger` interaction undocumented                         | Fixed (write paths clarified)                                                             |
| D7     | Low      | `get_pricing` ModelNotFound unreachable with builtins                                    | Fixed (builtin models documented)                                                         |
| D8     | Low      | Duplicate event_id silently returns Ok(())                                               | Fixed (idempotency documented)                                                            |
| D9     | Low      | Stale "still uses TEXT" comment in storage.rs                                            | Verified (pending C1/C2 migration)                                                        |
| D10    | Low      | Phase 1 unit test verification                                                           | Confirmed (compute_cost_tests in keys/mod.rs)                                             |
| E1     | High     | `BudgetError::InsufficientBalance` referenced in F3 but not defined                      | Fixed (added variant with key_id, available, estimated fields)                            |
| E2     | Medium   | Admin API `percent_used` uses f64 — floating-point inconsistency                         | Fixed (replaced with u64 hundredths, e.g., 8500 = 85.00%)                                 |
| E3     | High     | `get_team_spend` has `todo!()` placeholder — won't compile                               | Fixed (replaced with actual SQL JOIN implementation)                                      |
| E4     | Low      | Token source CHECK constraint validation not documented                                  | Fixed (added Token Source Validation section)                                             |
| E5     | Low      | RFC-0917 integration detail missing                                                      | Fixed (added Integration with RFC-0917 section)                                           |
| F1     | Medium   | F1 alert trigger formula uses ambiguous integer division                                 | Fixed (clarified truncation behavior and tolerance)                                       |
| F2     | Medium   | F2 budget reset logs to spend_ledger with invalid token_source 'budget_reset'            | Fixed (logs to separate budget_reset_log table instead)                                   |
| F3     | Low      | F3 OCTO-W interface underspecified (no balance/deduct signatures)                        | Fixed (added get_octo_w_balance, deduct_octo_w minimum spec)                              |
| G1     | Medium   | Admin API missing HTTP status codes, error response format, and auth requirements        | Fixed (added 200/404/500 specs, JSON error body format, auth note)                        |
| G2     | Low      | Admin API updated_at semantics for team aggregation unclear                              | Fixed (MAX of per-key updated_at; key_count includes revoked keys)                        |
| H1     | Low      | Revoked keys included in team spend — may be intentional but undocumented                | Fixed (added note: includes revoked keys for audit; filter revoked=0 for active-only)     |
| J1     | Low      | BudgetError::TeamRequired has no documented return path (dead variant)                   | Fixed (doc comment marks as reserved for future use)                                      |
| J2     | Low      | BudgetError::KeyNotFound and TeamNotFound unreachable via documented functions           | Fixed (doc comments added with actual return paths and notes)                             |
| J3     | Low      | BudgetError::CostOverflow theoretically unreachable with saturating arithmetic           | Fixed (doc comment notes it requires pathological pricing value)                          |
| M1     | Medium   | F3 OCTO-W deduction failure alert mechanism not specified                                | Fixed (added octo_w_deduction_failed event_type and JSON payload spec)                    |
| N1     | Low      | Precedence when both key AND team budget exceeded not documented                         | Fixed (added note: KeyBudgetExceeded returned when both exceeded, key checked first)      |
| Q1     | Low      | Admin API remaining can be negative if current_spend > budget_limit — undocumented       | Fixed (added semantics note: remaining may be negative, indicates budget exceeded)        |
| R8-01  | High     | Admin API protocol not declared (REST vs gRPC)                                           | Fixed (added REST/HTTPS protocol declaration)                                             |
| R8-02  | High     | F2 background job mechanism unspecified (who triggers reset?)                            | Fixed (external scheduler calls POST /admin/internal/budget/reset)                        |
| R8-03  | High     | budget_reset_log table schema never defined                                              | Fixed (added DDL with reset_id, key_id, team_id, reset_time, period_type, created_at)     |
| R8-04  | Medium   | Webhook URL authentication not specified                                                 | Fixed (HMAC-SHA256 signature, TLS required, 3x retry, 10s timeout)                        |
| R8-05  | Medium   | Billing period never defined                                                             | Fixed (default calendar month, configurable per key via metadata)                         |
| R8-06  | Medium   | compute_cost test vectors not present                                                    | Fixed (added 4 test vectors: 1500+500, 1000+1000, 0+500, 500+0)                           |
| R8-07  | Medium   | Webhook delivery guarantees unspecified (combined with R8-04)                            | Fixed (see R8-04 webhook configuration)                                                   |
| R8-08  | Medium   | API key path parameter format not specified                                              | Fixed (UUID lowercase hyphenated, 400 on invalid format)                                  |
| R8-09  | Medium   | Team lock granularity bottleneck concern (serializes team spend)                         | Fixed (added lock granularity note with throughput concern)                               |
| R8-10  | Medium   | Auto-reset period allocation calculation undefined                                       | Fixed (budget_limit / days_in_period with carry_over option)                              |
| R8-11  | Low      | Empty string model name handling                                                         | Fixed (added validation note: callers should validate; treat as ModelNotFound)            |
| R8-12  | Low      | updated_at team aggregation needs explicit SQL                                           | Fixed (added SQL aggregation note to updated_at field)                                    |
| R9-01  | High     | F2 auto-reset TOCTOU race condition (spend recorded between read and reset)              | Fixed (FOR UPDATE lock per key during reset processing)                                   |
| R9-02  | Medium   | auto_reset_period change mid-period undefined behavior                                   | Fixed (changes take effect at next scheduled boundary)                                    |
| R9-03  | High     | Webhook HMAC secret configuration unspecified (where stored, format)                     | Fixed (per-key metadata, hex-encoded, min 32 bytes)                                       |
| R9-04  | Medium   | HMAC verification algorithm missing (replay protection, timing-safe compare)             | Fixed (SHA256, X-Webhook-Timestamp, timing-safe compare)                                  |
| R9-05  | Medium   | /keys endpoint has no active/revoked filter                                              | Fixed (added ?include_revoked query param)                                                |
| R9-06  | Critical | F3 deduct failure — record_spend_ledger called or not? Budget split enforcement          | Fixed (F3 is standalone, replaces budget_limit, do NOT call both)                         |
| R9-07  | Medium   | billing_period first-period alignment undefined                                          | Fixed (first period starts at key creation, aligns to boundary)                           |
| R9-08  | Medium   | Team remaining misleading with per-key budget_limits                                     | Fixed (added note: reflects team-level only, use /keys for per-key)                       |
| R9-09  | Medium   | F1 threshold fires without debouncing or once-per-period semantics                       | Fixed (once per billing period, re-arms at reset)                                         |
| R9-10  | Low      | Team with zero keys — 404 or 200 key_count:0?                                            | Fixed (200 with key_count:0; 404 only if team_id doesn't exist)                           |
| R9-11  | Low      | record_spend_ledger_with_team orphaned team row (FK violation)                           | Fixed (returns TeamNotFound, notes orphaned key case)                                     |
| R9-12  | High     | carry_over_unused subtracts from key_spend accumulator — wrong semantics                 | Fixed (carry-over applied at query time, key_spend always set to zero at reset)           |
| R9-13  | Medium   | POST /admin/internal/budget/reset request/response shape undocumented                    | Fixed (added 200/500 responses, request body with trigger field)                          |
| R9-14  | High     | percent_used division by zero when budget_limit == 0                                     | Fixed (percent_used is null when budget_limit == 0)                                       |
| R9-15  | Low      | Manual vs scheduled reset — period_type doesn't distinguish                              | Fixed (added reset_trigger field: 'scheduled'                                             | 'manual') |
| R9-16  | Medium   | compute_pricing_hash algorithm not summarized in RFC-0904                                | Fixed (added RFC-0910 reference in Determinism Requirements)                              |
| R9-17  | Medium   | F3 OCTO-W pre-check vs F1 soft pre-check relationship unspecified                        | Fixed (documented standalone vs combined mode, both-must-pass flow)                       |
| R11-01 | Medium   | `carry_over_unused` never declared as a config field                                     | Fixed (added explicit field + boolean semantics to metadata config)                       |
| R11-02 | Medium   | F1 webhook delivery guarantee not specified (at-least-once vs at-most-once)              | Fixed (added at-least-once semantics note + idempotent receiver guidance)                 |
| R11-03 | Medium   | Team spend admin API doesn't support include_revoked filter                              | Fixed (added ?include_revoked=false query param to team endpoint)                         |
| R11-04 | Low      | F1 trigger doesn't validate threshold array is non-empty or values in range 1-100        | Fixed (added validation note + unlimited-key pass-through)                                |
| R11-05 | Low      | `period_start` for alert state tracking — how computed                                   | Fixed (documented period_start computation: start of billing period UTC)                  |
| R11-06 | Low      | F2 reset — no graceful handling of deleted keys with auto_reset_period set               | Fixed (added skip-on-delete note)                                                         |
| R11-07 | Medium   | Admin API team `current_spend` — from key_spend or spend_ledger?                         | Fixed (clarified data source: key_spend table, consistent with soft pre-check)            |
| R11-09 | Low      | F1 threshold values — no upper bound validation                                          | Fixed (added validation note for 1-100 range)                                             |
| R11-11 | Medium   | Admin API auth — "admin-level API key" not defined (no header/spec)                      | Fixed (added Bearer token auth spec, 401/403 error responses)                             |
| R11-12 | Medium   | F3 `estimated_cost` not defined — which formula?                                         | Fixed (specified pre-request estimate: max_tokens ceiling formula)                        |
| R11-13 | Low      | Phase 2 get_team_spend SQL spec'd but checklist unchecked                                | Fixed (checked Phase 2 items as spec-complete)                                            |
| R11-14 | Low      | `pricing_hash` cross-RFC stability risk                                                  | Fixed (added stability note: RFC-0910 must be stable, breaking changes require amendment) |
| R12-01 | High     | budget_alert_log UNIQUE on (key_id, threshold, period_start) with auto-increment alert_id allows duplicates | Fixed (composite PK on key_id+threshold+period_start, removed alert_id) |
| R12-02 | Low      | idx_budget_reset_log_team_id on nullable column — inefficient                             | Fixed (partial index WHERE team_id IS NOT NULL)                                             |
| R12-03 | Medium   | get_team_spend orphaned key case (JOIN on api_keys.team_id) not handled                   | Documented (orphaned key returns 0, team FK guarantees consistency)                     |
| R12-04 | Low      | BudgetError::Storage(String) not redacted in Admin API — may leak DB details            | Not changed (storage layer should redact before surfacing; RFC does not define redaction) |
| R12-05 | Low      | check_budget budget_limit==0 short-circuit comment vs implementation mismatch           | Not changed (RFC code example correct; existing middleware comment stale)                 |
| R12-06 | Low      | weekly period_allocation example missing                                                 | Fixed (added weekly example: day 3, budget 30000, effective=12857)                       |
| R12-07 | Low      | billing_period first-period alignment lacks timezone documentation                       | Not changed (already UTC-aligned; timezone implied by Unix epoch)                        |
| R12-08 | Low      | team_id nullable BLOB(16) index efficiency concern                                       | Not changed (index on nullable column acceptable for current scale)                       |
| R12-09 | High     | F3 get_octo_w_balance KeyNotFound vs BudgetError::InsufficientBalance inconsistency      | Fixed (added step 2: KeyNotFound→InsufficientBalance with available=0)                  |
| R12-10 | Medium   | /keys endpoint returns all keys — no pagination for large teams                         | Fixed (added offset/limit/total pagination params + response field)                       |
| R12-11 | Medium   | check_budget returns KeyError::BudgetExceeded but BudgetError::KeyBudgetExceeded defined | Fixed (documented KeyError→BudgetError conversion path)                                  |
| R12-12 | Low      | Future Work lists F1/F2/F3 despite Phase 3 fully specifying them                        | Fixed (Future Work section updated to show no remaining items)                            |
| R13-01 | Low      | G4 `<1ms` still unverified after B12 fix                                                | Fixed (removed specific latency claim, qualitative "storage-dependent")                     |
| R13-02 | Medium   | budget_limit==0 explanation self-contradictory (describes buggy then correct code)      | Fixed (simplified to show only correct implementation)                                      |
| R13-03 | Medium   | budget_alert_log threshold lacks CHECK constraint (1-100 enforced only at app layer)     | Fixed (added CHECK (threshold >= 1 AND threshold <= 100) to DDL)                           |
| R13-04 | Low      | LiteLLM table says "Future (F1)" for F2 budget reset                                     | Fixed (corrected to F2 auto-reset)                                                        |
| R13-05 | Medium   | get_team_spend returns i64 but spend is non-negative (u64 correct)                       | Fixed (return type changed to u64 with note)                                               |
| R13-06 | High     | carry_over_unused=false formula wrong (applied carry-over formula when no carry)          | Fixed (added explicit table: true=carry formula, false=budget_limit only)                  |
| R13-07 | Medium   | F2 reset destroys period spend history — audit gap                                       | Fixed (added spend_ledger is immutable + reset_time provides period boundary for queries)   |
| R13-08 | Low      | /keys pagination limit=0 edge case undefined                                             | Fixed (limit=0 returns empty array, offset>=total returns empty, limit max clamp noted)     |
| R13-09 | Low      | No admin endpoint to query active F1 alert state                                          | Fixed (added GET /admin/budget/key/{key_id}/alerts endpoint)                               |
| R13-10 | Medium   | F3 estimated_cost — model may not be known at pre-check time                              | Fixed (F3 flow now shows model selection prerequisite + post-model-positioned check)        |
| R13-11 | Low      | F3 OCTO-W enablement per-key — how configured?                                             | Fixed (added octo_w_enforcement metadata field + relationship with F1)                     |
| R13-12 | Low      | F1 webhook HMAC secret rotation unspecified                                              | Fixed (added rotation procedure + secrets manager note)                                  |
| R13-13 | Low      | F1 config change mid-period alert state unclear                                          | Fixed (added config-mid-period note: old entries persist, new thresholds can fire)           |
| R13-14 | Low      | get_team_spend empty team case not explicitly documented                                 | Fixed (added note: team exists but no keys → Ok(0), not TeamNotFound)                    |
| R14-01 | Medium   | percent_used overflow (u64 intermediate overflow on near-MAX current_spend)             | Fixed (saturating_mul instead of multiply)                                                |
| R14-02 | Low      | Admin API current_spend/budget_limit use i64 but spend is non-negative                   | Fixed (changed to u64)                                                                   |
| R14-03 | Medium   | get_team_spend has no billing period filter (aggregates ALL spend_ledger records)        | Fixed (added period_start/period_end filter)                                             |
| R14-04 | Low      | F3 deduct failure + F1 threshold coincidence not addressed                               | Fixed (added note: post-deduct F1 alert fires if threshold crossed; alert includes reason) |
| R14-05 | Medium   | F2 reset handler processes ALL auto_reset_period keys sequentially (thundering herd)      | Fixed (added batch size guidance + timeout per key + stagger hint)                       |
| R14-06 | Low      | Team auto_reset_period inheritance to member keys undocumented                           | Fixed (added note: teams don't inherit to keys; each key has its own config)             |
| R14-07 | Low      | F1 alert trigger uses budget_limit not effective_budget — needs explicit documentation    | Fixed (documented: alert trigger uses budget_limit, not effective_budget)                 |
| R14-08 | Low      | check_budget structurally unable to detect spend buildup on unlimited keys              | Fixed (added note: soft pre-check passes for unlimited keys; atomic enforcement used)     |
| R14-09 | Medium   | Admin API updated_at doesn't exist in spend_ledger                                      | Fixed (changed to created_at with SQL note)                                               |
| R14-10 | Low      | KeyError vs BudgetError type mismatch in Admin API                                      | Fixed (KeyError for storage, BudgetError for budget logic; clarified type usage)          |
| R14-11 | Low      | record_spend_ledger_with_team lock release timing between team-check and key-check      | Fixed (locks held for entire transaction; team lock acquired first, key second)            |
| R14-12 | Low      | fired_at redundant in budget_alert_log (period_start + fired_at not needed together)     | Fixed (removed from alert payload; period_start suffices for billing period ID)           |
| S1     | Medium   | /keys endpoint uses i64 for budget_limit/current_spend (inconsistent with u64 elsewhere) | Fixed (changed to u64 in /keys response)                                                 |
| S2     | Low      | Alerts endpoint budget_limit is i64 (should be u64)                                    | Fixed (changed to u64)                                                                   |
| S3     | Low      | fired array still contained fired_at despite R14-12 claim of removal                     | Fixed (fired_at removed from fired array entry structure)                                |
| S4     | Medium   | get_team_spend signature changed but no call site update guidance                        | Fixed (added backward compatibility note with period boundary derivation)                |
| S5     | Low      | key_spend table schema not defined in RFC-0904 or RFC-0903                               | Fixed (added key_spend schema in Team current_spend data source section)                 |
| S6     | Low      | RFC-0910 pricing_hash test vector not referenced in RFC-0904                            | Fixed (added test vector reference: 4a065c51147d4730379d600c4a491778b98f66a8e381c5dfdf51f42052c32f60) |
| S7     | Medium   | F1 alert current_spend source (key_spend vs spend_ledger) not explicit in F1 section   | Fixed (added current_spend source note: reads from key_spend.total_spend)                 |
| S8     | Low      | F2 reset scope ambiguous (total_spend only vs total_spend+window_start)                 | Fixed (clarified: resets both total_spend AND window_start)                                |
| S9     | Low      | check_budget error type distinction not clear in Budget Pre-Check section              | Fixed (added explicit error type distinction note)                                       |
| S10    | Low      | percent_used formula could be read as unconditional division                             | Fixed (added budget_limit>0 guard to formula)                                            |
| U1     | Medium   | Team endpoint percent_used unguarded (S10 missed team endpoint)                          | Fixed (added budget_limit>0 guard to team endpoint formula)                             |
| U2     | Medium   | F1 alert evaluation timing (sync/async) not specified                                   | Fixed (synchronous evaluation; delivery failure does not fail client request)            |
| U3     | Medium   | Admin API auth mechanism undefined                                                       | Fixed (Bearer token spec: hex-encoded 32+ bytes, SHA-256 hashed, constant-time compare) |
| U4     | Low      | budget_alert_log.fired_at dead storage                                                   | Fixed (documented as debug-only storage, implementations MAY omit)                     |
| U5     | Low      | reset_trigger informational nature undocumented                                          | Fixed (documented as purely informational, audit/debug only)                           |
| U6     | Low      | F3 deduct failure API response not specified                                             | Fixed (client receives 200 OK; deduct failure is internal reconciliation issue)          |
| U7     | Low      | Team budget_limit==0 (all unlimited) semantics missing                                   | Fixed (all-unlimited team → budget_limit=0, percent_used=null)                          |
| U8     | Low      | get_team_spend period derivation formula missing                                        | Fixed (added monthly/weekly/daily period derivation formulas)                          |
| U9     | Low      | F1 multi-threshold crossing semantics ambiguous                                           | Fixed (all crossed thresholds fire; alert handler evaluates all thresholds post-spend)  |
| U10    | Low      | RFC-0917 integration doesn't note RFC-0904 Draft dependency                              | Fixed (noted RFC-0904 must reach Accepted before RFC-0917 Phase 4 integration)            |
| U11    | Low      | token_source CHECK constraint vs F2 reset clarification                                   | Fixed (clarified F2 reset does NOT touch spend_ledger)                                  |
| U12    | Low      | carry_over_unused default value inconsistent                                             | Fixed (clarified field defaults to true when absent from metadata)                      |

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
