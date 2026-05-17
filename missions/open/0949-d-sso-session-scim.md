# Mission: 0949-d — Session Management and SCIM

## Status

Open

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- Mission-0949-a: SSO Core Infrastructure
- Mission-0949-b: OAuth2/OIDC

## Acceptance Criteria

- [ ] Define `Session` struct (id, user_id, provider, created_at, last_access, expires_at)
- [ ] Define `SessionStorage` trait (create, get, refresh, revoke, cleanup_expired)
- [ ] Sliding window session lifecycle (extend on activity)
- [ ] Implement `GET /auth/userinfo` — return current user info
- [ ] Implement SCIM 2.0 types: `ScimUser`, `ScimEmail`, `ScimGroup`, `ScimPatchOp`
- [ ] Implement SCIM operations: list, get, create, update, patch, deactivate
- [ ] Implement SCIM server-side endpoints at `/scim/v2/` (Users, Groups, ServiceProviderConfig, ResourceTypes)
- [ ] SCIM filter syntax documentation
- [ ] Deactivation vs deletion semantics (deactivate preferred)
- [ ] Implement `sync_users()` with per-user error isolation (`SyncResult`/`SyncError`)
- [ ] Error handling: 19 error codes with HTTP status codes and JSON format
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
