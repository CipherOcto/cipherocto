# Mission: 0949-b — OAuth2/OIDC

## Status

Open

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- Mission-0949-a: SSO Core Infrastructure

## Acceptance Criteria

### OAuth2 Flows
- [ ] Implement Authorization Code + PKCE flow (S256 mandatory)
- [ ] PKCE: code_verifier 43-128 chars, code_challenge = BASE64URL(SHA256(verifier))
- [ ] OAuth2 state parameter: `OAuth2State` struct with generation, validation, 5min expiry, nonce
- [ ] Implement Client Credentials flow

### Token Lifecycle
- [ ] Token lifecycle: access (1h), refresh (7d), session (30m) with refresh rotation
- [ ] Refresh token rotation on use (old revoked, new issued)

### SSO Flow Endpoints
- [ ] Implement `GET /auth/sso/:provider` — Initiate SSO flow (generates state + PKCE challenge)
- [ ] Implement `GET /auth/sso/:provider/callback` — OAuth2 callback (validates state, exchanges code)

### Token Endpoints
- [ ] Implement `POST /auth/token` — exchange code for tokens (sends code_verifier for PKCE)
- [ ] Implement `POST /auth/token/refresh` — refresh access token
- [ ] Implement `POST /auth/token/revoke` — revoke token (blacklist-based)
- [ ] Implement `POST /auth/token/introspect` — token introspection (for resource servers)

### OIDC Endpoints
- [ ] Implement `GET /.well-known/openid-configuration` — OIDC discovery
- [ ] Implement `GET /auth/jwks` — JWKS endpoint for token validation by resource servers

### User Endpoints
- [ ] Implement `GET /auth/userinfo` — return current user info
- [ ] Implement `GET /auth/userinfo/claims` — return token claims

### Provider Management
- [ ] Implement `GET /auth/providers` — List providers
- [ ] Implement `POST /auth/providers` — Add provider
- [ ] Implement `PUT /auth/providers/:id` — Update provider
- [ ] Implement `DELETE /auth/providers/:id` — Delete provider

### Logout
- [ ] Implement `POST /auth/logout` — Logout: revoke session, clear cookies (this endpoint handles both OAuth2 and SAML SLO — single owner: 0949-b)

### Rate Limiting
- [ ] Rate limiting: 10/min login (per IP), 30/min refresh/revoke (per user)
- [ ] Rate limiting: 20/min callback (per IP)
- [ ] Integrate with RFC-0933 rate limiting system

### Integration
- [ ] Integrate with virtual key system (RFC-0903)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/auth/sso/oauth2.rs` — New
- `crates/quota-router-core/src/auth/sso/pkce.rs` — New
- `crates/quota-router-core/src/admin.rs` — Add auth endpoints
