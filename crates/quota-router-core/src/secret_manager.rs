// Secret Manager Module (RFC-0935)
//
// Provides SecretReader/SecretWriter/SecretManager traits for integrating
// external secret managers (HashiCorp Vault, AWS Secrets Manager, OIDC)
// for API key resolution.
//
// Integration: RFC-0938's resolve_api_key() calls secret_reader.get_secret()
// as the lowest-priority tier (after env vars).

use crate::cache::StoolapCache;
use crate::config::SecretManagerConfig;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// Convert days since Unix epoch to (year, month, day) UTC.
/// Used for AWS SigV4 date formatting without chrono dependency.
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Simple algorithm: iterate years and months
    let mut year = 1970;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap_year(year);
    let days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1;
    for &dim in &days_in_month {
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    (year, month, days + 1)
}

#[allow(clippy::manual_is_multiple_of)]
fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ============================================================================
// Error Type
// ============================================================================

/// Error type for secret manager operations (RFC-0935)
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("Secret not found: {0}")]
    NotFound(String),
    #[error("Access denied: {0}")]
    AccessDenied(String),
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

// ============================================================================
// Traits (RFC-0935 Section 1)
// ============================================================================

/// Read-only secret access trait (RFC-0935)
#[async_trait]
pub trait SecretReader: Send + Sync + std::fmt::Debug {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError>;
}

/// Write-only secret mutation trait (RFC-0935)
/// Reserved for future use when write support is added to backends.
#[async_trait]
pub trait SecretWriter: Send + Sync {
    async fn set_secret(&self, key: &str, value: &str) -> Result<(), SecretError>;
    async fn delete_secret(&self, key: &str) -> Result<(), SecretError>;
}

/// Combined read-write trait (RFC-0935)
/// No backend in this RFC implements SecretManager (all are read-only).
/// This trait is reserved for future use.
#[async_trait]
pub trait SecretManager: SecretReader + SecretWriter {}

// ============================================================================
// EnvSecretManager (RFC-0935 Section 2.1)
// ============================================================================

/// Environment variable secret backend (read-only).
///
/// Reads secrets from `std::env::var()`. Strips `os.environ/` prefix
/// if present (convention, not LiteLLM — LiteLLM uses `os.environ["KEY"]`).
#[derive(Debug)]
pub struct EnvSecretManager;

impl EnvSecretManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnvSecretManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretReader for EnvSecretManager {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        let resolved_key = key.strip_prefix("os.environ/").unwrap_or(key);
        Ok(std::env::var(resolved_key).ok())
    }
}

// ============================================================================
// VaultSecretManager (RFC-0935 Section 2.2)
// ============================================================================

/// HashiCorp Vault KV v2 secret backend (read-only).
///
/// Targets Vault KV v2 only. KV v1 mounts are not supported.
pub struct VaultSecretManager {
    client: reqwest::Client,
    base_url: String,
    token: String,
    mount: String,
}

impl std::fmt::Debug for VaultSecretManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultSecretManager")
            .field("base_url", &self.base_url)
            .field("mount", &self.mount)
            .finish()
    }
}

#[derive(serde::Deserialize)]
struct VaultResponse {
    data: VaultResponseData,
}

#[derive(serde::Deserialize)]
struct VaultResponseData {
    data: std::collections::HashMap<String, String>,
}

impl VaultSecretManager {
    /// Create a new Vault secret manager.
    ///
    /// `base_url`: Vault server URL (e.g., "https://vault.example.com")
    /// `token`: Vault authentication token (loaded from VAULT_TOKEN env var)
    /// `mount`: KV v2 mount path (e.g., "secret")
    pub fn new(base_url: &str, token: &str, mount: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            mount: mount.to_string(),
        }
    }
}

#[async_trait]
impl SecretReader for VaultSecretManager {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        let url = format!(
            "{}/v1/{}/data/{}",
            self.base_url,
            self.mount,
            urlencoding::encode(key)
        );

        let resp = self
            .client
            .get(&url)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .map_err(|e| SecretError::NetworkError(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if resp.status() == reqwest::StatusCode::FORBIDDEN
            || resp.status() == reqwest::StatusCode::UNAUTHORIZED
        {
            return Err(SecretError::AccessDenied(format!(
                "Vault returned {} for key '{}'",
                resp.status(),
                key
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| SecretError::NetworkError(e.to_string()))?;

        let data: VaultResponse = serde_json::from_str(&body).map_err(|e| {
            SecretError::ParseError(format!(
                "Vault JSON deserialization failed at key '{}': {}",
                key, e
            ))
        })?;

        // KV v2 stores arbitrary key-value pairs — extract field name from key path
        // key format: "secret/path/field_name" where field_name is the Vault key
        let field_name = key.rsplit('/').next().unwrap_or(key);
        Ok(data.data.data.get(field_name).cloned())
    }
}

// ============================================================================
// AwsSecretManager (RFC-0935 Section 2.3)
// ============================================================================

/// AWS Secrets Manager secret backend (read-only).
///
/// Uses AWS credentials from the default credential chain:
/// 1. Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_SESSION_TOKEN)
/// 2. IAM role (instance profile, ECS task role)
///
/// Implements basic AWS Signature Version 4 signing for the GetSecretValue API.
pub struct AwsSecretManager {
    client: reqwest::Client,
    region: String,
}

impl std::fmt::Debug for AwsSecretManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsSecretManager")
            .field("region", &self.region)
            .finish()
    }
}

impl AwsSecretManager {
    /// Create a new AWS Secrets Manager secret reader.
    ///
    /// `region`: AWS region (e.g., "us-east-1")
    pub fn new(region: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            region: region.to_string(),
        }
    }

    fn endpoint(&self) -> String {
        format!("https://secretsmanager.{}.amazonaws.com", self.region)
    }
}

#[async_trait]
impl SecretReader for AwsSecretManager {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        let access_key = std::env::var("AWS_ACCESS_KEY_ID")
            .map_err(|_| SecretError::AccessDenied("AWS_ACCESS_KEY_ID not set".to_string()))?;
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY")
            .map_err(|_| SecretError::AccessDenied("AWS_SECRET_ACCESS_KEY not set".to_string()))?;
        let session_token = std::env::var("AWS_SESSION_TOKEN").ok();

        let body = serde_json::json!({
            "SecretId": key,
        });
        let body_str = body.to_string();
        let body_bytes = body_str.as_bytes();

        // AWS Signature Version 4 signing
        // Use std::time for UTC date formatting (avoid chrono dependency)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        // Days since epoch for rough UTC date (good enough for SigV4)
        let days = secs / 86400;
        let (year, month, day) = days_to_ymd(days);
        let hour = (secs % 86400) / 3600;
        let min = (secs % 3600) / 60;
        let sec = secs % 60;
        let datestamp = format!("{:04}{:02}{:02}", year, month, day);
        let amz_date = format!(
            "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
            year, month, day, hour, min, sec
        );
        let host = format!("secretsmanager.{}.amazonaws.com", self.region);

        // Create canonical request
        let content_type = "application/x-amz-json-1.1";
        let amz_target = "secretsmanager.GetSecretValue";

        let body_hash = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(body_bytes);
            hex::encode(hasher.finalize())
        };

        let canonical_headers = format!(
            "content-type:{}\nhost:{}\nx-amz-date:{}\nx-amz-target:{}\n",
            content_type, host, amz_date, amz_target
        );
        let signed_headers = "content-type;host;x-amz-date;x-amz-target";

        let canonical_uri = "/";
        let canonical_querystring = "";

        let canonical_request = format!(
            "POST\n{}\n{}\n{}\n{}\n{}",
            canonical_uri, canonical_querystring, canonical_headers, signed_headers, body_hash
        );

        // Create string to sign
        let algorithm = "AWS4-HMAC-SHA256";
        let credential_scope = format!("{}/{}/aws4_request", datestamp, self.region);

        let canonical_request_hash = {
            use sha2::Digest;
            let mut hasher = sha2::Sha256::new();
            hasher.update(canonical_request.as_bytes());
            hex::encode(hasher.finalize())
        };

        let string_to_sign = format!(
            "{}\n{}\n{}\n{}",
            algorithm, amz_date, credential_scope, canonical_request_hash
        );

        // Calculate signing key
        fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;
            let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
            mac.update(data);
            mac.finalize().into_bytes().to_vec()
        }

        let k_date = hmac_sha256(
            format!("AWS4{}", secret_key).as_bytes(),
            datestamp.as_bytes(),
        );
        let k_region = hmac_sha256(&k_date, self.region.as_bytes());
        let k_service = hmac_sha256(&k_region, b"secretsmanager");
        let k_signing = hmac_sha256(&k_service, b"aws4_request");

        let signature = {
            let hmac_result = hmac_sha256(&k_signing, string_to_sign.as_bytes());
            hex::encode(hmac_result)
        };

        let authorization = format!(
            "{} Credential={}/{}, SignedHeaders={}, Signature={}",
            algorithm, access_key, credential_scope, signed_headers, signature
        );

        let mut request_builder = self
            .client
            .post(self.endpoint())
            .header("Content-Type", content_type)
            .header("Host", &host)
            .header("X-Amz-Date", &amz_date)
            .header("X-Amz-Target", amz_target)
            .header("Authorization", &authorization)
            .body(body_bytes.to_vec());

        if let Some(ref token) = session_token {
            request_builder = request_builder.header("X-Amz-Security-Token", token);
        }

        let resp = request_builder
            .send()
            .await
            .map_err(|e| SecretError::NetworkError(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if resp.status().is_client_error() {
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(SecretError::AccessDenied(format!(
                "AWS returned error: {}",
                error_body
            )));
        }

        let resp_body = resp
            .text()
            .await
            .map_err(|e| SecretError::NetworkError(e.to_string()))?;

        let resp_json: serde_json::Value = serde_json::from_str(&resp_body)
            .map_err(|e| SecretError::ParseError(format!("AWS response parse error: {}", e)))?;

        Ok(resp_json
            .get("SecretString")
            .and_then(|v| v.as_str())
            .map(String::from))
    }
}

// ============================================================================
// OidcSecretManager (RFC-0935 Section 3)
// ============================================================================

/// OIDC token resolution secret backend (read-only).
///
/// Supports `oidc/{provider}/{audience}` key format.
/// Providers: google, azure, env
pub struct OidcSecretManager {
    client: reqwest::Client,
}

impl std::fmt::Debug for OidcSecretManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OidcSecretManager").finish()
    }
}

impl OidcSecretManager {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl Default for OidcSecretManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SecretReader for OidcSecretManager {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        // key format: "oidc/{provider}/{audience}"
        let parts: Vec<&str> = key.splitn(3, '/').collect();
        if parts.len() < 3 || parts[0] != "oidc" {
            return Err(SecretError::ParseError(format!(
                "Invalid OIDC key format: '{}'. Expected 'oidc/{{provider}}/{{audience}}'",
                key
            )));
        }

        let provider = parts[1];
        let audience = parts[2];

        match provider {
            "google" => {
                let url = format!(
                    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity?audience={}",
                    audience
                );
                let resp = self
                    .client
                    .get(&url)
                    .header("Metadata-Flavor", "Google")
                    .send()
                    .await
                    .map_err(|e| SecretError::NetworkError(e.to_string()))?;

                if resp.status().is_success() {
                    Ok(Some(
                        resp.text()
                            .await
                            .map_err(|e| SecretError::NetworkError(e.to_string()))?,
                    ))
                } else {
                    Err(SecretError::NetworkError(format!(
                        "Google metadata HTTP {}",
                        resp.status()
                    )))
                }
            }
            "azure" => {
                let token_file = std::env::var("AZURE_FEDERATED_TOKEN_FILE").map_err(|_| {
                    SecretError::NotFound("AZURE_FEDERATED_TOKEN_FILE not set".to_string())
                })?;
                Ok(Some(
                    std::fs::read_to_string(token_file)
                        .map_err(|e| SecretError::NetworkError(e.to_string()))?,
                ))
            }
            "env" => Ok(std::env::var(audience).ok()),
            _ => Err(SecretError::ParseError(format!(
                "Unsupported OIDC provider: '{}'",
                provider
            ))),
        }
    }
}

// ============================================================================
// SecretReader for Arc<dyn SecretReader> (allows wrapping in CachedSecretManager)
// ============================================================================

#[async_trait]
impl SecretReader for Arc<dyn SecretReader> {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        self.as_ref().get_secret(key).await
    }
}

// ============================================================================
// CachedSecretManager (RFC-0935 Section 4)
// ============================================================================

/// Caching wrapper for any SecretReader implementation.
///
/// Uses the StoolapCache trait for secret caching with configurable TTL.
/// When Mission-0914-a completes, switch to stoolap-backed implementation.
/// Currently uses InMemoryCache (HashMap-based) as interim.
pub struct CachedSecretManager<T: SecretReader> {
    inner: T,
    cache: Arc<dyn StoolapCache>,
    ttl: Duration,
}

impl<T: SecretReader> std::fmt::Debug for CachedSecretManager<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedSecretManager")
            .field("inner", &self.inner)
            .field("ttl", &self.ttl)
            .finish()
    }
}

impl<T: SecretReader> CachedSecretManager<T> {
    /// Create a new cached secret manager.
    ///
    /// `inner`: The underlying secret reader to wrap
    /// `cache`: Cache implementation (InMemoryCache interim, stoolap-backed future)
    /// `ttl`: Cache TTL duration
    pub fn new(inner: T, cache: Arc<dyn StoolapCache>, ttl: Duration) -> Self {
        Self { inner, cache, ttl }
    }
}

#[async_trait]
impl<T: SecretReader + Send + Sync> SecretReader for CachedSecretManager<T> {
    async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
        // Check cache first
        if let Some(cached) = self.cache.get(key).await {
            return Ok(Some(cached));
        }

        // Fetch from underlying manager
        let value = self.inner.get_secret(key).await?;

        // Cache if found
        if let Some(ref v) = value {
            let _ = self
                .cache
                .set(key, v, self.ttl.as_secs())
                .await
                .map_err(|e| SecretError::NetworkError(format!("Cache set error: {}", e)));
        }

        Ok(value)
    }
}

// ============================================================================
// Factory Function
// ============================================================================

/// Create a SecretReader from configuration (RFC-0935 Section 6).
///
/// Returns a boxed SecretReader based on the configured type:
/// - "env": EnvSecretManager (reads from std::env::var)
/// - "vault": VaultSecretManager (HashiCorp Vault KV v2)
/// - "aws": AwsSecretManager (AWS Secrets Manager)
/// - "oidc": OidcSecretManager (OIDC token resolution)
///
/// When cache is enabled, wraps the reader in CachedSecretManager.
pub fn create_secret_reader(
    config: &SecretManagerConfig,
) -> Result<Arc<dyn SecretReader>, SecretError> {
    let reader: Arc<dyn SecretReader> = match config.r#type.as_str() {
        "env" => Arc::new(EnvSecretManager::new()),
        "vault" => {
            let vault_config = config.vault.as_ref().ok_or_else(|| {
                SecretError::ParseError("Vault configuration required for type 'vault'".to_string())
            })?;
            let token = std::env::var("VAULT_TOKEN").map_err(|_| {
                SecretError::ParseError(
                    "VAULT_TOKEN environment variable not set. \
                     Vault token cannot use YAML interpolation to avoid circular dependency."
                        .to_string(),
                )
            })?;
            Arc::new(VaultSecretManager::new(
                &vault_config.url,
                &token,
                &vault_config.mount,
            ))
        }
        "aws" => {
            let aws_config = config.aws.as_ref().ok_or_else(|| {
                SecretError::ParseError("AWS configuration required for type 'aws'".to_string())
            })?;
            Arc::new(AwsSecretManager::new(&aws_config.region))
        }
        "oidc" => Arc::new(OidcSecretManager::new()),
        other => {
            return Err(SecretError::ParseError(format!(
                "Unknown secret manager type: '{}'. Expected: env, vault, aws, oidc",
                other
            )));
        }
    };

    // Wrap with cache if configured
    if let Some(ref cache_config) = config.cache {
        if cache_config.enabled {
            let cache: Arc<dyn StoolapCache> = Arc::new(crate::cache::InMemoryCache::new());
            let ttl = Duration::from_secs(cache_config.ttl_seconds);
            return Ok(Arc::new(CachedSecretManager::new(reader, cache, ttl)));
        }
    }

    Ok(reader)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;

    // -----------------------------------------------------------------------
    // Mock SecretReader for testing
    // -----------------------------------------------------------------------

    /// In-memory mock secret reader for testing
    #[derive(Debug)]
    struct MockSecretReader {
        secrets: HashMap<String, String>,
    }

    impl MockSecretReader {
        fn new(secrets: HashMap<String, String>) -> Self {
            Self { secrets }
        }
    }

    #[async_trait]
    impl SecretReader for MockSecretReader {
        async fn get_secret(&self, key: &str) -> Result<Option<String>, SecretError> {
            Ok(self.secrets.get(key).cloned())
        }
    }

    /// Mock secret reader that always returns an error
    #[derive(Debug)]
    #[allow(dead_code)]
    struct FailingSecretReader;

    #[async_trait]
    impl SecretReader for FailingSecretReader {
        async fn get_secret(&self, _key: &str) -> Result<Option<String>, SecretError> {
            Err(SecretError::NetworkError("mock failure".to_string()))
        }
    }

    // -----------------------------------------------------------------------
    // Mock Cache for testing TTL behavior
    // -----------------------------------------------------------------------

    /// Test cache that supports TTL-based expiration
    struct TestCache {
        entries: RwLock<HashMap<String, (String, std::time::Instant, u64)>>,
    }

    impl TestCache {
        fn new() -> Self {
            Self {
                entries: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl StoolapCache for TestCache {
        async fn get(&self, key: &str) -> Option<String> {
            let entries = self.entries.read().unwrap();
            if let Some((value, cached_at, ttl_secs)) = entries.get(key) {
                if cached_at.elapsed() < Duration::from_secs(*ttl_secs) {
                    return Some(value.clone());
                }
            }
            None
        }

        async fn set(&self, key: &str, value: &str, ttl_secs: u64) -> Result<(), String> {
            let mut entries = self.entries.write().unwrap();
            entries.insert(
                key.to_string(),
                (value.to_string(), std::time::Instant::now(), ttl_secs),
            );
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<(), String> {
            let mut entries = self.entries.write().unwrap();
            entries.remove(key);
            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // EnvSecretManager Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_env_secret_manager_lookup() {
        std::env::set_var("TEST_SECRET_0935", "my-secret-value");
        let reader = EnvSecretManager::new();
        let result = reader.get_secret("TEST_SECRET_0935").await.unwrap();
        assert_eq!(result, Some("my-secret-value".to_string()));
        std::env::remove_var("TEST_SECRET_0935");
    }

    #[tokio::test]
    async fn test_env_secret_manager_not_found() {
        std::env::remove_var("NONEXISTENT_SECRET_0935");
        let reader = EnvSecretManager::new();
        let result = reader.get_secret("NONEXISTENT_SECRET_0935").await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_env_secret_manager_os_environ_prefix_stripping() {
        std::env::set_var("MY_ENV_KEY_0935", "env-value");
        let reader = EnvSecretManager::new();
        let result = reader
            .get_secret("os.environ/MY_ENV_KEY_0935")
            .await
            .unwrap();
        assert_eq!(result, Some("env-value".to_string()));
        std::env::remove_var("MY_ENV_KEY_0935");
    }

    #[tokio::test]
    async fn test_env_secret_manager_no_prefix() {
        std::env::set_var("PLAIN_KEY_0935", "plain-value");
        let reader = EnvSecretManager::new();
        let result = reader.get_secret("PLAIN_KEY_0935").await.unwrap();
        assert_eq!(result, Some("plain-value".to_string()));
        std::env::remove_var("PLAIN_KEY_0935");
    }

    // -----------------------------------------------------------------------
    // CachedSecretManager Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_cache_miss_fetches_from_inner() {
        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "sk-test-123".to_string());
        let inner = MockSecretReader::new(secrets);
        let cache: Arc<dyn StoolapCache> = Arc::new(crate::cache::InMemoryCache::new());
        let cached = CachedSecretManager::new(inner, cache.clone(), Duration::from_secs(300));

        let result = cached.get_secret("api_key").await.unwrap();
        assert_eq!(result, Some("sk-test-123".to_string()));

        // Verify it was cached
        let cached_value = cache.get("api_key").await;
        assert_eq!(cached_value, Some("sk-test-123".to_string()));
    }

    #[tokio::test]
    async fn test_cache_hit_returns_cached_value() {
        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "sk-original".to_string());
        let inner = MockSecretReader::new(secrets);
        let cache: Arc<dyn StoolapCache> = Arc::new(crate::cache::InMemoryCache::new());

        // Pre-populate cache with different value
        cache.set("api_key", "sk-cached", 300).await.unwrap();

        let cached = CachedSecretManager::new(inner, cache, Duration::from_secs(300));
        let result = cached.get_secret("api_key").await.unwrap();
        // Should return cached value, not inner
        assert_eq!(result, Some("sk-cached".to_string()));
    }

    #[tokio::test]
    async fn test_cache_ttl_expiration() {
        let mut secrets = HashMap::new();
        secrets.insert("api_key".to_string(), "sk-fresh".to_string());
        let inner = MockSecretReader::new(secrets);
        let cache: Arc<TestCache> = Arc::new(TestCache::new());

        // Pre-populate cache with expired entry (0 second TTL)
        cache.set("api_key", "sk-expired", 0).await.unwrap();

        let cached = CachedSecretManager::new(
            inner,
            cache as Arc<dyn StoolapCache>,
            Duration::from_secs(0),
        );

        // Small delay to ensure TTL expires
        tokio::time::sleep(Duration::from_millis(10)).await;

        let result = cached.get_secret("api_key").await.unwrap();
        // Should fetch from inner since cache expired
        assert_eq!(result, Some("sk-fresh".to_string()));
    }

    #[tokio::test]
    async fn test_cache_not_found_not_cached() {
        let inner = MockSecretReader::new(HashMap::new());
        let cache: Arc<dyn StoolapCache> = Arc::new(crate::cache::InMemoryCache::new());
        let cached = CachedSecretManager::new(inner, cache.clone(), Duration::from_secs(300));

        let result = cached.get_secret("missing_key").await.unwrap();
        assert_eq!(result, None);

        // Should not cache misses
        let cached_value = cache.get("missing_key").await;
        assert_eq!(cached_value, None);
    }

    // -----------------------------------------------------------------------
    // VaultSecretManager Tests (mocked HTTP)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_vault_secret_manager_parses_key_field() {
        // Test that field_name extraction works correctly
        // key = "secret/path/api_key" -> field_name = "api_key"
        let key = "secret/path/api_key";
        let field_name = key.rsplit('/').next().unwrap_or(key);
        assert_eq!(field_name, "api_key");
    }

    #[tokio::test]
    async fn test_vault_secret_manager_simple_key() {
        // When key has no slashes, field_name = key
        let key = "api_key";
        let field_name = key.rsplit('/').next().unwrap_or(key);
        assert_eq!(field_name, "api_key");
    }

    // -----------------------------------------------------------------------
    // AwsSecretManager Tests (mocked)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_aws_secret_manager_endpoint_format() {
        let manager = AwsSecretManager::new("us-east-1");
        assert_eq!(
            manager.endpoint(),
            "https://secretsmanager.us-east-1.amazonaws.com"
        );
    }

    #[tokio::test]
    async fn test_aws_secret_manager_endpoint_eu_west() {
        let manager = AwsSecretManager::new("eu-west-1");
        assert_eq!(
            manager.endpoint(),
            "https://secretsmanager.eu-west-1.amazonaws.com"
        );
    }

    // -----------------------------------------------------------------------
    // OidcSecretManager Tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_oidc_secret_manager_invalid_key_format() {
        let reader = OidcSecretManager::new();
        let result = reader.get_secret("not-oidc/key").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SecretError::ParseError(msg) => {
                assert!(msg.contains("Invalid OIDC key format"));
            }
            other => panic!("Expected ParseError, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_oidc_secret_manager_unsupported_provider() {
        let reader = OidcSecretManager::new();
        let result = reader.get_secret("oidc/unsupported/audience").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SecretError::ParseError(msg) => {
                assert!(msg.contains("Unsupported OIDC provider: 'unsupported'"));
            }
            other => panic!("Expected ParseError, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_oidc_secret_manager_env_provider() {
        std::env::set_var("OIDC_TEST_TOKEN_0935", "env-token-value");
        let reader = OidcSecretManager::new();
        let result = reader
            .get_secret("oidc/env/OIDC_TEST_TOKEN_0935")
            .await
            .unwrap();
        assert_eq!(result, Some("env-token-value".to_string()));
        std::env::remove_var("OIDC_TEST_TOKEN_0935");
    }

    #[tokio::test]
    async fn test_oidc_secret_manager_env_provider_not_found() {
        std::env::remove_var("NONEXISTENT_OIDC_0935");
        let reader = OidcSecretManager::new();
        let result = reader
            .get_secret("oidc/env/NONEXISTENT_OIDC_0935")
            .await
            .unwrap();
        assert_eq!(result, None);
    }

    // -----------------------------------------------------------------------
    // SecretError Tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_secret_error_display() {
        assert_eq!(
            SecretError::NotFound("key1".to_string()).to_string(),
            "Secret not found: key1"
        );
        assert_eq!(
            SecretError::AccessDenied("denied".to_string()).to_string(),
            "Access denied: denied"
        );
        assert_eq!(
            SecretError::NetworkError("timeout".to_string()).to_string(),
            "Network error: timeout"
        );
        assert_eq!(
            SecretError::ParseError("bad json".to_string()).to_string(),
            "Parse error: bad json"
        );
    }

    // -----------------------------------------------------------------------
    // Factory Function Tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_secret_reader_env() {
        let config = SecretManagerConfig {
            r#type: "env".to_string(),
            vault: None,
            aws: None,
            cache: None,
        };
        let reader = create_secret_reader(&config);
        assert!(reader.is_ok());
    }

    #[test]
    fn test_create_secret_reader_unknown_type() {
        let config = SecretManagerConfig {
            r#type: "unknown".to_string(),
            vault: None,
            aws: None,
            cache: None,
        };
        let reader = create_secret_reader(&config);
        assert!(reader.is_err());
        match reader.unwrap_err() {
            SecretError::ParseError(msg) => {
                assert!(msg.contains("Unknown secret manager type: 'unknown'"));
            }
            other => panic!("Expected ParseError, got: {:?}", other),
        }
    }

    #[test]
    fn test_create_secret_reader_vault_missing_config() {
        let config = SecretManagerConfig {
            r#type: "vault".to_string(),
            vault: None,
            aws: None,
            cache: None,
        };
        let reader = create_secret_reader(&config);
        assert!(reader.is_err());
        match reader.unwrap_err() {
            SecretError::ParseError(msg) => {
                assert!(msg.contains("Vault configuration required"));
            }
            other => panic!("Expected ParseError, got: {:?}", other),
        }
    }

    #[test]
    fn test_create_secret_reader_aws_missing_config() {
        let config = SecretManagerConfig {
            r#type: "aws".to_string(),
            vault: None,
            aws: None,
            cache: None,
        };
        let reader = create_secret_reader(&config);
        assert!(reader.is_err());
        match reader.unwrap_err() {
            SecretError::ParseError(msg) => {
                assert!(msg.contains("AWS configuration required"));
            }
            other => panic!("Expected ParseError, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // YAML Config Parsing Tests (RFC-0935 Section 6)
    // -----------------------------------------------------------------------

    #[test]
    fn test_secret_manager_config_yaml_env() {
        let yaml = r#"
type: env
"#;
        let config: SecretManagerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.r#type, "env");
        assert!(config.vault.is_none());
        assert!(config.aws.is_none());
        assert!(config.cache.is_none());
    }

    #[test]
    fn test_secret_manager_config_yaml_vault() {
        let yaml = r#"
type: vault
vault:
  url: https://vault.example.com
  mount: secret
"#;
        let config: SecretManagerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.r#type, "vault");
        let vault = config.vault.unwrap();
        assert_eq!(vault.url, "https://vault.example.com");
        assert_eq!(vault.mount, "secret");
    }

    #[test]
    fn test_secret_manager_config_yaml_vault_default_mount() {
        let yaml = r#"
type: vault
vault:
  url: https://vault.example.com
"#;
        let config: SecretManagerConfig = serde_yaml::from_str(yaml).unwrap();
        let vault = config.vault.unwrap();
        assert_eq!(vault.mount, "secret");
    }

    #[test]
    fn test_secret_manager_config_yaml_aws() {
        let yaml = r#"
type: aws
aws:
  region: us-east-1
"#;
        let config: SecretManagerConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.r#type, "aws");
        let aws = config.aws.unwrap();
        assert_eq!(aws.region, "us-east-1");
    }

    #[test]
    fn test_secret_manager_config_yaml_with_cache() {
        let yaml = r#"
type: env
cache:
  enabled: true
  ttl_seconds: 300
  max_entries: 1000
"#;
        let config: SecretManagerConfig = serde_yaml::from_str(yaml).unwrap();
        let cache = config.cache.unwrap();
        assert!(cache.enabled);
        assert_eq!(cache.ttl_seconds, 300);
        assert_eq!(cache.max_entries, 1000);
    }

    #[test]
    fn test_secret_manager_config_yaml_cache_defaults() {
        let yaml = r#"
type: env
cache: {}
"#;
        let config: SecretManagerConfig = serde_yaml::from_str(yaml).unwrap();
        let cache = config.cache.unwrap();
        assert!(cache.enabled);
        assert_eq!(cache.ttl_seconds, 300);
        assert_eq!(cache.max_entries, 1000);
    }

    // -----------------------------------------------------------------------
    // GatewayConfig with secret_manager (integration)
    // -----------------------------------------------------------------------

    #[test]
    fn test_gateway_config_with_secret_manager() {
        let yaml = r#"
deployments:
  - model_name: gpt-4o
    litellm_params:
      provider: openai
      model: gpt-4o
secret_manager:
  type: env
  cache:
    enabled: true
    ttl_seconds: 60
"#;
        let config = crate::config::parse_config(yaml).unwrap();
        let sm = config.secret_manager.unwrap();
        assert_eq!(sm.r#type, "env");
        let cache = sm.cache.unwrap();
        assert!(cache.enabled);
        assert_eq!(cache.ttl_seconds, 60);
    }
}
