# Mission: 0932-a — Gateway Auth Wiring

## Status

Open

## RFC

RFC-0932 (Economics): Gateway Auth & API Key Management

## Dependencies

- Mission-0929-d: Wire DispatchInfo to Proxy (in progress)

## Context

The existing `KeyMiddleware` in `middleware.rs` implements key extraction, validation, budget checking, and rate limiting. But `proxy.rs` does NOT call it — requests pass through without authentication. This mission wires the existing middleware into the proxy request path.

## Acceptance Criteria

### Core Wiring

- [ ] `master_key` field already exists on `GatewayConfig` in `config.rs` — no creation needed
- [ ] Wire `KeyMiddleware::extract_and_validate()` into `proxy.rs::handle_request()`
- [ ] Wire `KeyMiddleware::validate_request_key_for_route()` for route permission checks
- [ ] Wire `KeyMiddleware::check_budget()` for budget enforcement
- [ ] Support existing header formats: `Authorization: Bearer` and `X-API-Key`
- [ ] Add `X-AnyLLM-Key` header support (new code in `extract_key_from_request()`)
- [ ] Master key bypasses all validation when configured (constant-time comparison)

### Error Handling

- [ ] Return 401 for `KeyError::MissingKey`, `NotFound`, `Expired`, `Revoked`
- [ ] Return 403 for `KeyError::RouteNotAllowed`, `BudgetExceeded`
- [ ] Return 429 for `KeyError::RateLimited { retry_after }`
- [ ] Error response format: `{"error": {"message": "...", "type": "...", "code": "..."}}`

### Management Endpoints

- [ ] `POST /v1/keys` — create key (requires Management key type)
- [ ] `GET /v1/keys` — list keys with pagination
- [ ] `DELETE /v1/keys/{id}` — revoke key
- [ ] `POST /v1/keys/{id}/rotate` — rotate key

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

**Management endpoints:** The `/v1/keys/*` endpoints specified in this mission are ADDITIONS to the proxy server, NOT replacements for the existing admin API at `/key/*` paths in `admin.rs`. Both will coexist — the admin API is for internal management, the proxy endpoints are for external API key management.
