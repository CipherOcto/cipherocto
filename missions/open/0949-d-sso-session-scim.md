# Mission: 0949-d — Session Management and SCIM

## Status

Completed

Open

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- Mission-0949-a: SSO Core Infrastructure
- Mission-0949-b: OAuth2/OIDC

## Acceptance Criteria

### Session Management
- [x] Define `Session` struct (id, user_id, provider, created_at, last_access, expires_at)
- [x] Define `SessionStorage` trait (create, get, refresh, revoke, cleanup_expired)
- [x] Sliding window session lifecycle (extend on activity)

### SCIM 2.0 Types
- [x] Implement `ScimUser` (schemas, id, externalId, userName, name.givenName, name.familyName, emails, active, groups)
- [x] Implement `ScimEmail` (value, type, primary)
- [x] Implement `ScimGroup` (schemas, id, displayName, members)
- [x] Implement `ScimPatchOp` (schemas, Operations)

### SCIM Operations
- [x] Implement list, get, create, update, patch, deactivate operations
- [x] Deactivation vs deletion semantics (deactivate preferred — set active=false)
- [x] SCIM filter syntax documentation (eq, ne, co, sw, ew)
- [x] Implement `sync_users()` with per-user error isolation (`SyncResult`/`SyncError`)

### SCIM Server-Side Endpoints
- [x] Implement `GET /scim/v2/Users` — List users (SCIM filter + pagination)
- [x] Implement `GET /scim/v2/Users/:id` — Get user
- [x] Implement `POST /scim/v2/Users` — Create user
- [x] Implement `PUT /scim/v2/Users/:id` — Replace user
- [x] Implement `PATCH /scim/v2/Users/:id` — Patch user
- [x] Implement `DELETE /scim/v2/Users/:id` — Delete user
- [x] Implement `GET /scim/v2/Groups` — List groups
- [x] Implement `GET /scim/v2/ServiceProviderConfig` — SCIM service provider config
- [x] Implement `GET /scim/v2/ResourceTypes` — SCIM resource types

### SCIM Error Handling
- [x] Use SCIM-specific error format (RFC 7644 Section 3.12)
- [x] SCIM rate limiting: 20/min per IP (coordinate with RFC-0933)

### Error Handling
- [x] Distributed across all 0949 missions (19 error codes total)

### Verification
- [x] Clippy passes with zero warnings
- [x] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/auth/sso/session.rs` — New
- `crates/quota-router-core/src/auth/sso/scim.rs` — New
- `crates/quota-router-core/src/auth/sso/scim_server.rs` — New
- `crates/quota-router-core/src/admin.rs` — Add session/SCIM endpoints
