//! JWT Validation (RFC-0949)
//!
//! JWT validation with JWKS caching, clock skew tolerance, and audience/issuer validation.

use super::{JwtAlgorithm, JwtValidationConfig, SsoError, TokenClaims};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ============================================================================
// JWT Header
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtHeader {
    pub alg: String,
    pub typ: Option<String>,
    pub kid: Option<String>,
}

// ============================================================================
// JWKS Cache Entry
// ============================================================================

#[derive(Debug, Clone)]
struct JwksCacheEntry {
    keys: Vec<JwksKey>,
    fetched_at: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwksKey {
    pub kty: String,
    #[serde(rename = "use")]
    pub key_use: Option<String>,
    pub kid: Option<String>,
    pub alg: Option<String>,
    pub n: Option<String>,
    pub e: Option<String>,
    pub x: Option<String>,
    pub y: Option<String>,
    pub crv: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwksResponse {
    pub keys: Vec<JwksKey>,
}

// ============================================================================
// Token Validator
// ============================================================================

pub struct TokenValidator {
    config: JwtValidationConfig,
    jwks_cache: Arc<RwLock<HashMap<String, JwksCacheEntry>>>,
}

impl TokenValidator {
    pub fn new(config: JwtValidationConfig) -> Self {
        Self {
            config,
            jwks_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Validate a JWT token and return claims
    pub async fn validate(
        &self,
        token: &str,
        expected_audience: &str,
        expected_issuer: &str,
    ) -> Result<TokenClaims, SsoError> {
        // 1. Parse header to get algorithm and kid
        let header = self.parse_header(token)?;

        // 2. Reject alg=none
        if header.alg.to_uppercase() == "NONE" {
            return Err(SsoError::TokenAlgorithmNone);
        }

        // 3. Check algorithm is supported
        let alg = parse_algorithm(&header.alg)?;
        if !self.config.supported_algorithms.contains(&alg) {
            return Err(SsoError::TokenAlgorithmUnsupported(header.alg));
        }

        // 4. Decode payload
        // TODO(RFC-0949): Signature verification requires JWKS key matching by kid,
        // then RSA/EC signature verification. This is a Phase 2 feature.
        // Current implementation validates: algorithm, audience, issuer, expiry, not-before.
        // Tokens with forged signatures will pass until signature verification is implemented.
        let claims = self.decode_payload(token)?;

        // 5. Validate audience
        if claims.aud != expected_audience {
            return Err(SsoError::AudienceMismatch {
                expected: expected_audience.to_string(),
                actual: claims.aud,
            });
        }

        // 6. Validate issuer
        if claims.iss != expected_issuer {
            return Err(SsoError::IssuerMismatch {
                expected: expected_issuer.to_string(),
                actual: claims.iss,
            });
        }

        // 7. Validate expiration with clock skew
        let now = chrono::Utc::now().timestamp();
        if claims.exp + (self.config.clock_skew as i64) < now {
            return Err(SsoError::TokenExpired);
        }

        // 8. Validate not-before (iat) with clock skew
        if claims.iat - (self.config.clock_skew as i64) > now {
            return Err(SsoError::TokenInvalid("token used before issued".into()));
        }

        Ok(claims)
    }

    /// Parse JWT header without verification
    fn parse_header(&self, token: &str) -> Result<JwtHeader, SsoError> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(SsoError::TokenInvalid("invalid JWT format".into()));
        }

        let header_bytes = base64_url_decode(parts[0])
            .map_err(|_| SsoError::TokenInvalid("invalid header encoding".into()))?;
        serde_json::from_slice(&header_bytes)
            .map_err(|_| SsoError::TokenInvalid("invalid header JSON".into()))
    }

    /// Decode JWT payload without signature verification
    fn decode_payload(&self, token: &str) -> Result<TokenClaims, SsoError> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(SsoError::TokenInvalid("invalid JWT format".into()));
        }

        let payload_bytes = base64_url_decode(parts[1])
            .map_err(|_| SsoError::TokenInvalid("invalid payload encoding".into()))?;
        serde_json::from_slice(&payload_bytes)
            .map_err(|_| SsoError::TokenInvalid("invalid payload JSON".into()))
    }

    /// Fetch JWKS from URL with caching
    pub async fn fetch_jwks(&self, jwks_url: &str) -> Result<Vec<JwksKey>, SsoError> {
        // Check cache
        {
            let cache = self.jwks_cache.read().await;
            if let Some(entry) = cache.get(jwks_url) {
                if entry.fetched_at.elapsed() < Duration::from_secs(self.config.jwks_cache_ttl) {
                    return Ok(entry.keys.clone());
                }
            }
        }

        // Fetch from URL
        let client = reqwest::Client::new();
        let response = client
            .get(jwks_url)
            .send()
            .await
            .map_err(|e| SsoError::ProviderError(format!("JWKS fetch failed: {}", e)))?;

        let jwks: JwksResponse = response
            .json()
            .await
            .map_err(|e| SsoError::ProviderError(format!("JWKS parse failed: {}", e)))?;

        // Update cache
        {
            let mut cache = self.jwks_cache.write().await;
            cache.insert(
                jwks_url.to_string(),
                JwksCacheEntry {
                    keys: jwks.keys.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }

        Ok(jwks.keys)
    }

    /// Clear JWKS cache
    pub async fn clear_cache(&self) {
        let mut cache = self.jwks_cache.write().await;
        cache.clear();
    }
}

// ============================================================================
// Algorithm Parsing
// ============================================================================

fn parse_algorithm(alg: &str) -> Result<JwtAlgorithm, SsoError> {
    match alg.to_uppercase().as_str() {
        "RS256" => Ok(JwtAlgorithm::RS256),
        "RS384" => Ok(JwtAlgorithm::RS384),
        "RS512" => Ok(JwtAlgorithm::RS512),
        "ES256" => Ok(JwtAlgorithm::ES256),
        "ES384" => Ok(JwtAlgorithm::ES384),
        "PS256" => Ok(JwtAlgorithm::PS256),
        _ => Err(SsoError::TokenAlgorithmUnsupported(alg.to_string())),
    }
}

// ============================================================================
// Base64 URL Decoding
// ============================================================================

fn base64_url_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|_| "invalid base64url")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_algorithm() {
        assert!(matches!(parse_algorithm("RS256"), Ok(JwtAlgorithm::RS256)));
        assert!(matches!(parse_algorithm("ES256"), Ok(JwtAlgorithm::ES256)));
        assert!(matches!(parse_algorithm("PS256"), Ok(JwtAlgorithm::PS256)));
        assert!(parse_algorithm("none").is_err());
        assert!(parse_algorithm("HS256").is_err());
    }

    #[test]
    fn test_reject_algorithm_none() {
        let config = JwtValidationConfig::default();
        let validator = TokenValidator::new(config);

        // Create a token with alg=none header
        let header = base64_url_encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload = base64_url_encode(
            r#"{"sub":"user","iss":"issuer","aud":"audience","exp":9999999999,"iat":1000000000}"#,
        );
        let token = format!("{}.{}.signature", header, payload);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(validator.validate(&token, "audience", "issuer"));
        assert!(matches!(result, Err(SsoError::TokenAlgorithmNone)));
    }

    #[test]
    fn test_validate_audience_mismatch() {
        let config = JwtValidationConfig::default();
        let validator = TokenValidator::new(config);

        let header = base64_url_encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = base64_url_encode(
            r#"{"sub":"user","iss":"issuer","aud":"wrong","exp":9999999999,"iat":1000000000}"#,
        );
        let token = format!("{}.{}.sig", header, payload);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(validator.validate(&token, "expected", "issuer"));
        assert!(matches!(result, Err(SsoError::AudienceMismatch { .. })));
    }

    #[test]
    fn test_validate_issuer_mismatch() {
        let config = JwtValidationConfig::default();
        let validator = TokenValidator::new(config);

        let header = base64_url_encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = base64_url_encode(
            r#"{"sub":"user","iss":"wrong","aud":"audience","exp":9999999999,"iat":1000000000}"#,
        );
        let token = format!("{}.{}.sig", header, payload);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(validator.validate(&token, "audience", "expected"));
        assert!(matches!(result, Err(SsoError::IssuerMismatch { .. })));
    }

    #[test]
    fn test_validate_expired_token() {
        let config = JwtValidationConfig::default();
        let validator = TokenValidator::new(config);

        let header = base64_url_encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = base64_url_encode(
            r#"{"sub":"user","iss":"issuer","aud":"audience","exp":1000000000,"iat":999999000}"#,
        );
        let token = format!("{}.{}.sig", header, payload);

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(validator.validate(&token, "audience", "issuer"));
        assert!(matches!(result, Err(SsoError::TokenExpired)));
    }

    #[test]
    fn test_jwks_cache_config() {
        let config = JwtValidationConfig {
            jwks_cache_ttl: 3600,
            clock_skew: 30,
            supported_algorithms: vec![JwtAlgorithm::RS256],
        };
        assert_eq!(config.jwks_cache_ttl, 3600);
        assert_eq!(config.clock_skew, 30);
    }

    fn base64_url_encode(input: &str) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(input.as_bytes())
    }
}
