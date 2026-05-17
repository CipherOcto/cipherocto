# Mission: 0949-b — OAuth2/OIDC

## Status

Open

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

- Mission-0949-a: SSO Core Infrastructure

## Acceptance Criteria

- [ ] Implement Authorization Code + PKCE flow (S256 mandatory)
- [ ] PKCE: code_verifier 43-128 chars, code_challenge = BASE64URL(SHA256(verifier))
- [ ] OAuth2 state parameter: `OAuth2State` struct with generation, validation, 5min expiry, nonce
- [ ] Implement Client Credentials flow
- [ ] Token lifecycle: access (1h), refresh (7d), session (30m) with refresh rotation
- [ ] Implement `POST /auth/token` — exchange code for tokens
- [ ] Implement `POST /auth/token/refresh` — refresh access token
- [ ] Implement `POST /auth/token/revoke` — revoke token (blacklist-based)
- [ ] Implement `POST /auth/token/introspect` — token introspection
- [ ] Implement provider management endpoints
- [ ] Integrate with virtual key system (RFC-0903)
- [ ] Rate limiting: 10/min login, 30/min refresh/revoke
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
