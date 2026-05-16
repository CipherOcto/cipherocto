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

- [x] `master_key` field already exists on `GatewayConfig` in `config.rs` — no creation needed
- [x] Wire key validation into `proxy.rs::handle_request()` — uses `KeyStorage::lookup_by_hash()` directly (avoids generic KeyMiddleware complexity)
- [ ] Wire `KeyMiddleware::validate_request_key_for_route()` for route permission checks (deferred to 0933)
- [ ] Wire `KeyMiddleware::check_budget()` for budget enforcement (deferred to 0934)
- [x] Support existing header formats: `Authorization: Bearer` and `X-API-Key`
- [x] Add `X-AnyLLM-Key` header support — added to both middleware.rs and proxy.rs extract_client_key()
- [x] Master key bypasses all validation when configured (constant-time comparison using `subtle` crate)

**Note:** Master key bypass skips ALL validation including rate limits. This is intentional — master key is for administrative access only.

### Error Handling

- [x] Return 401 for missing key or key not found
- [ ] Return 403 for `KeyError::RouteNotAllowed`, `BudgetExceeded` (deferred to 0933/0934)
- [ ] Return 429 for `KeyError::RateLimited { retry_after }` (deferred to 0933)
- [ ] Error response format: `{"error": {"message": "...", "type": "...", "code": "..."}}` (deferred — current format is plain text)

### Management Endpoints

- [x] `POST /key/generate` — exists in admin.rs (admin server has no auth, separate from proxy)
- [x] `GET /key/list` — exists in admin.rs
- [x] `DELETE /key/{id}` — exists in admin.rs
- [x] `POST /key/{id}/regenerate` — exists in admin.rs
- [ ] `GET /team/list` — NEW endpoint (deferred — admin.rs team endpoints exist, list endpoint is new)

> **Note:** Management endpoints are served by AdminServer (separate HTTP server on admin_port). Route permission checks (Management key type requirement) are deferred to Mission 0933-a.
- [ ] `GET /team/list` — list teams (NEW endpoint — not in existing admin.rs). Note: GET /team/list is a NEW endpoint that does not exist in admin.rs. The existing admin team routes are POST /team, GET /team/:team_id, PUT /team/:team_id.

**Note:** Budget endpoints (`GET /budget/{entity_type}/{entity_id}` etc.) belong to Mission-0934-a, not this mission. This mission only wires auth for existing admin.rs endpoints (`/key/*`, `/team/*`).

### Tests

- [x] Master key bypasses all checks (constant-time comparison with `subtle`)
- [x] Missing key → 401
- [x] Key not found → 401
- [x] All three header formats work (Authorization: Bearer, X-API-Key, X-AnyLLM-Key)
- [ ] Valid LlmApi key → 200 on /v1/chat/completions (integration test, deferred)
- [ ] ReadOnly key on POST → 403 (route permission checks, deferred to 0933)
- [ ] Revoked key → 401 (deferred — requires full key lifecycle)
- [ ] Expired key → 401 (deferred — requires full key lifecycle)
- [ ] Management endpoints require Management key type (deferred to 0933)

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
