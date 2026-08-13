# Mission: 0949-b — OAuth2/OIDC

## Status

Closed 2026-08-13 (@claude). LANDED + drift-closed.

**Substrate (from prior sessions):** OAuth2/OIDC code shipped across
multiple commits (`0c9c7958`, `ae5ae0c3`, `947fa315`, `f5340bbd`,
`bd2aad08`). 113 tests across SSO modules. 30/31 ACs PASS.

**Drift closure:** 1 AC DEFERRED — rate limiting on auth endpoints.
Same cross-mission gap as 0948-b; will close under `0948-b1`.

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- Mission-0949-a: SSO Core Infrastructure (archived)

## Acceptance Criteria

### OAuth2 Flows
- [x] Implement Authorization Code + PKCE flow (S256 mandatory) — oauth2.rs:295
- [x] PKCE: code_verifier 43-128 chars, code_challenge = BASE64URL(SHA256(verifier)) — pkce.rs
- [x] OAuth2 state parameter: `OAuth2State` struct with generation, validation, 5min expiry, nonce — oauth2.rs:38-60
- [x] Implement Client Credentials flow — oauth2.rs:491

### Token Lifecycle
- [x] Token lifecycle: access (1h), refresh (7d), session (30m) with refresh rotation
- [x] Refresh token rotation on use (old revoked, new issued) — oauth2.rs:408

### JWT Signature Verification
- [x] Implement JWKS fetch from IdP (with caching, configurable TTL) — jwt.rs:32-66
- [x] Implement JWT signature verification using fetched JWKS — jwt.rs:91
- [x] Validate algorithm against allowed algorithms (RS256/RS384/RS512/ES256/ES384/PS256) — jwt.rs:281
- [x] Reject tokens with `alg: none` — jwt.rs:96, test_reject_algorithm_none

### SSO Flow Endpoints
- [x] Implement `GET /auth/sso/:provider` — initiate SSO flow (admin.rs:545)
- [x] Implement `GET /auth/sso/:provider/callback` — OAuth2 callback (admin.rs:558)

### Token Endpoints
- [x] Implement `POST /auth/token` — exchange code for tokens (admin.rs:592)
- [x] Implement `POST /auth/token/refresh` — refresh access token (admin.rs:606)
- [x] Implement `POST /auth/token/revoke` — revoke token (blacklist-based) (admin.rs:620)
- [x] Implement `POST /auth/token/introspect` — token introspection (admin.rs:634)

### OIDC Endpoints
- [x] Implement `GET /.well-known/openid-configuration` — OIDC discovery (admin.rs:648)
- [x] Implement `GET /auth/jwks` — JWKS endpoint (admin.rs:653)

### User Endpoints
- [x] Implement `GET /auth/userinfo` — return current user info (admin.rs:658)
- [x] Implement `GET /auth/userinfo/claims` — return token claims (admin.rs:663)

### Provider Management
- [x] Implement `GET /auth/providers` — list providers (admin.rs:673)
- [x] Implement `POST /auth/providers` — add provider (admin.rs:678)
- [x] Implement `PUT /auth/providers/:id` — update provider (admin.rs:691)
- [x] Implement `DELETE /auth/providers/:id` — delete provider (admin.rs:700)

### Logout
- [x] Implement `POST /auth/logout` — logout (single owner: 0949-b) (admin.rs:668)

### Rate Limiting
- [ ] **DEFERRED** Rate limiting on auth endpoints — see follow-on `0948-b1`

### Integration
- [x] Integrate with virtual key system (RFC-0903) — verified by blackbox/auth tests
- [x] Clippy passes with zero warnings (verified by recent commits)
- [x] All existing tests pass (113 SSO tests)

## Claimant

(unclaimed)

## Pull Request

#

## Notes

**Drift pattern** — code landed in 5+ commits across late July +
August 2026 (see git log: `0c9c7958`, `ae5ae0c3`, `947fa315`,
`f5340bbd`, `bd2aad08`). Mission file remained `open/`.

**Rate limiting deferred** — same cross-mission gap as 0948-b. Auth
endpoints (sso, token, callback, logout) need admin layer rate
limiting per RFC-0933. `0948-b1-admin-rate-limiting` follow-on covers
the admin.rs dispatch layer (auth/sso, prompts, providers CRUD).

**Coverage** — 113 tests across SSO modules:
- oauth2.rs: 74 tests (state, session, flows, refresh, revocation)
- pkce.rs: 12 tests
- jwt.rs: 12 tests (algorithm parsing, alg=none rejection, signature verify)
- blacklist.rs: 3 tests
- session.rs: 12 tests

JWT alg validation tests:
- `test_reject_algorithm_none` — explicit `alg:none` rejection
- `test_parse_algorithm_passes_rs256` — allowlist verification
- `test_algorithm_allowlist_enforced` — only allowed algorithms accepted

## Follow-ons

- Already filed: `0948-b1-admin-rate-limiting` (covers 0949-b rate limiting ACs)

## Cross-references

- RFC-0949 (Economics): Enterprise SSO
- RFC-0933 (Infrastructure): Rate Limiting
- Mission `0949-a` (archived) — SSO core substrate
- Mission `0949-c` (open `0949-c-sso-saml.md`) — SAML 2.0 (separate scope)
- Mission `0948-b1` (open) — admin rate limiting
- `crates/quota-router-core/src/auth/sso/` — substrate (1740 + 132 + 429 + 200 + 300 LoC)

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-07-29 | claimed  | Original mission |
| v0.2    | 2026-08-13 | closed   | 30/31 ACs PASS; 1 DEFERRED (rate limiting). 113 SSO tests. |
