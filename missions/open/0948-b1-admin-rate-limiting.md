# Mission: 0948-b1 — Admin Endpoint Rate Limiting

## Status

Open. Follow-on from 0948-b (commit `9ae79d1d`) drift closure
(2026-08-13). Cross-mission: also unblocks 0949-b rate limiting ACs.

## RFC

RFC-0933 (Infrastructure): Rate Limiting
RFC-0948 (Economics): Prompt Management
RFC-0949 (Economics): Enterprise SSO

## Dependencies

- 0948-b drift closure (2026-08-13)
- 0949-b drift closure (when filed)
- `crates/quota-router-core/src/rate_limit/` substrate (already exists)

## Acceptance Criteria

- [ ] Define `AdminRateLimit` policy struct per RFC-0933 §Admin endpoints:
  - [ ] `POST /auth/sso/:provider` (SSO init): 10/min per IP
  - [ ] `GET /auth/sso/:provider/callback`: 20/min per IP
  - [ ] `POST /auth/token`: 30/min per user
  - [ ] `POST /auth/token/refresh`: 30/min per user
  - [ ] `POST /auth/token/revoke`: 30/min per user
  - [ ] `POST /auth/token/introspect`: 60/min per user
  - [ ] `POST /auth/logout`: 10/min per user
  - [ ] `POST /prompts`, `PUT /prompts/:id`, `DELETE /prompts/:id`: 60/min per API key
  - [ ] `POST /prompts/:id/versions`, `POST /prompts/:id/rollback`, `POST /prompts/:id/versions/:v/activate`: 30/min per API key
  - [ ] `POST/PUT/DELETE /auth/providers`: 60/min per API key
- [ ] Wire `check_admin_rate_limit(&Request, &Policy)` middleware into admin.rs dispatch
- [ ] Return `429 Too Many Requests` with `Retry-After` header on limit hit
- [ ] Per-IP: hash IP + path into ratelimit key
- [ ] Per-user: hash user_id + path into ratelimit key
- [ ] Per-API-key: hash api_key_id (first 8 chars) + path into ratelimit key
- [ ] Cfg-able via `cfg.rate_limit.admin.enabled: bool` (default true)
- [ ] Add ≥5 tests: per-IP limit hit, per-user limit hit, per-key limit hit, 429 + Retry-After header, breach across endpoints is independent
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/admin.rs` — dispatch + middleware
- `crates/quota-router-core/src/rate_limit/admin.rs` (NEW) — policy struct + middleware
- `crates/quota-router-core/src/lib.rs` — register new module

Drift context: 0948-b + 0949-b both list rate limiting as AC without
implementation. The rate limiter substrate (`rate_limit/mod.rs` +
`key_rate_limiter.rs`) exists but is wired only at the proxy layer
(chat completion path). Admin layer is unprotected.

## Version History

| Version | Date       | Change                                                                                                |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-13 | Mission filed. 0948-b drift closure follow-on. 13 ACs. |

Last Updated: 2026-08-13
Version: 0.1
