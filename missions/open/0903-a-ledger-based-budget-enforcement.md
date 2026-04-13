# Mission: RFC-0903 Phase 1 — Ledger-Based Budget Enforcement

## Status

Open

## RFC

RFC-0903 (Economics): Virtual API Key System — Final v29

## Summary

Replace the deprecated `key_spend` counter table with the `spend_ledger` architecture as specified in RFC-0903 §Ledger-Based Architecture. Implement `record_spend()` with `FOR UPDATE` row locking to prevent double-spend in multi-router deployments. This is the canonical spend enforcement pattern required for deterministic quota accounting.

## Motivation

Current implementation uses `key_spend` counter table which is **deprecated** per RFC-0903 v22+. The ledger-based approach provides:
- Single source of truth for economic state
- Deterministic replay (SUM from ledger = authoritative balance)
- Prevention of double-spend via `FOR UPDATE` row locking
- Full audit trail for disputes/fraud

## Dependencies

**Stoolap FOR UPDATE:** Already implemented in stoolap (missions 0912-a/b/c completed, executor routes to `collect_all_rows_for_update()`). The SQL syntax `SELECT ... FOR UPDATE` is available and functional.

**Stoolap Blob:** Already implemented in stoolap (`DataType::Blob`, `Value::Blob`, `FromValue for Vec<u8>`, `ToParam for Vec<u8>`, `parse_data_type("BYTEA(32)")`). The TODO comments claiming "stoolap doesn't support BYTEA yet" are stale — Blob is fully implemented. The hex encoding workaround in quota-router exists because code reads Blob as String instead of Vec<u8>. This mission should use direct binary storage.

No other dependencies — foundational for RFC-0903 compliance

## Acceptance Criteria

- [ ] **Schema migration:** Replace `key_spend` table with `spend_ledger` table per RFC-0903 DDL:
  ```sql
  CREATE TABLE spend_ledger (
      event_id TEXT PRIMARY KEY,
      request_id TEXT NOT NULL,
      key_id TEXT NOT NULL,
      UNIQUE(key_id, request_id),
      team_id TEXT,
      provider TEXT NOT NULL,
      model TEXT NOT NULL,
      input_tokens INTEGER NOT NULL,
      output_tokens INTEGER NOT NULL,
      cost_amount BIGINT NOT NULL,
      pricing_hash BYTEA(32) NOT NULL,
      token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
      tokenizer_version TEXT,
      provider_usage_json TEXT,
      timestamp INTEGER NOT NULL,
      created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
  );
  ```
  - Note: `pricing_hash BYTEA(32)` uses stoolap's native Blob type — no hex encoding needed
- [ ] **TokenSource enum:** Implement `TokenSource` enum with `ProviderUsage` and `CanonicalTokenizer` variants, including `to_hash_str()` and `to_db_str()` methods
- [ ] **SpendEvent struct:** Implement `SpendEvent` struct with all fields per RFC-0903 §SpendEvent
- [ ] **record_spend() - key only:** Implement atomic budget enforcement with `FOR UPDATE` row locking:
  ```rust
  pub fn record_spend(db: &Database, key_id: &Uuid, event: &SpendEvent) -> Result<(), KeyError> {
      // 1. SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE (acquires row lock)
      // 2. Compute current = SUM(cost_amount) FROM spend_ledger WHERE key_id = $1
      // 3. Verify budget not exceeded
      // 4. INSERT INTO spend_ledger ... ON CONFLICT(key_id, request_id) DO NOTHING
  }
  ```
  - Uses stoolap's `SELECT ... FOR UPDATE` syntax (already implemented per RFC-0912)
- [ ] **record_spend_with_team():** Implement team budget enforcement with lock ordering (team BEFORE key):
  ```rust
  pub fn record_spend_with_team(db: &Database, key_id: &Uuid, team_id: &str, event: &SpendEvent) -> Result<(), KeyError> {
      // 1. SELECT budget_limit FROM teams WHERE team_id = $1 FOR UPDATE (lock team first - deadlock prevention)
      // 2. SELECT budget_limit FROM api_keys WHERE key_id = $2 FOR UPDATE (lock key second)
      // 3. Compute key_current and team_current from ledger
      // 4. Verify both budgets
      // 5. INSERT INTO spend_ledger
  }
  ```
  - Lock ordering (team before key) per RFC-0903 §Lock Ordering Invariant — prevents deadlocks
- [ ] **compute_event_id():** Implement deterministic event_id generation per RFC-0903 §Deterministic event_id generation
- [ ] **Migration path:** `spend_ledger` index creation:
  ```sql
  CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
  CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
  CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
  CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);
  ```
- [ ] **Deprecation:** Add `#[deprecated]` to existing `record_spend()` that uses counter approach

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/schema.rs` | Add spend_ledger table, drop key_spend |
| `crates/quota-router-core/src/keys/models.rs` | Add TokenSource enum, SpendEvent struct |
| `crates/quota-router-core/src/storage.rs` | Implement ledger-based record_spend() |
| `crates/quota-router-core/src/keys/mod.rs` | Add compute_event_id() |

## Complexity

Medium — database schema migration + atomic transaction implementation

## Reference

- RFC-0903 §Ledger-Based Architecture (lines 1722-2037)
- RFC-0903 §Atomic Budget Accounting (lines 447-497)
- RFC-0903 §Ledger-Based Spend Recording (lines 1224-1334)
- RFC-0903 §Canonical Token Accounting (lines 1336-1472)
- RFC-0903 §Deterministic Replay Procedure (lines 1658-1670)
- RFC-0903 §Lock Ordering Invariant (lines 1700-1711)
- **RFC-0912 (Accepted):** FOR UPDATE row locking — stoolap missions 0912-a/b/c completed, `SELECT ... FOR UPDATE` syntax available
- **Stoolap Blob:** Already implemented — `DataType::Blob`, `Value::Blob`, `FromValue for Vec<u8>`, `ToParam for Vec<u8>`, `parse_data_type("BYTEA(32)")` all functional

## Future Work

**Note on Decimal Types:** RFC-0903 was written before RFC-0202 (BIGINT/DECIMAL) and RFC-0202-B (DQA) existed. It uses integer cost units (e.g., nanodollars) to avoid floating-point determinism issues. A future revision (per RFC-0909 Deterministic Quota Accounting) could explore using DFP or DQA for `cost_amount` to represent real pricing like `$0.0016` directly. This would require:
- Canonical unit definition (e.g., store prices in smallest unit)
- Avoid division by pre-computing cost tables
- Commit unit interpretation via `pricing_hash`
This is NOT part of 0903-a scope — flagged for RFC-0909 consideration.
