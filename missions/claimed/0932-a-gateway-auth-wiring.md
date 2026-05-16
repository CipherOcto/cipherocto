# Mission: 0932-a — Gateway Auth Wiring

## Status

Open

## RFC

RFC-0932 (Economics): Gateway Auth & API Key Management

## Dependencies

- Mission-0929-d: Wire DispatchInfo to Proxy (in progress) — must be complete before this mission can be fully tested (proxy dispatch wiring)

### Crate Dependencies

- `subtle` crate: Add to Cargo.toml for constant-time comparison of master key bypass (timing attack prevention)

## Context

The existing `KeyMiddleware` in `middleware.rs` implements key extraction, validation, budget checking, and rate limiting. But `proxy.rs` does NOT call it — requests pass through without authentication. This mission wires the existing middleware into the proxy request path.

## Acceptance Criteria

### Core Wiring

- [ ] `master_key` field already exists on `GatewayConfig` in `config.rs` — no creation needed
- [ ] Wire `KeyMiddleware::extract_and_validate()` into `proxy.rs::handle_request()`
- [ ] Wire `KeyMiddleware::validate_request_key_for_route()` for route permission checks
- [ ] Wire `KeyMiddleware::check_budget()` for budget enforcement
- [ ] Support existing header formats: `Authorization: Bearer` and `X-API-Key`
- [ ] Add `X-AnyLLM-Key` header support — Implementation: Add X-AnyLLM-Key header extraction to `extract_key_from_request()` in middleware.rs. This is a new header, not the existing X-API-Key.
- [ ] Master key bypasses all validation when configured (constant-time comparison using `subtle` crate)

**Note:** Master key bypass skips ALL validation including rate limits. This is intentional — master key is for administrative access only.

### Error Handling

- [ ] Return 401 for `KeyError::MissingKey`, `NotFound`, `Expired`, `Revoked`
- [ ] Return 403 for `KeyError::RouteNotAllowed`, `BudgetExceeded` — `KeyError::BudgetExceeded { current, limit }` serializes to JSON: `{"error": "budget_exceeded", "current": <number>, "limit": <number>}`
- [ ] Return 429 for `KeyError::RateLimited { retry_after }`
- [ ] Error response format: `{"error": {"message": "...", "type": "...", "code": "..."}}`

### Management Endpoints

- [ ] `POST /key/generate` — create key (requires Management key type, matches existing admin.rs)
- [ ] `GET /key/list` — list keys with pagination (matches existing admin.rs)
- [ ] `DELETE /key/{id}` — revoke key (matches existing admin.rs)
- [ ] `POST /key/{id}/regenerate` — rotate key (matches existing admin.rs)
- [ ] `GET /team/list` — list teams (NEW endpoint — not in existing admin.rs). Note: GET /team/list is a NEW endpoint that does not exist in admin.rs. The existing admin team routes are POST /team, GET /team/:team_id, PUT /team/:team_id.

**Note:** Budget endpoints (`GET /budget/{entity_type}/{entity_id}` etc.) belong to Mission-0934-a, not this mission. This mission only wires auth for existing admin.rs endpoints (`/key/*`, `/team/*`).

### Tests

- [ ] Valid LlmApi key → 200 on /v1/chat/completions
- [ ] ReadOnly key on POST → 403
- [ ] Revoked key → 401
- [ ] Expired key → 401
- [ ] Master key bypasses all checks
- [ ] Missing key → 401
- [ ] All three header formats work (Authorization: Bearer, X-API-Key, X-AnyLLM-Key)
- [ ] Management endpoints require Management key type

## Key Files

- `crates/quota-router-core/src/proxy.rs` — main request handler
- `crates/quota-router-core/src/middleware.rs` — existing KeyMiddleware
- `crates/quota-router-core/src/keys/mod.rs` — key validation functions
- `crates/quota-router-core/src/keys/models.rs` — ApiKey struct
- `crates/quota-router-core/src/config.rs` — GatewayConfig (master_key field already exists)
- `crates/quota-router-core/src/admin.rs` — existing admin API at /key/* paths

## Notes

The existing `KeyMiddleware` in `middleware.rs` already implements all the hard work (hash lookup, expiry check, budget check, rate limits). This mission is primarily about wiring it into the proxy request path.

**Management endpoints:** The `/key/*` endpoints in this mission match the existing admin.rs paths. The proxy server should route these paths to the same handler functions as admin.rs, extending them with auth middleware validation. This is an integration of existing admin endpoints into the proxy's auth flow, not a separate set of endpoints.
