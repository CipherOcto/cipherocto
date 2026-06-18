# Mission: 0932-b — Team Management API

## Status

Open

## RFC

RFC-0932 (Economics): Gateway Auth API Key Management

## Context

LiteLLM has /team/new, /team/info, /team/update, /team/list, /team/member_add, /team/member_delete.

## Acceptance Criteria

- [ ] Add GET /team/list — list all teams
- [ ] Add POST /team/member_add — add team member
- [ ] Add POST /team/member_delete — remove team member
- [ ] All endpoints require Management key type

## Files to Modify

- `crates/quota-router-core/src/admin.rs` — add team endpoints
