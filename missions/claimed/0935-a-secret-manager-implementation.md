# Mission: 0935-a — Secret Manager Implementation

## Status

Complete

Open

## RFC

RFC-0935 (Economics): Secret Manager Integration

## Dependencies

- Mission-0931-a: Env Var Syntax Implementation (complementary)
- Mission-0932-a: Gateway Auth Wiring (provides ApiKey context)
- Mission-0914-a: Stoolap Persistence (Open — for caching secrets; if not yet complete, define an InMemoryCache implementation using `HashMap<String, (String, Instant)>` as interim)

## Context

RFC-0935 specifies `SecretReader` and `SecretWriter` traits for integrating external secret managers (HashiCorp Vault, AWS Secrets Manager, OIDC). This mission implements the trait and basic backends.

## Acceptance Criteria

### Traits

- [x] `SecretReader` trait with `get_secret(&self, key: &str) -> Result<Option<String>, SecretError>`
- [x] `SecretWriter` trait with `set_secret()`, `delete_secret()`
- [x] `SecretManager` trait combining both
- [x] `SecretError` enum: NotFound, AccessDenied, NetworkError, ParseError — `ParseError` messages: For Vault deserialization failures, include the JSON path that failed and the serde error message.

### Implementations

- [x] `EnvSecretManager` — reads from `std::env::var()`, strips `os.environ/` prefix
- [x] `VaultSecretManager` — HashiCorp Vault integration
- [x] `AwsSecretManager` — AWS Secrets Manager integration
- [x] `OidcSecretManager` — OIDC token resolution (google, azure, env)

### Caching

- [x] `CachedSecretManager<T: SecretReader>` wrapper — uses `StoolapCache` trait (defined by Mission-0914-a). Interim implementation: `InMemoryCache` (HashMap-based). When Mission-0914-a completes, switch to stoolap-backed implementation.
- [x] Stoolap cache with configurable TTL
- [x] Cache format: `(key, value, expires_at)`

**Integration:** RFC-0938's `resolve_api_key()` calls `secret_reader.get_secret()` as the lowest-priority tier (after env vars). This mission provides the `SecretReader` backend; RFC-0938 provides the precedence chain.

### Tests

- [x] Env var lookup works
- [x] os.environ/ prefix stripping works
- [x] Cache hit returns cached value
- [x] Cache miss fetches from underlying manager
- [x] Cache TTL expiration works
- [x] Tests must cover: env var backend, Vault backend (mock), AWS Secrets Manager (mock), OIDC token exchange (mock). Use mockall or similar for external service mocking.

## Key Files

- `crates/quota-router-core/src/secret_manager.rs` — new file (traits + implementations)
- `crates/quota-router-core/src/config.rs` — secret_manager config

## Notes

This is a new module. The traits should be in `secret_manager.rs`. Implementations can be in submodules. The cache should use stoolap for persistence.

### Crate Dependencies

- `urlencoding` crate: Add to Cargo.toml (used for Vault KV v2 path encoding)
- `reqwest` crate features needed: `json`, `rustls-tls` (for Vault and OIDC HTTPS)
