# RFC-0935: Secret Manager Integration

## Status: Draft

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
        let url = format!("{}/v1/secret/data/{}", self.base_url, urlencoding::encode(key));
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
pub struct OidcSecretManager;

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
                let client = reqwest::Client::new();
                let resp = client
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
    cache: StoolapCache,  // Defined in RFC-0914 (stoolap-only persistence)
    ttl: Duration,
}

// StoolapCache interface (from RFC-0914):
// async fn get(&self, key: &str) -> Result<Option<CacheEntry>>
// async fn set(&self, key: &str, value: &str, expires_at: i64) -> Result<()>
// CacheEntry { value: String, expires_at: i64 }

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

RFC-0938's `resolve_api_key()` uses `std::env::var()` directly for env var fallback. The `SecretReader` trait is used for external secret managers (Vault, AWS, OIDC) only, not for basic env var lookup.

**Integration:** When a `SecretReader` is configured (Vault/AWS/OIDC), RFC-0938's `resolve_api_key()` should call `secret_reader.get_secret()` as an additional fallback tier after `std::env::var()`.

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
7. Fallback chain: config → secret manager → env var
8. Configuration validation works
