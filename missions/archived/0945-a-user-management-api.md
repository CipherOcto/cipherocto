# Mission: 0945-a — User Management API

## Status

Open

## RFC

RFC-0945 (Economics)

> **Numbering note.** RFC-0945 was never authored. Git history contains no
> RFC file with this number under any category, so this mission was filed
> against a specification that does not exist. No current RFC carries this
> title, so the governing specification is unconfirmed. The mission body,
> not any RFC, is the only surviving record of the intended scope.
> : User Management API

## Context

LiteLLM has /user/new, /user/info, /user/update endpoints.

## Acceptance Criteria

- [ ] Add POST /user/new — create user
- [ ] Add GET /user/info — get user info
- [ ] Add POST /user/update — update user
- [ ] All endpoints require Management key type

## Files to Modify

- `crates/quota-router-core/src/admin.rs` — add user endpoints
