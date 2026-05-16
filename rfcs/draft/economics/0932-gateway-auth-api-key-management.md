# RFC-0932: Gateway Auth & API Key Management

## Status: Draft

## Summary

Wire the existing API key management infrastructure (keys/ module) into the proxy request path, enabling auth for both litellm-mode and any-llm-mode. This RFC specifies how the proxy authenticates requests using API keys, validates permissions, and enforces access control.

## Motivation

quota-router has a full API key management infrastructure:
- Key generation with HMAC-SHA256
- Key types: Default, LlmApi, Management, ReadOnly
- Budget limits, RPM/TPM limits per key
- Revocation, rotation, team association
- AdminServer for key management

But the proxy doesn't use any of it. Requests pass through without authentication. This RFC wires the existing infrastructure into the proxy.

## Specification

### 1. Auth Middleware

```rust
// proxy.rs - integrate existing KeyMiddleware
// Reuse existing KeyMiddleware from middleware.rs — do NOT create new auth logic

// 1. Extract key from request (existing: extract_key_from_request)
// 2. Validate key (existing: validate_request_key — hash lookup, expiry, revoked check)
// 3. Check route permission (existing: validate_request_key_for_route — uses allowed_routes)
// 4. Check budget (existing: check_budget — compares spend vs budget_limit)
// 5. Check rate limits (RFC-0933: check_rpm_limit pre-request, check_tpm_limit post-request)
// 6. Inject ApiKey into request extensions (reuse existing ApiKey struct)

// Master key bypass: constant-time comparison to prevent timing attacks
// Use subtle::ConstantTimeEq or HMAC-based comparison
// if let Some(ref mk) = config.master_key {
//     if !mk.is_empty() && constant_time_eq(mk.as_bytes(), api_key.as_bytes()) {
//         warn!("Master key used — audit log");
//         return next.run(request).await;
//     }
// }
```

**Implementation note:** The existing `KeyMiddleware` in `middleware.rs` already implements:
- `extract_key_from_request()` — supports `Authorization: Bearer` and `X-API-Key`
- `validate_request_key()` — hash-based lookup via `KeyStorage::lookup_by_hash()`
- `validate_request_key_for_route()` — adds `allowed_routes` permission check
- `check_budget()` — compares spend against `budget_limit`
- `check_rate_limits()` — RPM/TPM via `RateLimiterStore`

The RFC should wire these existing methods into the proxy, not create new ones.

### 2. API Key Header

Support three header formats:
- `Authorization: Bearer sk-xxx` (LiteLLM style)
- `X-API-Key: sk-xxx` (existing middleware style)
- `X-AnyLLM-Key: sk-xxx` (any-llm style — **requires extending `extract_key_from_request()`**)

Priority: Authorization > X-API-Key > X-AnyLLM-Key

**Note:** The existing `extract_key_from_request()` in `middleware.rs` only supports `Authorization: Bearer` and `X-API-Key`. Adding `X-AnyLLM-Key` requires modifying this function — it is NOT just wiring existing code.

### 3. Key Lookup Priority

1. Master key (config.master_key) → full access, skip all validation
2. Key storage lookup by hash (existing `KeyStorage::lookup_by_hash()`) → permission-based access
3. No key → 401 Unauthorized

**Note:** Existing code uses hash-based lookup (`compute_key_hash()` then `lookup_by_hash()`), NOT prefix-based lookup.

### 4. Endpoint Permissions

Use existing `check_route_permission()` from `keys/mod.rs` which checks `ApiKey::allowed_routes` field.

Default route permissions by KeyType (matches existing `check_route_permission()` in keys/mod.rs):

| Endpoint | LlmApi | Management | ReadOnly |
|----------|--------|------------|----------|
| /v1/chat/ | ✓ | ✗ | ✗ |
| /v1/completions/ | ✓ | ✗ | ✗ |
| /v1/embeddings/ | ✓ | ✗ | ✗ |
| /models/, /info | ✓ | ✓ | ✓ |
| /key/, /team/, /user/ | ✗ | ✓ | ✗ |

**Note:** `allowed_routes` field on ApiKey can override defaults. Route matching uses prefix matching. Existing code uses paths WITHOUT `/v1/` prefix for management routes. Management keys have access to `/key/`, `/team/`, `/user/` — NOT to LLM endpoints. ReadOnly keys only have access to `/models/` and `/info`.

### 5. Management Endpoints

Expose existing AdminServer functionality via REST:

**POST /v1/keys** — create key
```json
// Request
{
  "key_type": "LlmApi",
  "team_id": "...",
  "budget_limit": 10000,
  "rpm_limit": 100,
  "tpm_limit": 10000,
  "expires_at": "2026-12-31T00:00:00Z",
  "allowed_routes": "/v1/chat,/v1/embeddings",
  "description": "Production key"
}
// Response: { "key_id": "...", "key": "sk-qr-...", "key_prefix": "sk-qr-..." }
```

**GET /v1/keys** — list keys (with pagination)
```json
// Query: ?team_id=...&key_type=LlmApi&limit=100&offset=0
// Response: { "keys": [...], "total": 42 }
```

**DELETE /v1/keys/{id}** — revoke key
```json
// Request: { "reason": "Compromised" }
// Response: { "key_id": "...", "revoked_at": "...", "revoked_by": "..." }
```

**POST /v1/keys/{id}/rotate** — rotate key
```json
// Response: { "key_id": "...", "new_key": "sk-qr-...", "old_key_revoked_at": "..." }
```

**GET /v1/users** — list users
```json
// Query: ?limit=100&offset=0
// Response: { "users": [...], "total": 10 }
```

**GET /v1/budgets/{entity_type}/{entity_id}** — get budget

### 6. Error Responses

Use existing `KeyError` enum from `keys/mod.rs`:

| KeyError variant | HTTP status | Error code | Fields |
|------------------|-------------|------------|--------|
| `MissingKey` | 401 | `missing_api_key` | — |
| `NotFound` | 401 | `invalid_api_key` | — |
| `Expired(i64)` | 401 | `key_expired` | expiry timestamp |
| `Revoked(String)` | 401 | `key_revoked` | reason |
| `RouteNotAllowed(String)` | 403 | `route_not_allowed` | route path |
| `BudgetExceeded { current, limit }` | 403 | `budget_exceeded` | current: u64, limit: u64 |
| `TeamBudgetExceeded { current, limit }` | 403 | `team_budget_exceeded` | current: u64, limit: u64 |
| `TeamKeyLimitExceeded { current, limit }` | 403 | `team_key_limit_exceeded` | current: u32, limit: u32 |
| `RateLimited { retry_after }` | 429 | `rate_limit_exceeded` | retry_after: u64 |
| `InvalidFormat` | 400 | `invalid_format` | — |
| `AlreadyExists` | 409 | `already_exists` | — |
| `Storage(String)` | 500 | `storage_error` | error message |

**Note:** `Expired` carries timestamp, `Revoked` carries reason, `RouteNotAllowed` carries path, `BudgetExceeded`/`TeamBudgetExceeded` carry current/limit values.

```json
{
  "error": {
    "message": "Invalid API key",
    "type": "authentication_error",
    "code": "invalid_api_key"
  }
}
```

## Dependencies

- RFC-0903: Virtual API Key System (key storage schema)
- RFC-0914: stoolap-only persistence (key storage backend)
- RFC-0933: Rate Limiting Integration (check_rate_limits called in auth flow)
- RFC-0934: Budget Management & Spend Tracking (check_budget called in auth flow)

## Test Plan

1. Valid LlmApi key → 200 on /v1/chat/completions
2. ReadOnly key on POST → 403
3. Revoked key → 401 (KeyError::Revoked)
4. Expired key → 401 (KeyError::Expired)
5. Master key bypasses all checks
6. Missing key → 401 (KeyError::MissingKey)
7. Management endpoints require Management key type
8. Key rotation invalidates old key
9. All three header formats work (Authorization, X-API-Key, X-AnyLLM-Key)
10. Route permission check works (allowed_routes field)
11. Budget exceeded → 403 (KeyError::BudgetExceeded)
12. Rate limit exceeded → 429 (KeyError::RateLimited { retry_after })
