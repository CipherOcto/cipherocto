# Mission: RFC-0903 Phase 2 — Key Management REST API Routes

## Status

Completed

## RFC

RFC-0903 (Economics): Virtual API Key System — Final v29

## Dependencies

- Mission: 0903-a-ledger-based-budget-enforcement (ledger must exist before route integration)

## Claimant

@claude-code

## Summary

Implemented the HTTP REST API routes for key and team CRUD operations as specified in RFC-0903 §API Endpoints. Key generation, listing, revocation, rotation, and team management endpoints are fully implemented and compatible with LiteLLM's key management API.

## Implementation

### Key Endpoints

| Method | Path | Handler |
|--------|------|---------|
| POST | /key/generate | handle_create_key |
| GET | /key/list | handle_list_keys |
| PUT | /key/:id | handle_update_key |
| DELETE | /key/:id | handle_revoke_key |
| POST | /key/:id/regenerate | handle_rotate_key |
| GET | /key/info | handle_get_key_info |

### Team Endpoints

| Method | Path | Handler |
|--------|------|---------|
| POST | /team | handle_create_team |
| GET | /team/:team_id | handle_get_team |
| PUT | /team/:team_id | handle_update_team |

### Key Features

- JSON body parsing for all POST/PUT handlers using http-body-util
- Team key limit enforcement (MAX_KEYS_PER_TEAM = 100)
- Revoke reason tracking (revoked_by, revocation_reason)
- Key rotation with grace period
- LiteLLM-compatible /key/info endpoint
- Admin API separated from proxy (admin.rs vs proxy.rs)

## Acceptance Criteria

- [x] **POST /key/generate:** GenerateKeyRequest/GenerateKeyResponse
- [x] **GET /key/list:** List keys with optional team_id filter
- [x] **DELETE /key/:id:** Revoke key with reason tracking
- [x] **PUT /key/:id:** Update key (budget_limit, rpm_limit, tpm_limit)
- [x] **POST /key/:id/regenerate:** Rotate key with grace period
- [x] **POST /team:** Create team
- [x] **GET /team/:team_id:** Get team info
- [x] **PUT /team/:team_id:** Update team
- [x] **GET /key/info:** LiteLLM-compatible key info from token
- [x] **check_team_key_limit:** Enforce MAX_KEYS_PER_TEAM = 100

## Key Files Modified

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

## Notes

**Implemented:**
- Admin API extracted to `admin.rs` — proper separation from proxy.rs
- AdminServer wired up in commands.rs — runs on --admin-port (default 8081)
- ProxyServer runs on --proxy-port (default 8080)
- Team endpoints: POST /team, GET /team/:team_id, PUT /team/:team_id
- GET /key/info — LiteLLM-compatible key info from token (using lookup_by_hash)
- check_team_key_limit() — enforce MAX_KEYS_PER_TEAM = 100 (in keys/mod.rs)
- HTTP verb fix: DELETE /key/:id (not POST /revoke)
- update_team() added to KeyStorage trait
- count_keys_for_team() added to KeyStorage trait
- RevokeKeyRequest parsing with revoked_by and reason fields
- Full JSON body parsing for all POST/PUT handlers
- Route paths aligned with RFC-0903 LiteLLM compatibility (/key/... not /api/keys)

**Completed:** All acceptance criteria met. Commits 6882e4c, 3eaaf29, 974c356.
