# RFC-0935: Secret Manager Integration

## Status: Accepted

## Summary

Integrate external secret managers (HashiCorp Vault, AWS Secrets Manager, OIDC) for API key resolution, matching LiteLLM's `get_secret()` behavior.

## Motivation

LiteLLM supports multiple secret backends via `get_secret()`:
- HashiCorp Vault
- AWS Secrets Manager
- OIDC providers (Google, Azure, env)
- Environment variables with `os.environ/` prefix

quota-router only reads from environment variables. This RFC adds secret manager support.

**Note:** CircleCI and GitHub OIDC providers are not yet implemented. Only Google, Azure, and env are specified.

## Specification

### 1. Secret Manager Trait

Split into read-only and read-write traits:
```rust
#[async_trait]
pub trait SecretReader: Send + Sync {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError>;
}

#[async_trait]
pub trait SecretWriter: Send + Sync {
    async fn set_secret(&self, key: &str, value: &str) -> Result<(), SecretError>;
    async fn delete_secret(&self, key: &str) -> Result<(), SecretError>;
}

// Combined trait for backends that support read+write
// NOTE: No backend in this RFC implements SecretManager (all are read-only).
// This trait is reserved for future use when write support is added.
#[async_trait]
pub trait SecretManager: SecretReader + SecretWriter {}

// Error type
pub enum SecretError {
    NotFound,
    AccessDenied,
    NetworkError(String),
    ParseError(String),
}
```

**Note:** EnvSecretManager and OIDC only implement `SecretReader`. Vault and AWS also only implement `SecretReader` in this RFC (write operations are not specified). `SecretWriter` is defined for future use.

### 2. Implementations

#### Environment Variables (default, read-only)
```rust
pub struct EnvSecretManager;

#[async_trait]
impl SecretReader for EnvSecretManager {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        // Handle os.environ/ prefix (convention, not LiteLLM — LiteLLM uses os.environ["KEY"])
        let key = key.strip_prefix("os.environ/").unwrap_or(key);
        Ok(std::env::var(key).ok())
    }
}
// Note: EnvSecretManager only implements SecretReader (not SecretWriter)
// because environment variables cannot be set programmatically
```

#### HashiCorp Vault
```rust
pub struct VaultSecretManager {
    client: reqwest::Client,
    base_url: String,
    token: String,
    mount: String,  // KV v2 mount path (from config, e.g., "secret")
    // Note: This implementation targets Vault KV v2 only. KV v1 mounts are not supported.
}

#[derive(serde::Deserialize)]
struct VaultResponse {
    data: VaultResponseData,
}

#[derive(serde::Deserialize)]
struct VaultResponseData {
    data: std::collections::HashMap<String, String>,
}

#[async_trait]
impl SecretReader for VaultSecretManager {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        let url = format!("{}/v1/{}/data/{}", self.base_url, self.mount, urlencoding::encode(key));
        let resp = self.client.get(&url)
            .header("X-Vault-Token", &self.token)
            .send().await
            .map_err(|e| SecretError::NetworkError(e.to_string()))?;

        if resp.status() == 404 {
            return Ok(None);
        }

        let data: VaultResponse = resp.json().await
            .map_err(|e| SecretError::ParseError(e.to_string()))?;
        // KV v2 stores arbitrary key-value pairs — extract field name from key path
        // key format: "secret/path/field_name" where field_name is the Vault key
        // self.mount is used in the URL path (e.g., /v1/secret/data/{key})
        let field_name = key.rsplit('/').next().unwrap_or(key);
        Ok(data.data.data.get(field_name).cloned())
    }
}
```

#### AWS Secrets Manager
```rust
pub struct AwsSecretManager {
    client: aws_sdk_secretsmanager::Client,
}

#[async_trait]
impl SecretReader for AwsSecretManager {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        let resp = self.client.get_secret_value()
            .secret_id(key)
            .send().await
            .map_err(|e| SecretError::NetworkError(e.to_string()))?;

        Ok(resp.secret_string().map(String::from))
    }
}
```

### 3. OIDC Support

Implement `SecretReader` for OIDC tokens with `oidc/` prefix:
```rust
pub struct OidcSecretManager {
    client: reqwest::Client,  // Reuse client across calls
}

#[async_trait]
impl SecretReader for OidcSecretManager {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        // key format: "oidc/{provider}/{audience}"
        let parts: Vec<&str> = key.splitn(3, '/').collect();
        if parts.len() < 3 || parts[0] != "oidc" {
            return Err(SecretError::ParseError("Invalid OIDC key format".to_string()));
        }

        let provider = parts[1];
        let audience = parts[2];

        match provider {
            "google" => {
                let resp = self.client
                    .get("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity")
                    .query(&[("audience", audience)])
                    .header("Metadata-Flavor", "Google")
                    .send().await
                    .map_err(|e| SecretError::NetworkError(e.to_string()))?;

                if resp.status().is_success() {
                    Ok(Some(resp.text().await.map_err(|e| SecretError::NetworkError(e.to_string()))?))
                } else {
                    Err(SecretError::NetworkError(format!("HTTP {}", resp.status())))
                }
            }
            "azure" => {
                let token_file = std::env::var("AZURE_FEDERATED_TOKEN_FILE")
                    .map_err(|_| SecretError::NotFound)?;
                Ok(Some(std::fs::read_to_string(token_file)
                    .map_err(|e| SecretError::NetworkError(e.to_string()))?))
            }
            "env" => {
                Ok(std::env::var(audience).ok())
            }
            _ => Err(SecretError::ParseError(format!("Unsupported OIDC provider: {}", provider)))
        }
    }
}
```

### 4. Secret Caching

Cache secrets in stoolap to reduce external calls:
```rust
pub struct CachedSecretManager<T: SecretReader> {
    inner: T,
    cache: StoolapCache,
    ttl: Duration,
}

// StoolapCache interface — must be defined before implementation
// (RFC-0914 may provide this, or define inline)
pub struct CacheEntry {
    pub value: String,
    pub expires_at: i64,  // Unix timestamp
}

#[async_trait]
pub trait StoolapCache: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<CacheEntry>, SecretError>;
    async fn set(&self, key: &str, value: &str, expires_at: i64) -> Result<(), SecretError>;
}

#[async_trait]
impl<T: SecretReader + Send + Sync> SecretReader for CachedSecretManager<T> {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        // Check cache first
        if let Some(cached) = self.cache.get(key).await? {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            if cached.expires_at > now {
                return Ok(Some(cached.value));
            }
        }

        // Fetch from underlying manager
        let value = self.inner.get_secret(key).await?;

        // Cache if found
        if let Some(ref v) = value {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            self.cache.set(key, v, now + self.ttl.as_secs() as i64).await?;
        }

        Ok(value)
    }
}
```

Cache format: `(key TEXT PRIMARY KEY, value TEXT, expires_at INTEGER)` — uses wall-clock timestamp (i64 Unix seconds), NOT Instant.

### 5. Key Resolution Chain

This RFC provides the `SecretReader` backend. The actual `resolve_api_key()` function is defined in RFC-0938. This RFC does NOT define its own `resolve_api_key()`.

RFC-0938's `resolve_api_key()` uses `std::env::var()` directly as a fast path. When a `SecretReader` is configured, it is called as an additional fallback tier after `std::env::var()`. For `type: env`, this is equivalent to `std::env::var()` (with `os.environ/` prefix support). For `type: vault/aws/oidc`, it queries the external backend.

**Integration:** When `secret_manager.type` is configured, RFC-0938's `resolve_api_key()` should call `secret_reader.get_secret()` as tier 5 (after provider-specific env var). The `std::env::var()` calls in RFC-0938 serve as the fast path when no SecretReader is configured.

### 6. Configuration

```yaml
secret_manager:
  type: env  # env, vault, aws, oidc
  vault:
    url: https://vault.example.com
    # Token loaded from env var VAULT_TOKEN (not interpolated in YAML to avoid circular dependency)
    mount: secret
  aws:
    region: us-east-1
    # Uses default AWS credential chain (env vars, IAM role, etc.)
  cache:
    enabled: true
    ttl_seconds: 300  # 5 minutes
    max_entries: 1000
```

**Circular dependency note:** Vault token cannot use `${VAULT_TOKEN}` YAML interpolation because the secret manager provides the interpolation. Instead, vault token is loaded directly from `std::env::var("VAULT_TOKEN")` at runtime.

## Dependencies

- RFC-0932: gateway auth
- RFC-0914: stoolap-only persistence (for caching secrets)

## Test Plan

1. Env var lookup works
2. os.environ/ prefix stripping works
3. HashiCorp Vault integration works
4. AWS Secrets Manager integration works
5. OIDC token resolution works for supported providers
6. Secret caching in stoolap works
7. Fallback chain: config → ANY_LLM_KEY → provider env var → secret manager (per RFC-0938 precedence)
8. Configuration validation works

## Version History

| Version | Date       | Change                                                                                |
|---------|------------|---------------------------------------------------------------------------------------|
| 1.0     | 2026-08-22 | Retroactive VH table addition (per long-horizon plan v1.3 Phase 1 + Option C per M37). |
