# RFC-0934: Budget Management & Spend Tracking

## Status: Accepted

## Summary

Implement real budget management and spend tracking using stoolap, replacing the mock OCTO-W balance checking. This enables per-key budget enforcement. Per-user and per-team budget aggregation is deferred to a future RFC.

## Motivation

quota-router currently has mock OCTO-W balance checking. LiteLLM and any-llm both have real per-user spend tracking with database persistence. This RFC specifies how to implement budget management using stoolap.

## Specification

### 1. Budget Model

Use i64 for monetary values (microdollars) to match existing `ApiKey::budget_limit` type:
```rust
pub struct Budget {
    pub budget_id: String,        // UUID as string
    pub entity_type: EntityType,  // Key, User, Team
    pub entity_id: String,        // UUID as string
    pub max_budget: i64,          // max spend in microdollars (1 USD = 1_000_000)
    pub current_spend: i64,       // current spend in microdollars
    pub soft_limit_pct: u8,       // warning threshold percentage (e.g., 80)
    pub period: BudgetPeriod,     // Daily, Weekly, Monthly, Total
    pub period_start: i64,        // Unix timestamp
    pub period_end: i64,          // Unix timestamp (i64::MAX for Total period)
    pub alert_webhook: Option<String>,
}

pub enum EntityType {
    Key,    // Only Key is implemented in this RFC
    User,   // Reserved — per-user budgets deferred to future RFC
    Team,   // Reserved — per-team budgets deferred to future RFC
}

pub enum BudgetPeriod {
    Daily,
    Weekly,
    Monthly,
    Total,  // period_end = i64::MAX, never resets
}
```

**Note:** Existing `ApiKey::budget_limit` is `i64`. Using microdollars avoids floating-point precision issues. `soft_limit_pct` is a percentage (0-100) rather than absolute value. `BudgetPeriod::Total` uses `period_end = i64::MAX` so it never resets.

**Helper functions:**
```rust
fn period_duration_secs(period: &BudgetPeriod) -> i64 {
    match period {
        BudgetPeriod::Daily => 24 * 60 * 60,
        BudgetPeriod::Weekly => 7 * 24 * 60 * 60,
        BudgetPeriod::Monthly => 30 * 24 * 60 * 60,  // approximation — see note
        BudgetPeriod::Total => i64::MAX - Utc::now().timestamp(),  // effectively infinite
    }
}
```

**Note on monthly period:** Uses fixed 30-day duration (2,592,000 seconds). This means February budgets get 2 extra days, and months with 31 days lose a day. The drift accumulates over time (budget created Jan 1 resets Jan 31, then Mar 2, etc.). This approximation matches LiteLLM's approach and is acceptable for v1. Calendar-month boundaries (1st to 1st) would be more accurate but significantly more complex.

**Stoolap schema:**
```sql
CREATE TABLE budgets (
    entity_id TEXT NOT NULL,
    entity_type TEXT NOT NULL,  -- 'Key', 'User', 'Team'
    max_budget BIGINT NOT NULL,
    current_spend BIGINT NOT NULL DEFAULT 0,
    soft_limit_pct INTEGER NOT NULL DEFAULT 80,
    period TEXT NOT NULL,       -- 'Daily', 'Weekly', 'Monthly', 'Total'
    period_start BIGINT NOT NULL,
    period_end BIGINT NOT NULL,
    alert_webhook TEXT,
    PRIMARY KEY (entity_id, entity_type)
);
```

**Known limitation:** Each entity can have only ONE budget row (single period per entity). A key cannot have both a daily budget AND a monthly budget simultaneously. This matches LiteLLM's behavior. To support multiple periods per entity, change PRIMARY KEY to `(entity_id, entity_type, period)`.

**Usage struct:** Use existing `SpendEvent` from `keys/models.rs` or define:
```rust
pub struct Usage {
    pub model: String,
    pub input_tokens: u32,   // matches SpendEvent naming
    pub output_tokens: u32,  // matches SpendEvent naming
}
```

### 2. Spend Tracking

After each request:
1. Calculate cost from token count × pricing
2. Update current_spend in stoolap
3. Check against budget limits
4. Send alert if soft_limit exceeded

```rust
async fn track_spend(key: &ApiKey, usage: &Usage, pricing_table: &PricingTable) -> Result<()> {
    // Use existing compute_cost() from keys/mod.rs
    // Use pricing.rs compute_cost_from_pricing_table which returns Result<u64, BudgetError>
    // pricing_table is &PricingTable (from pricing.rs), NOT &PricingModel (from keys/models.rs)
    let cost = i64::try_from(compute_cost_from_pricing_table(pricing_table, usage.input_tokens, usage.output_tokens)?)
        .unwrap_or(i64::MAX);  // Cap at i64::MAX on overflow

    // CHECK FIRST, THEN RECORD — avoid charging for blocked requests
    // Atomic check-and-increment: only increments if budget allows
    let result: Option<(i64, u8)> = stoolap.query_optional(
        "UPDATE budgets SET current_spend = current_spend + ? \
         WHERE entity_id = ? \
         AND entity_type = 'Key' \
         AND current_spend + ? <= max_budget \
         RETURNING current_spend, soft_limit_pct",
        params![cost, key.key_id, cost],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).await?;

    let (new_spend, soft_limit_pct) = match result {
        Some((spend, pct)) => (spend, pct),
        None => {
            // Budget exceeded — UPDATE didn't match WHERE clause
            // Spend is NOT recorded — user not charged for blocked request
            // Query current spend for diagnostic purposes
            let current_spend: i64 = stoolap.query_row(
                "SELECT current_spend FROM budgets WHERE entity_id = ? AND entity_type = 'Key'",
                params![key.key_id],
                |row| row.get(0),
            ).await.unwrap_or(0);

            return Err(BudgetError::KeyBudgetExceeded {
                key_id: uuid::Uuid::parse_str(&key.key_id).unwrap_or_default(),
                current: current_spend.max(0) as u64,
                limit: key.budget_limit.max(0) as u64,
                requested: cost.max(0) as u64,
            });
        }
    };

    // Use max_budget from budgets table (consistent with check_budget hard limit)
    let max_budget: i64 = stoolap.query_row(
        "SELECT max_budget FROM budgets WHERE entity_id = ? AND entity_type = 'Key'",
        params![key.key_id],
        |row| row.get(0),
    ).await.unwrap_or(key.budget_limit);

    let soft_limit = max_budget * (soft_limit_pct as i64) / 100;
    if new_spend >= soft_limit {
        send_alert(key, new_spend).await?;
    }

    Ok(())
}
```

**Note:** Uses existing `compute_cost_from_pricing_table()` from keys/mod.rs (returns Result<u64, BudgetError>, cast to i64). `soft_limit_pct` read from budgets table, not from ApiKey.

### 3. Budget Enforcement

Pre-request check (atomic to avoid race condition):
```rust
async fn check_budget(key: &ApiKey) -> Result<()> {
    // Single atomic statement: check limit AND reset period if needed
    // Avoids TOCTOU race between check and reset
    // Use SQL CASE to compute new period_end based on period type
    // Use query_optional (not query_row) to handle keys without a budget row —
    // no budget configured means no limit, so pass through.
    let result: Option<(i64, i64)> = stoolap.query_optional(
        "UPDATE budgets SET \
         current_spend = CASE WHEN period_end < ? THEN 0 ELSE current_spend END, \
         period_end = CASE WHEN period_end < ? THEN \
           CASE period \
             WHEN 'Daily' THEN ? + 86400 \
             WHEN 'Weekly' THEN ? + 604800 \
             WHEN 'Monthly' THEN ? + 2592000 \
             ELSE 9223372036854775807 \
           END \
           ELSE period_end END \
         WHERE entity_id = ? AND entity_type = 'Key' \
         RETURNING current_spend, period_end",
        {let now = Utc::now().timestamp(); params![now, now, now, now, now, key.key_id]},
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).await?;

    // No budget row = no limit configured → pass through
    let (current_spend, _period_end) = match result {
        Some((spend, end)) => (spend, end),
        None => return Ok(()),
    };

    // Check hard limit — use max_budget from budgets table, not key.budget_limit
    let max_budget: i64 = stoolap.query_row(
        "SELECT max_budget FROM budgets WHERE entity_id = ? AND entity_type = 'Key'",
        params![key.key_id],
        |row| row.get(0),
    ).await.unwrap_or(key.budget_limit);

    if current_spend >= max_budget {
        return Err(BudgetError::KeyBudgetExceeded {
            key_id: uuid::Uuid::parse_str(&key.key_id).unwrap_or_default(),
            current: current_spend as u64,
            limit: max_budget as u64,
            requested: 0,
        });
    }

    Ok(())
}
```

### 4. Cost Calculation

Use existing `compute_cost_from_pricing_table()` from `keys/mod.rs` which returns `Result<u64, BudgetError>`:
```rust
// keys/mod.rs — returns Result<u64, BudgetError>
pub fn compute_cost_from_pricing_table(
    pricing_table: &PricingTable,
    input_tokens: u32,
    output_tokens: u32,
) -> Result<u64, BudgetError> { ... }
```

**Usage in track_spend:**
```rust
let cost = i64::try_from(compute_cost_from_pricing_table(pricing_table, usage.input_tokens, usage.output_tokens)?)
        .unwrap_or(i64::MAX);  // Cap at i64::MAX on overflow
```

**Note:** Both `compute_cost()` and `compute_cost_from_pricing_table()` use `u32` for token counts. The `pricing_table` parameter is `&PricingTable` (from `pricing.rs`), NOT `&PricingModel` (from `keys/models.rs`).

### 5. Alert Webhooks

When soft_limit_pct exceeded, POST to configured webhook:

```json
{
  "event": "budget_warning",
  "entity_type": "key",
  "entity_id": "...",
  "current_spend": 85500000,
  "soft_limit_pct": 80,
  "max_budget": 100000000,
  "period": "monthly",
  "timestamp": 1715800000
}
```

**Implementation:**
- HTTP POST with JSON body
- Timeout: 5 seconds (configurable)
- Retry: 3 attempts with exponential backoff (1s, 2s, 4s)
- Error handling: log warning on failure, don't block request
- Deduplication: don't send same alert within 1 hour

### 6. Management API

- `GET /budget/{entity_type}/{entity_id}` — get budget (matches RFC-0932 paths)
- `POST /budget/{entity_type}/{entity_id}` — set budget
- `DELETE /budget/{entity_type}/{entity_id}` — remove budget
- `GET /budget/{entity_type}/{entity_id}/history` — spend history

### 7. Configuration

```yaml
budget:
  enabled: true
  storage: stoolap
  default_max_budget: 100000000  # microdollars (100 USD = 100_000_000)
  default_period: monthly
  soft_limit_percentage: 80
  alert_webhook: null
```

## Dependencies

- RFC-0914: stoolap-only persistence
- RFC-0910: pricing table registry
- RFC-0903: Virtual API Key System (ApiKey struct and KeyStorage)

## Test Plan

1. Spend tracking updates current_spend correctly
2. Hard limit blocks requests when exceeded
3. Soft limit triggers alert webhook
4. Budget reset on period boundary
5. Per-key budgets work correctly (per-user and per-team are deferred)
6. Cost calculation matches expected values
7. stoolap persistence survives restart
8. Management API CRUD operations work
