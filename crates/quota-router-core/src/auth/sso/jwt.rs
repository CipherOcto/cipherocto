//! JWT Validation (RFC-0949)
//!
//! JWT validation with JWKS caching, clock skew tolerance, and audience/issuer validation.
//! Implements cryptographic signature verification using JWKS keys.

use super::{JwtAlgorithm, JwtValidationConfig, SsoError, TokenClaims};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
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

    /// Validate a JWT token with cryptographic signature verification
    ///
    /// Performs full validation:
    /// 1. Parse header to get algorithm and kid
    /// 2. Reject alg=none
    /// 3. Check algorithm is supported
    /// 4. Fetch JWKS and find matching key by kid
    /// 5. Verify cryptographic signature using JWKS key
    /// 6. Validate audience, issuer, expiry, not-before
    pub async fn validate(
        &self,
        token: &str,
        expected_audience: &str,
        expected_issuer: &str,
        jwks_url: &str,
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

        // 4. Fetch JWKS and find matching key
        let jwks_keys = self.fetch_jwks(jwks_url).await?;
        let kid = header.kid.as_deref().unwrap_or("");
        let jwks_key = jwks_keys
            .iter()
            .find(|k| k.kid.as_deref() == Some(kid))
            .ok_or_else(|| SsoError::TokenInvalid(format!("no JWKS key found for kid: {}", kid)))?;

        // 5. Verify cryptographic signature using JWKS key
        let claims = self.verify_signature(token, jwks_key, &alg)?;

        // 6. Validate claims
        self.validate_claims(&claims, expected_audience, expected_issuer)
    }

    /// Validate token claims (audience, issuer, expiry, not-before)
    ///
    /// This method validates the claims without signature verification.
    /// Used internally by validate() and for testing.
    pub fn validate_claims(
        &self,
        claims: &TokenClaims,
        expected_audience: &str,
        expected_issuer: &str,
    ) -> Result<TokenClaims, SsoError> {
        // Validate audience
        if claims.aud != expected_audience {
            return Err(SsoError::AudienceMismatch {
                expected: expected_audience.to_string(),
                actual: claims.aud.clone(),
            });
        }

        // Validate issuer
        if claims.iss != expected_issuer {
            return Err(SsoError::IssuerMismatch {
                expected: expected_issuer.to_string(),
                actual: claims.iss.clone(),
            });
        }

        // Validate expiration with clock skew
        let now = chrono::Utc::now().timestamp();
        if claims.exp + (self.config.clock_skew as i64) < now {
            return Err(SsoError::TokenExpired);
        }

        // Validate not-before (iat) with clock skew
        if claims.iat - (self.config.clock_skew as i64) > now {
            return Err(SsoError::TokenInvalid("token used before issued".into()));
        }

        Ok(claims.clone())
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

    /// Verify JWT signature using JWKS key
    fn verify_signature(
        &self,
        token: &str,
        jwks_key: &JwksKey,
        alg: &JwtAlgorithm,
    ) -> Result<TokenClaims, SsoError> {
        // Build decoding key from JWKS key components
        let decoding_key = build_decoding_key(jwks_key, alg)?;

        // Map our algorithm to jsonwebtoken algorithm
        let json_alg = match alg {
            JwtAlgorithm::RS256 => Algorithm::RS256,
            JwtAlgorithm::RS384 => Algorithm::RS384,
            JwtAlgorithm::RS512 => Algorithm::RS512,
            JwtAlgorithm::ES256 => Algorithm::ES256,
            JwtAlgorithm::ES384 => Algorithm::ES384,
            JwtAlgorithm::PS256 => Algorithm::PS256,
        };

        // Set up validation (we validate aud/iss/exp/iat ourselves)
        let mut validation = Validation::new(json_alg);
        validation.validate_aud = false;
        validation.validate_exp = false;
        validation.validate_nbf = false;
        validation.set_audience::<&str>(&[]); // Disable audience check in jsonwebtoken
        validation.set_issuer::<&str>(&[]); // Disable issuer check in jsonwebtoken

        // Decode and verify signature
        let token_data =
            decode::<TokenClaims>(token, &decoding_key, &validation).map_err(|e| {
                SsoError::TokenInvalid(format!("JWT signature verification failed: {}", e))
            })?;

        Ok(token_data.claims)
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
// JWKS Key to DecodingKey Conversion
// ============================================================================

/// Build a DecodingKey from JWKS key components
fn build_decoding_key(
    key: &JwksKey,
    alg: &JwtAlgorithm,
) -> Result<DecodingKey, SsoError> {
    match alg {
        JwtAlgorithm::RS256 | JwtAlgorithm::RS384 | JwtAlgorithm::RS512 | JwtAlgorithm::PS256 => {
            // RSA key: needs n (modulus) and e (exponent)
            let n = key
                .n
                .as_ref()
                .ok_or_else(|| SsoError::TokenInvalid("RSA key missing modulus (n)".into()))?;
            let e = key
                .e
                .as_ref()
                .ok_or_else(|| SsoError::TokenInvalid("RSA key missing exponent (e)".into()))?;
            DecodingKey::from_rsa_components(n, e)
                .map_err(|e| SsoError::TokenInvalid(format!("invalid RSA key: {}", e)))
        }
        JwtAlgorithm::ES256 | JwtAlgorithm::ES384 => {
            // EC key: needs x and y coordinates
            let x = key
                .x
                .as_ref()
                .ok_or_else(|| SsoError::TokenInvalid("EC key missing x coordinate".into()))?;
            let y = key
                .y
                .as_ref()
                .ok_or_else(|| SsoError::TokenInvalid("EC key missing y coordinate".into()))?;
            DecodingKey::from_ec_components(x, y)
                .map_err(|e| SsoError::TokenInvalid(format!("invalid EC key: {}", e)))
        }
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
        let result = rt.block_on(validator.validate(&token, "audience", "issuer", "https://example.com/.well-known/jwks.json"));
        assert!(matches!(result, Err(SsoError::TokenAlgorithmNone)));
    }

    #[test]
    fn test_validate_audience_mismatch() {
        let config = JwtValidationConfig::default();
        let validator = TokenValidator::new(config);

        let claims = TokenClaims {
            sub: "user".to_string(),
            iss: "issuer".to_string(),
            aud: "wrong".to_string(),
            exp: 9999999999,
            iat: 1000000000,
            ..Default::default()
        };

        let result = validator.validate_claims(&claims, "expected", "issuer");
        assert!(matches!(result, Err(SsoError::AudienceMismatch { .. })));
    }

    #[test]
    fn test_validate_issuer_mismatch() {
        let config = JwtValidationConfig::default();
        let validator = TokenValidator::new(config);

        let claims = TokenClaims {
            sub: "user".to_string(),
            iss: "wrong".to_string(),
            aud: "audience".to_string(),
            exp: 9999999999,
            iat: 1000000000,
            ..Default::default()
        };

        let result = validator.validate_claims(&claims, "audience", "expected");
        assert!(matches!(result, Err(SsoError::IssuerMismatch { .. })));
    }

    #[test]
    fn test_validate_expired_token() {
        let config = JwtValidationConfig::default();
        let validator = TokenValidator::new(config);

        let claims = TokenClaims {
            sub: "user".to_string(),
            iss: "issuer".to_string(),
            aud: "audience".to_string(),
            exp: 1000000000, // Expired in 2001
            iat: 999999000,
            ..Default::default()
        };

        let result = validator.validate_claims(&claims, "audience", "issuer");
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
