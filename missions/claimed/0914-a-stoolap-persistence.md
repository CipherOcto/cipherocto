# Mission: 0914-a — Stoolap Persistence for Rate Limiting and Caching

## Status

Complete

## RFC

RFC-0914 (Economics): Stoolap-Only Quota Router Persistence (Draft)

> **Note:** RFC-0914 is in Draft status. This mission tracks the persistence work required by Missions 0933-a, 0934-a, and 0935-a. The mission exists to formalize the dependency — implementation may proceed before RFC-0914 is Accepted if the specific table schemas below are stable.

## Dependencies

- RFC-0912: Stoolap FOR UPDATE Row Locking (Accepted)
- RFC-0913: Stoolap Pub/Sub for Cache Invalidation (Accepted)

## Context

Missions 0933-a (Rate Limiting), 0934-a (Budget/Spend), and 0935-a (Secret Manager) all require stoolap persistence:

- **0933-a:** Rate limit state must survive restarts for distributed deployments
- **0934-a:** Budget/spend data already uses `spend_ledger` table (exists), but budget configuration needs a new `budgets` table
- **0935-a:** Secret cache needs a `StoolapCache` trait implementation

Current state:
- `api_keys` table: EXISTS (RFC-0903)
- `spend_ledger` table: EXISTS (RFC-0903)
- `rate_limit_state` table: DOES NOT EXIST (rate limiter is in-memory only)
- `budgets` table: DOES NOT EXIST
- `StoolapCache` trait: DOES NOT EXIST

## Scope

### In Scope

1. **`rate_limit_state` table** — persistent rate limiting counters
2. **`budgets` table** — budget configuration per entity
3. **`StoolapCache` trait** — generic cache interface for secret manager
4. **Migration strategy** — from in-memory to stoolap-backed state

### Out of Scope

- L1 cache table (future phase per RFC-0914)
- Full pub/sub protocol (only what's needed for rate limit invalidation)
- Redis migration tooling

## Acceptance Criteria

### rate_limit_state Table

- [x] `rate_limit_state` table created with schema:
  ```sql
  CREATE TABLE rate_limit_state (
      entity_id TEXT NOT NULL,
      entity_type TEXT NOT NULL,  -- 'key', 'user', 'team'
      counter_type TEXT NOT NULL,  -- 'rpm', 'tpm'
      current_count INTEGER NOT NULL DEFAULT 0,
      window_start INTEGER NOT NULL,  -- Unix timestamp (seconds)
      last_updated INTEGER NOT NULL,  -- Unix timestamp (seconds)
      PRIMARY KEY (entity_id, entity_type, counter_type)
  );
  ```
- [x] `flush_interval_seconds` config option added to `RouterSettings` (default: 60)
- [x] In-memory counters flushed to stoolap at configured interval
- [x] On startup, counters loaded from stoolap (if table exists)
- [x] On restart, counters reset if `window_start` is older than current window
- [x] `Retry-After` header value computed from window boundaries

### budgets Table

- [x] `budgets` table created with schema:
  ```sql
  CREATE TABLE budgets (
      entity_id TEXT NOT NULL,
      entity_type TEXT NOT NULL,  -- 'key', 'user', 'team'
      budget_limit BIGINT NOT NULL,  -- microdollars
      period TEXT NOT NULL,  -- 'daily', 'weekly', 'monthly', 'total'
      current_spend BIGINT NOT NULL DEFAULT 0,
      soft_limit_pct INTEGER,  -- 0-100, nullable
      alert_webhook TEXT,  -- nullable
      last_reset INTEGER NOT NULL,  -- Unix timestamp (seconds)
      created_at INTEGER NOT NULL,
      PRIMARY KEY (entity_id, entity_type)
  );
  ```
- [x] Budget reset logic: compute next reset from `period` + `last_reset`
- [x] `query_optional` used for all budget lookups (not `query_row`)
- [x] `BudgetPeriod` enum: `Daily`, `Weekly`, `Monthly`, `Total`
- [x] `EntityType` enum: `Key`, `User`, `Team`

### StoolapCache Trait

- [x] `StoolapCache` trait defined in `cache.rs`:
  ```rust
  #[async_trait]
  pub trait StoolapCache: Send + Sync {
      async fn get(&self, key: &str) -> Option<String>;
      async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<()>;
      async fn delete(&self, key: &str) -> Result<()>;
  }
  ```
- [x] `InMemoryCache` implementation using `HashMap<String, (String, Instant)>`
- [x] `InMemoryCache` used as interim until stoolap-backed implementation

### General

- [x] All tables use `INTEGER` for timestamps (Unix seconds, not `Instant`)
- [x] All timestamps use `SystemTime::now().duration_since(UNIX_EPOCH).as_secs() as i64`
- [x] Existing tests pass
- [x] New tests for each table CRUD operation
- [x] Clippy passes

## Notes

- **Instant vs SystemTime:** All persistent state uses `SystemTime` → i64 Unix seconds. `Instant` is only for in-memory timing (rate limiter refill logic).
- **Clock drift:** On reload, clamp future `last_updated` to `now` and log warning.
- **Budget period limitation:** Single period per entity (PRIMARY KEY on entity_id, entity_type). No daily+monthly budgets simultaneously. Document as known limitation.
- **Dead enum variants:** `EntityType::User` and `EntityType::Team` are reserved for future use. Only `Key` is used initially.
