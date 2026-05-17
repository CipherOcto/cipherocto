# Mission: 0943-a — Per-User Rate Limits

## Status

Open

## RFC

RFC-0943 (Economics): Per-User/Team Rate Limits & Budgets

## Dependencies

- Mission 0933-a: Rate Limiting (COMPLETE — 9dac71a)

## Context

RFC-0933 supports per-key rate limits. This mission extends it to per-user rate limits using the `user` field from request body or `X-User-Id` header.

## Acceptance Criteria

- [ ] Extract user from request body `user` field
- [ ] Also support `X-User-Id` header as fallback
- [ ] Apply per-user RPM/TPM limits
- [ ] Return 429 with Retry-After when exceeded

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — extract user and check limits
