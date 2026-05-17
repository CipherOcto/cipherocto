# Mission: 0949-a — SSO Core Infrastructure

## Status

Completed

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

None

## Acceptance Criteria

### Core Types
- [ ] Define `SsoFlow` enum (AuthorizationCode, ClientCredentials, Saml, Oidc)
- [ ] Define `IdentityProvider` struct with validation rules table (Okta: client_id/client_secret/issuer/scopes, AzureAd: client_id/client_secret/issuer/scopes, Auth0: client_id/client_secret/issuer/scopes, GenericOidc: client_id/issuer/client_secret/scopes)
- [ ] Define `TokenClaims` struct (sub, iss, aud, exp, iat, email, groups, roles)
- [ ] Define `SsoKeyMetadata` extension for VirtualKey.metadata (sso_subject, sso_provider)
- [ ] Implement `SsoKeyStorageExt` extension trait with `get_key_by_sso_subject()`
- [ ] Implement `SsoKeyMapper` — maps SSO user to virtual API key via user.sub
- [ ] Implement `map_role()` method on SsoKeyMapper with role_mapping config

### JWT Validation
- [ ] Implement JWT validation with JWKS caching (jwks_cache_ttl: 3600 seconds)
- [ ] Implement clock skew tolerance (clock_skew: 30 seconds)
- [ ] Validate audience claim (error: `sso_audience_mismatch`)
- [ ] Validate issuer claim (error: `sso_issuer_mismatch`)
- [ ] JWT algorithms: RS256, RS384, RS512, ES256, ES384, PS256 (reject alg=none)
- [ ] Implement `TokenBlacklistStorage` trait (add, contains, cleanup_expired)

### Error Handling
- [ ] Implement all 18 SSO error codes (sso_provider_not_found, sso_provider_disabled, sso_invalid_state, sso_invalid_code, sso_token_expired, sso_token_revoked, sso_token_invalid, sso_token_algorithm_unsupported, sso_token_algorithm_none, sso_audience_mismatch, sso_issuer_mismatch, sso_saml_signature_invalid, sso_saml_assertion_expired, sso_saml_audience_mismatch, sso_no_key_mapping, sso_user_deactivated, sso_provider_error, sso_rate_limited)
- [ ] Error response format: `{"error": {"message": "...", "type": "...", "code": "...", "status_code": ...}}`

### Configuration
- [ ] Add `SsoConfig` to `config.rs` (providers, role/team mapping, token, JWT, rate_limit)
- [ ] JWT config: jwks_cache_ttl (default 3600), clock_skew (default 30), supported_algorithms
- [ ] Token config: access_token_ttl (1h), refresh_token_ttl (7d), session_ttl (30m)

### Verification
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
- `crates/quota-router-core/src/auth/sso/blacklist.rs` — New
- `crates/quota-router-core/src/config.rs` — Add SsoConfig
