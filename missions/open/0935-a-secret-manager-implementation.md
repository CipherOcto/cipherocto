# Mission: 0935-a — Secret Manager Implementation

## Status

Open

## RFC

RFC-0935 (Economics): Secret Manager Integration

## Dependencies

- Mission-0931-a: Env Var Syntax Implementation (complementary)

## Context

RFC-0935 specifies `SecretReader` and `SecretWriter` traits for integrating external secret managers (HashiCorp Vault, AWS Secrets Manager, OIDC). This mission implements the trait and basic backends.

## Acceptance Criteria

### Traits

- [ ] `SecretReader` trait with `get_secret(&self, key: &str) -> Result<Option<String>, SecretError>`
- [ ] `SecretWriter` trait with `set_secret()`, `delete_secret()`
- [ ] `SecretManager` trait combining both
- [ ] `SecretError` enum: NotFound, AccessDenied, NetworkError, ParseError

### Implementations

- [ ] `EnvSecretManager` — reads from `std::env::var()`, strips `os.environ/` prefix
- [ ] `VaultSecretManager` — HashiCorp Vault integration
- [ ] `AwsSecretManager` — AWS Secrets Manager integration
- [ ] `OidcSecretManager` — OIDC token resolution (google, azure, env)

### Caching

- [ ] `CachedSecretManager<T: SecretReader>` wrapper
- [ ] Stoolap cache with configurable TTL
- [ ] Cache format: `(key, value, expires_at)`

### Tests

- [ ] Env var lookup works
- [ ] os.environ/ prefix stripping works
- [ ] Cache hit returns cached value
- [ ] Cache miss fetches from underlying manager
- [ ] Cache TTL expiration works

## Key Files

- `crates/quota-router-core/src/secret_manager.rs` — new file (traits + implementations)
- `crates/quota-router-core/src/config.rs` — secret_manager config

## Notes

This is a new module. The traits should be in `secret_manager.rs`. Implementations can be in submodules. The cache should use stoolap for persistence.
