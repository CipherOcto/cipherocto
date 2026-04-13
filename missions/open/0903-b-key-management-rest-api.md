# Mission: RFC-0903 Phase 2 — Key Management REST API Routes

## Status

Open

## RFC

RFC-0903 (Economics): Virtual API Key System — Final v29

## Summary

Implement the HTTP REST API routes for key and team CRUD operations as specified in RFC-0903 §API Endpoints. This includes key generation, listing, revocation, rotation, and team management endpoints compatible with LiteLLM's key management API.

## Motivation

RFC-0903 specifies a REST API for key management that does not exist in the current `commands.rs`. The current CLI only has basic operational commands (init, add_provider, balance, list, proxy, route). Key management requires:
- POST /key/generate — Create new API key
- GET /key/list — List keys with filters
- DELETE /key/{key_id} — Revoke key
- PUT /key/{key_id} — Update key (budget, limits)
- POST /key/regenerate — Rotate key
- POST /team, GET /team/{team_id}, PUT /team/{team_id} — Team management
- GET /key/info — Get key info from token (LiteLLM compatibility)

## Dependencies

- Mission: 0903-a-ledger-based-budget-enforcement (ledger must exist before route integration)

## Acceptance Criteria

- [ ] **POST /key/generate:** Implement key generation endpoint per RFC-0903 §GenerateKeyRequest/GenerateKeyResponse:
  ```rust
  pub async fn generate_key(
      db: &Database,
      req: GenerateKeyRequest,
  ) -> Result<GenerateKeyResponse, KeyError>
  ```
  - Accepts budget_limit, rpm_limit, tpm_limit, key_type, auto_rotate, rotation_interval_days, team_id, metadata
  - Returns key (sk-qr-...), key_id, expires, team_id, key_type, created_at
  - Enforces MAX_KEYS_PER_TEAM (100) via check_team_key_limit()
- [ ] **GET /key/list:** List keys with optional team_id filter:
  ```rust
  pub fn list_keys(db: &Database, team_id: Option<&str>) -> Result<Vec<ApiKey>, KeyError>
  ```
- [ ] **DELETE /key/{key_id}:** Revoke key with reason tracking:
  ```rust
  pub fn revoke_key(
      db: &Database,
      key_id: &Uuid,
      revoked_by: &str,
      reason: &str,
  ) -> Result<(), KeyError>
  ```
  - Sets revoked=1, revoked_at, revoked_by, revocation_reason
  - Invalidates cache and rate limiter
- [ ] **PUT /key/{key_id}:** Update key (budget_limit, rpm_limit, tpm_limit, expires_at):
  ```rust
  pub fn update_key(
      db: &Database,
      key_id: &Uuid,
      updates: &KeyUpdates,
  ) -> Result<(), KeyError>
  ```
- [ ] **POST /key/regenerate:** Rotate key with grace period:
  ```rust
  pub fn rotate_key(
      db: &Database,
      key_id: &Uuid,
  ) -> Result<GenerateKeyResponse, KeyError>
  ```
  - Generates new key with rotated_from reference
  - Invalidates old key immediately
  - Sets expiration grace period
- [ ] **POST /team:** Create team:
  ```rust
  pub fn create_team(db: &Database, team: &Team) -> Result<(), KeyError>
  ```
- [ ] **GET /team/{team_id}:** Get team info:
  ```rust
  pub fn get_team(db: &Database, team_id: &str) -> Result<Option<Team>, KeyError>
  ```
- [ ] **PUT /team/{team_id}:** Update team:
  ```rust
  pub fn update_team(db: &Database, team_id: &str, name: &str, budget_limit: u64) -> Result<(), KeyError>
  ```
- [ ] **GET /key/info:** LiteLLM-compatible key info from token:
  ```rust
  pub fn get_key_info(db: &Database, key: &str) -> Result<ApiKey, KeyError>
  ```
- [ ] **check_team_key_limit():** Enforce MAX_KEYS_PER_TEAM = 100 at application layer (per RFC-0903 §Not Implemented):
  ```rust
  pub fn check_team_key_limit(db: &Database, team_id: &Uuid) -> Result<(), KeyError>
  ```

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/keys/mod.rs` | Implement generate_key, rotate_key, revoke_key, check_team_key_limit |
| `crates/quota-router-core/src/storage.rs` | Add delete_key, update_team to KeyStorage trait |
| `crates/quota-router-cli/src/commands.rs` | Add HTTP route handlers for key/team CRUD |
| `crates/quota-router-cli/src/main.rs` | Wire up new routes |

## Complexity

Medium — REST API implementation with database integration

## Reference

- RFC-0903 §API Endpoints (lines 293-311)
- RFC-0903 §Key Generation (lines 928-1001)
- RFC-0903 §Key Validation (lines 1004-1022)
- RFC-0903 §Key Rotation Protocol (lines 668-739)
- RFC-0903 §Cache Invalidation (lines 1118-1154)
- RFC-0903 §Key Management Routes (lines 2204-2237)
