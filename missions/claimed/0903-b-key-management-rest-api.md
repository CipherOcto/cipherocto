# Mission: RFC-0903 Phase 2 — Key Management REST API Routes

## Status

Claimed

## RFC

RFC-0903 (Economics): Virtual API Key System — Final v29

## Dependencies

- Mission: 0903-a-ledger-based-budget-enforcement (ledger must exist before route integration)

## Claimant

@claude-code

## Notes

**Implemented:**
- Admin API extracted to `admin.rs` — proper separation from proxy.rs
- AdminServer wired up in commands.rs — runs on --admin-port (default 8081)
- ProxyServer runs on --proxy-port (default 8080)
- Team endpoints: POST /api/team, GET /api/team/:team_id, PUT /api/team/:team_id (stubs with body parsing needed)
- GET /key/info — LiteLLM-compatible key info from token (using lookup_by_hash)
- check_team_key_limit() — enforce MAX_KEYS_PER_TEAM = 100 (in keys/mod.rs)
- HTTP verb fix: DELETE /api/keys/:key_id (not POST /revoke) ✅
- update_team() added to KeyStorage trait ✅
- count_keys_for_team() added to KeyStorage trait ✅

**Missing:**
- GenerateKeyRequest parsing for /key/generate endpoint (full JSON body parsing)
- handle_create_team/handle_update_team full JSON body parsing
- revoke_key with reason tracking (revoked_by, revocation_reason fields)

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
| `crates/quota-router-core/src/admin.rs` | Admin API HTTP handlers (key/team CRUD) |
| `crates/quota-router-core/src/proxy.rs` | LLM proxy only (separated from admin) |
| `crates/quota-router-core/src/keys/mod.rs` | check_team_key_limit |
| `crates/quota-router-core/src/storage.rs` | update_team, count_keys_for_team |
| `crates/quota-router-core/src/config.rs` | Added db_path for database location |
| `crates/quota-router-cli/src/commands.rs` | Wire up AdminServer with database |
| `crates/quota-router-cli/src/cli.rs` | Added --admin-port option |

## Complexity

Medium — REST API implementation with database integration

## Reference

- RFC-0903 §API Endpoints (lines 293-311)
- RFC-0903 §Key Generation (lines 928-1001)
- RFC-0903 §Key Validation (lines 1004-1022)
- RFC-0903 §Key Rotation Protocol (lines 668-739)
- RFC-0903 §Cache Invalidation (lines 1118-1154)
- RFC-0903 §Key Management Routes (lines 2204-2237)
