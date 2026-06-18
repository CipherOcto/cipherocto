# Mission: 0945-a — User Management API

## Status

Open

## RFC

RFC-0945 (Economics): User Management API

## Context

LiteLLM has /user/new, /user/info, /user/update endpoints.

## Acceptance Criteria

- [ ] Add POST /user/new — create user
- [ ] Add GET /user/info — get user info
- [ ] Add POST /user/update — update user
- [ ] All endpoints require Management key type

## Files to Modify

- `crates/quota-router-core/src/admin.rs` — add user endpoints
