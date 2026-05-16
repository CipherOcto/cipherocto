# Mission: 0934-a — Budget & Spend Tracking

## Status

Open

## RFC

RFC-0934 (Economics): Budget Management & Spend Tracking

## Dependencies

- Mission-0932-a: Gateway Auth Wiring (provides ApiKey context)

**Note:** Mission-0914-a: Stoolap Persistence (Open — provides storage backend). This mission assumes stoolap is already available as a storage backend. If stoolap integration is not ready, use in-memory storage with periodic flush as interim solution.

## Context

The existing `KeyMiddleware::check_budget()` compares spend against `budget_limit`. But spend tracking uses mock OCTO-W balance. This mission implements real spend tracking with stoolap.

## Acceptance Criteria

### Spend Tracking

- [ ] Create `budgets` table in stoolap: `(budget_id TEXT, entity_id TEXT, entity_type TEXT, max_budget INTEGER, current_spend INTEGER, soft_limit_percentage INTEGER DEFAULT 80, period TEXT, period_start INTEGER, period_end INTEGER, alert_webhook TEXT, PRIMARY KEY (entity_id, entity_type))`

### Types to Define

```
enum BudgetPeriod { Daily, Weekly, Monthly, Total }
enum EntityType { Key, User, Team }
```
- [ ] After each request, calculate cost using existing `compute_cost_from_pricing_table()` from keys/mod.rs (returns Result<u64, BudgetError>, cast to i64). Note: `compute_cost_from_pricing_table` takes `&PricingTable` (from pricing.rs), not `&PricingModel` (from keys/models.rs).
- [ ] Use `query_optional()` for budget lookups — the entity may not have a budget row yet. `query_row` will error on zero rows.
- [ ] Atomic UPDATE with WHERE clause to avoid race conditions (check THEN record)
- [ ] Return `BudgetError::KeyBudgetExceeded` when hard limit exceeded
- [ ] Read `soft_limit_percentage` from budgets table (not from ApiKey)
- [ ] Alert webhook when `soft_limit_percentage` exceeded

### Budget Enforcement

- [ ] Pre-request budget check using single atomic UPDATE with CASE (reset period if needed)
- [ ] Budget reset on period boundary (atomic, part of check_budget)
- [ ] Per-key budgets (entity_type = Key)
- [ ] Use `BudgetPeriod::Total` with `period_end = i64::MAX` for lifetime budgets

### Cost Calculation

- [ ] Use existing `compute_cost_from_pricing_table()` from keys/mod.rs (returns Result<u64, BudgetError>). Note: takes `&PricingTable` (from pricing.rs), not `&PricingModel` (from keys/models.rs).
- [ ] Return `BudgetError::ModelNotFound(String)` for unpriced models
- [ ] Cast u64 to i64 for budget tracking — `SpendEvent.cost_amount` is `u64` (microdollars). Cast to `i64` for budget comparison using `i64::try_from()` with overflow check.

### Management API

- [ ] `GET /budget/{entity_type}/{entity_id}` — get budget
- [ ] `POST /budget/{entity_type}/{entity_id}` — set budget
- [ ] `DELETE /budget/{entity_type}/{entity_id}` — remove budget
- [ ] `GET /budget/{entity_type}/{entity_id}/history` — spend history

### Alert Webhooks

- [ ] HTTP POST with JSON body to configured webhook URL
- [ ] Timeout: 5 seconds, retry: 3 attempts with exponential backoff (1s, 2s, 4s)
- [ ] Deduplication: don't send same alert within 1 hour

**Alert deduplication:** In-memory `HashSet<(entity_id, entity_type, period)>` tracking which alerts have been sent in the current budget period. Reset on period rollover.

### Configuration

- [ ] `budget.enabled: true`
- [ ] `budget.storage: stoolap`
- [ ] `budget.default_max_budget: 100000000` (microdollars)
- [ ] `budget.default_period: monthly`
- [ ] `budget.soft_limit_percentage: 80`
- [ ] `budget.alert_webhook: null`

### Tests

- [ ] Spend tracking updates current_spend correctly
- [ ] Hard limit blocks requests when exceeded
- [ ] Soft limit triggers alert webhook
- [ ] Budget reset on period boundary
- [ ] Cost calculation matches expected values
- [ ] Stoolap persistence survives restart (requires Mission-0914-a or interim in-memory + flush solution)

## Key Files

- `crates/quota-router-core/src/proxy.rs` — main request handler
- `crates/quota-router-core/src/middleware.rs` — check_budget(), record_spend()
- `crates/quota-router-core/src/keys/models.rs` — SpendEvent struct
- `crates/quota-router-core/src/keys/mod.rs` — compute_cost_from_pricing_table()

## Notes

The existing `KeyMiddleware::process_response()` handles spend ledger recording (delegates to `record_spend_ledger()`). The older `record_spend()` method is deprecated. This mission is about replacing mock OCTO-W with real stoolap-backed budget tracking.
