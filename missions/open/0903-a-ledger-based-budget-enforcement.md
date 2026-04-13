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
      pricing_hash BLOB NOT NULL,
      token_source TEXT NOT NULL,
      tokenizer_version TEXT,
      provider_usage_json TEXT,
      timestamp INTEGER NOT NULL,
      created_at INTEGER NOT NULL DEFAULT 0
  );
  ```
  - Note: `pricing_hash BLOB` uses stoolap's native Blob type — no hex encoding needed
  - Note: `created_at DEFAULT 0` avoids SQLite-specific syntax; application sets value explicitly
  - Note: `token_source TEXT` with application-level validation (stoolap CHECK constraint support TBD)
- [ ] **Index creation:**
  ```sql
  CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
  CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
  CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
  CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);
  CREATE INDEX idx_spend_ledger_team_time ON spend_ledger(team_id, timestamp);
  ```
  - `idx_spend_ledger_key_time` needed for `ORDER BY timestamp` queries in key replay
  - `idx_spend_ledger_team_time` needed for team replay (SUM by team_id)
- [ ] **TokenSource enum:** Implement `TokenSource` enum with `ProviderUsage` and `CanonicalTokenizer` variants:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum TokenSource {
      ProviderUsage,
      CanonicalTokenizer,
  }

  impl TokenSource {
      /// String for event_id hash (different from DB storage strings)
      pub fn to_hash_str(&self) -> &'static str { ... }
      /// String for database storage and CHECK constraint validation
      pub fn to_db_str(&self) -> &'static str { ... }
  }
  ```
  - Application-level validation: reject if `token_source` not in `["provider_usage", "canonical_tokenizer"]`
- [ ] **SpendEvent struct:** Implement `SpendEvent` struct with all fields per RFC-0903 §SpendEvent:
  ```rust
  pub struct SpendEvent {
      pub event_id: String,
      pub request_id: String,
      pub key_id: Uuid,
      pub team_id: Option<String>,
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
- [ ] **compute_event_id():** Implement deterministic event_id generation:
  ```rust
  pub fn compute_event_id(
      request_id: &str,
      key_id: &Uuid,
      provider: &str,
      model: &str,
      input_tokens: u32,
      output_tokens: u32,
      pricing_hash: &[u8; 32],
      token_source: TokenSource,
  ) -> String
  ```
  - Called by **caller** of `record_spend()` before constructing `SpendEvent`
  - `event_id` is passed into `record_spend()` via `SpendEvent`
- [ ] **record_spend() - key only:** Implement atomic budget enforcement with `FOR UPDATE`:
  ```rust
  pub fn record_spend(db: &Database, key_id: &Uuid, event: &SpendEvent) -> Result<(), KeyError> {
      let tx = db.transaction()?;

      // 1. Lock key row FOR UPDATE
      let budget: i64 = tx.query_row(
          "SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE",
          params![key_id.to_string()],
          |row| row.get(0),
      ).map_err(|_| KeyError::NotFound)?;  // Return NotFound if key doesn't exist

      // 2. Compute current = SUM from ledger
      let current: i64 = tx.query_row(
          "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
          params![key_id.to_string()],
          |row| row.get(0),
      )?;

      // 3. Verify budget
      if current + event.cost_amount as i64 > budget {
          return Err(KeyError::BudgetExceeded { current: current as u64, limit: budget as u64 });
      }

      // 4. Validate token_source before INSERT (CHECK constraint may not be enforced)
      let token_source_str = event.token_source.to_db_str();
      if token_source_str != "provider_usage" && token_source_str != "canonical_tokenizer" {
          return Err(KeyError::InvalidFormat); // or define specific error
      }

      // 5. INSERT (idempotent via ON CONFLICT)
      tx.execute(
          "INSERT INTO spend_ledger (
              event_id, request_id, key_id, team_id, provider, model,
              input_tokens, output_tokens, cost_amount, pricing_hash,
              token_source, tokenizer_version, provider_usage_json, timestamp
          ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
          ON CONFLICT(key_id, request_id) DO NOTHING",
          params![
              event.event_id.to_string(),
              event.request_id,
              event.key_id.to_string(),
              event.team_id,
              event.provider,
              event.model,
              event.input_tokens,
              event.output_tokens,
              event.cost_amount as i64,
              event.pricing_hash.as_slice(),  // &[u8; 32] coerces to &[u8]
              token_source_str,
              event.tokenizer_version,
              event.provider_usage_json,
              event.timestamp,
          ],
      )?;

      tx.commit()?;
      Ok(())
  }
  ```
  - Returns `KeyError::NotFound` if key_id doesn't exist (attack vector: spam with invalid keys → fail fast)
  - Application-level token_source validation as belt-and-suspenders since CHECK constraint enforcement TBD
- [ ] **record_spend_with_team():** Team budget with lock ordering (team BEFORE key):
  ```rust
  pub fn record_spend_with_team(
      db: &Database,
      key_id: &Uuid,
      team_id: &str,  // Uuid as string (consistent with schema team_id TEXT)
      event: &SpendEvent,
  ) -> Result<(), KeyError> {
      let tx = db.transaction()?;

      // 1. Lock team row FIRST (deadlock prevention per RFC-0903 §Lock Ordering Invariant)
      let team_budget: i64 = tx.query_row(
          "SELECT budget_limit FROM teams WHERE team_id = $1 FOR UPDATE",
          params![team_id],
      ).map_err(|_| KeyError::NotFound)?;

      // 2. Lock key row SECOND
      let key_budget: i64 = tx.query_row(
          "SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE",
          params![key_id.to_string()],
      ).map_err(|_| KeyError::NotFound)?;

      // 3. Compute both spends from ledger
      let key_current: i64 = tx.query_row(
          "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
          params![key_id.to_string()],
          |row| row.get(0),
      )?;
      let team_current: i64 = tx.query_row(
          "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE team_id = $1",
          params![team_id],
          |row| row.get(0),
      )?;

      // 4. Verify both budgets
      if key_current + event.cost_amount as i64 > key_budget {
          return Err(KeyError::BudgetExceeded { ... });
      }
      if team_current + event.cost_amount as i64 > team_budget {
          return Err(KeyError::TeamBudgetExceeded { ... });
      }

      // 5. INSERT (same as record_spend)
      ...
  }
  ```
- [ ] **Determinism tests:**
  - [ ] `compute_event_id()` with same inputs produces identical output on repeated calls
  - [ ] Different `token_source` for same inputs produces different `event_id`
  - [ ] Same request_id with different routers produces identical event_id (cross-router determinism)
- [ ] **Integration test:** Concurrent `record_spend()` with two transactions targeting same key — second must wait or fail (verify FOR UPDATE works)
- [ ] **Deprecation:** Add `#[deprecated]` to existing counter-based `record_spend()` with note pointing to ledger version

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/schema.rs` | Add `spend_ledger` table, `key_spend` can remain for migration or be dropped |
| `crates/quota-router-core/src/keys/models.rs` | Add `TokenSource` enum, `SpendEvent` struct |
| `crates/quota-router-core/src/storage.rs` | Implement ledger-based `record_spend()`, `record_spend_with_team()` |
| `crates/quota-router-core/src/keys/mod.rs` | Add `compute_event_id()` |
| `crates/quota-router-core/src/errors.rs` | Add `TeamBudgetExceeded` variant if not present |

## Complexity

Medium — database schema migration + atomic transaction implementation

## Notes

### On Conflict Syntax
Stoolap's `INSERT ... ON CONFLICT` syntax must be verified. If only `ON CONFLICT DO NOTHING` is supported (without target columns), the UNIQUE constraint on `(key_id, request_id)` can serve as the conflict target implicitly.

### CHECK Constraint Enforcement
Stoolap may not enforce CHECK constraints. TokenSource validation must be done at the application layer before INSERT.

### pricing_hash Type
`pricing_hash` is `[u8; 32]` in code and `BLOB` in schema. Stoolap's `ToParam` for `&[u8]` should handle this via the existing `impl ToParam for &[u8]` which calls `Value::blob(self.to_vec())`.

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
