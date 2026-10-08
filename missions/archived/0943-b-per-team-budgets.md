# Mission: 0943-b — Per-Team Budgets

## Status

Open

## RFC

RFC-0943 (Economics)

> **Numbering note.** RFC-0943 was never authored. Git history contains no
> RFC file with this number under any category, so this mission was filed
> against a specification that does not exist. No current RFC carries this
> title, so the governing specification is unconfirmed. The mission body,
> not any RFC, is the only surviving record of the intended scope.
> : Per-User/Team Rate Limits & Budgets

## Dependencies

- Mission 0934-a: Budget & Spend Tracking (COMPLETE — 9dac71a)

## Context

RFC-0934 supports per-key budgets. This mission extends it to per-team budgets using the team_id from the API key.

## Acceptance Criteria

- [ ] Extract team_id from validated API key
- [ ] Check team budget before allowing request
- [ ] Return 403 with budget_exceeded error when exceeded
- [ ] Include current/limit in error response

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — check team budget
