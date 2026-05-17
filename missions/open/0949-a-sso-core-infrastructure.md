# Mission: 0949-a — SSO Core Infrastructure

## Status

Open

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

None

## Acceptance Criteria

- [ ] Define `SsoFlow` enum (AuthorizationCode, ClientCredentials, Saml, Oidc)
- [ ] Define `IdentityProvider` struct with validation rules table
- [ ] Define `TokenClaims` struct (sub, iss, aud, exp, iat, email, groups, roles)
- [ ] Define `SsoKeyMetadata` extension for VirtualKey.metadata (sso_subject, sso_provider)
- [ ] Implement `SsoKeyStorageExt` extension trait with `get_key_by_sso_subject()`
- [ ] Implement `SsoKeyMapper` — maps SSO user to virtual API key via user.sub
- [ ] Implement `map_role()` method on SsoKeyMapper with role_mapping config
- [ ] Implement JWT validation with JWKS caching
- [ ] JWT algorithms: RS256, RS384, RS512, ES256, ES384, PS256 (reject alg=none)
- [ ] Implement `TokenBlacklistStorage` trait (add, contains, cleanup_expired)
- [ ] Add `SsoConfig` to `config.rs` (providers, role/team mapping, token, JWT, rate_limit)
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/auth/sso/mod.rs` — New
- `crates/quota-router-core/src/auth/sso/mapper.rs` — New
- `crates/quota-router-core/src/auth/sso/jwt.rs` — New
- `crates/quota-router-core/src/config.rs` — Add SsoConfig
