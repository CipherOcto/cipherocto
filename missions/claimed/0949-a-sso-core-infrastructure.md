# Mission: 0949-a — SSO Core Infrastructure

## Status

Completed

## RFC

RFC-0949 (Economics): Enterprise SSO

## Dependencies

None

## Acceptance Criteria

### Core Types
- [x] Define `SsoFlow` enum (AuthorizationCode, ClientCredentials, Saml, Oidc)
- [x] Define `IdentityProvider` struct with validation rules table (Okta: client_id/client_secret/issuer/scopes, AzureAd: client_id/client_secret/issuer/scopes, Auth0: client_id/client_secret/issuer/scopes, GenericOidc: client_id/issuer/client_secret/scopes)
- [x] Define `TokenClaims` struct (sub, iss, aud, exp, iat, email, groups, roles)
- [x] Define `SsoKeyMetadata` extension for VirtualKey.metadata (sso_subject, sso_provider)
- [x] Implement `SsoKeyStorageExt` extension trait with `get_key_by_sso_subject()`
- [x] Implement `SsoKeyMapper` — maps SSO user to virtual API key via user.sub
- [x] Implement `map_role()` method on SsoKeyMapper with role_mapping config

### JWT Validation
- [x] Implement JWT validation with JWKS caching (jwks_cache_ttl: 3600 seconds)
- [x] Implement clock skew tolerance (clock_skew: 30 seconds)
- [x] Validate audience claim (error: `sso_audience_mismatch`)
- [x] Validate issuer claim (error: `sso_issuer_mismatch`)
- [x] JWT algorithms: RS256, RS384, RS512, ES256, ES384, PS256 (reject alg=none)
- [x] Implement `TokenBlacklistStorage` trait (add, contains, cleanup_expired)

### Error Handling
- [x] Implement all 18 SSO error codes (sso_provider_not_found, sso_provider_disabled, sso_invalid_state, sso_invalid_code, sso_token_expired, sso_token_revoked, sso_token_invalid, sso_token_algorithm_unsupported, sso_token_algorithm_none, sso_audience_mismatch, sso_issuer_mismatch, sso_saml_signature_invalid, sso_saml_assertion_expired, sso_saml_audience_mismatch, sso_no_key_mapping, sso_user_deactivated, sso_provider_error, sso_rate_limited)
- [x] Error response format: `{"error": {"message": "...", "type": "...", "code": "...", "status_code": ...}}`

### Configuration
- [x] Add `SsoConfig` to `config.rs` (providers, role/team mapping, token, JWT, rate_limit)
- [x] JWT config: jwks_cache_ttl (default 3600), clock_skew (default 30), supported_algorithms
- [x] Token config: access_token_ttl (1h), refresh_token_ttl (7d), session_ttl (30m)

### Production Storage Backends (Secondary Gaps — Completed)
- [x] Implement `StoolapTokenBlacklistStorage` — persistent token blacklist via stoolap
- [x] Implement `SsoKeyStorageExt` on `StoolapKeyStorage` — production SSO key storage
- [x] Wire token blacklist into OAuth2FlowHandler for logout/revocation support
- [x] Add `token_blacklist` table to `schema.rs` initialization

### Verification
- [x] Clippy passes with zero warnings
- [x] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/auth/sso/mod.rs` — SSO core types and errors
- `crates/quota-router-core/src/auth/sso/jwt.rs` — JWT validation with JWKS caching
- `crates/quota-router-core/src/auth/sso/mapper.rs` — SSO-to-API-key mapping
- `crates/quota-router-core/src/auth/sso/blacklist.rs` — Token blacklist trait + in-memory impl
- `crates/quota-router-core/src/auth/sso/blacklist_stoolap.rs` — Production TokenBlacklistStorage via stoolap
- `crates/quota-router-core/src/auth/sso/mapper_stoolap.rs` — Production SsoKeyStorageExt via stoolap
- `crates/quota-router-core/src/auth/sso/oauth2.rs` — OAuth2 flow handler with id_token decoding and blacklist revocation
- `crates/quota-router-core/src/config.rs` — SsoConfig
- `crates/quota-router-core/src/schema.rs` — token_blacklist table
- `crates/quota-router-core/src/keys/models.rs` — KeyType::Sso variant
- `crates/quota-router-core/src/storage.rs` — KeyUpdates.metadata field

Production storage backends completed:
1. `blacklist_stoolap.rs` — ✅ production TokenBlacklistStorage via stoolap
2. `mapper_stoolap.rs` — ✅ production SsoKeyStorageExt wrapping StoolapKeyStorage
3. Schema update to add `token_blacklist` table — ✅
4. Wire both into config and OAuth2FlowHandler — ✅ (revoke_with_blacklist method)
