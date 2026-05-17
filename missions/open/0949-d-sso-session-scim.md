# Mission: 0949-d — Session Management and SCIM

## Status

Open

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- Mission-0949-a: SSO Core Infrastructure
- Mission-0949-b: OAuth2/OIDC

## Acceptance Criteria

### Session Management
- [ ] Define `Session` struct (id, user_id, provider, created_at, last_access, expires_at)
- [ ] Define `SessionStorage` trait (create, get, refresh, revoke, cleanup_expired)
- [ ] Sliding window session lifecycle (extend on activity)

### SCIM 2.0 Types
- [ ] Implement `ScimUser` (schemas, id, externalId, userName, name.givenName, name.familyName, emails, active, groups)
- [ ] Implement `ScimEmail` (value, type, primary)
- [ ] Implement `ScimGroup` (schemas, id, displayName, members)
- [ ] Implement `ScimPatchOp` (schemas, Operations)

### SCIM Operations
- [ ] Implement list, get, create, update, patch, deactivate operations
- [ ] Deactivation vs deletion semantics (deactivate preferred — set active=false)
- [ ] SCIM filter syntax documentation (eq, ne, co, sw, ew)
- [ ] Implement `sync_users()` with per-user error isolation (`SyncResult`/`SyncError`)

### SCIM Server-Side Endpoints
- [ ] Implement `GET /scim/v2/Users` — List users (SCIM filter + pagination)
- [ ] Implement `GET /scim/v2/Users/:id` — Get user
- [ ] Implement `POST /scim/v2/Users` — Create user
- [ ] Implement `PUT /scim/v2/Users/:id` — Replace user
- [ ] Implement `PATCH /scim/v2/Users/:id` — Patch user
- [ ] Implement `DELETE /scim/v2/Users/:id` — Delete user
- [ ] Implement `GET /scim/v2/Groups` — List groups
- [ ] Implement `GET /scim/v2/ServiceProviderConfig` — SCIM service provider config
- [ ] Implement `GET /scim/v2/ResourceTypes` — SCIM resource types

### SCIM Error Handling
- [ ] Use SCIM-specific error format (RFC 7644 Section 3.12): `{"schemas": ["urn:ietf:params:scim:api:messages:2.0:Error"], "scimType": "...", "detail": "...", "status": "..."}`
- [ ] SCIM rate limiting: 20/min per IP (coordinate with RFC-0933)

### Error Handling
- [ ] Distributed across all 0949 missions (19 error codes total — 0949-a owns core JWT errors, 0949-b owns OAuth2 errors, 0949-c owns SAML errors, 0949-d owns SCIM/session errors)

### Verification
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

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
