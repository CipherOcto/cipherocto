//! SSO Core Infrastructure (RFC-0949)
//!
//! Enterprise Single Sign-On support for OAuth2, OIDC, and SAML authentication.

pub mod blacklist;
pub mod blacklist_stoolap;
pub mod jwt;
pub mod mapper;
pub mod mapper_stoolap;
pub mod newtypes;
pub mod oauth2;
pub mod pkce;
pub mod saml;
pub mod scim;
pub mod scim_server;
pub mod session;

pub use self::blacklist::*;
pub use self::jwt::*;
pub use self::mapper::*;
pub use self::newtypes::*;
pub use self::oauth2::*;
pub use self::pkce::*;
pub use self::saml::*;
pub use self::scim::*;
pub use self::scim_server::*;
pub use self::session::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tracing;

// ============================================================================
// SSO Error Codes (18 total per RFC-0949)
// ============================================================================

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code", content = "message")]
pub enum SsoError {
    #[error("SSO provider not found: {0}")]
    ProviderNotFound(String),
    #[error("SSO provider disabled: {0}")]
    ProviderDisabled(String),
    #[error("Invalid OAuth2 state parameter")]
    InvalidState,
    #[error("Invalid authorization code")]
    InvalidCode,
    #[error("SSO token expired")]
    TokenExpired,
    #[error("SSO token revoked")]
    TokenRevoked,
    #[error("SSO token invalid: {0}")]
    TokenInvalid(String),
    #[error("Unsupported JWT algorithm: {0}")]
    TokenAlgorithmUnsupported(String),
    #[error("JWT algorithm 'none' is not allowed")]
    TokenAlgorithmNone,
    #[error("JWT audience mismatch: expected {expected}, got {actual}")]
    AudienceMismatch { expected: String, actual: String },
    #[error("JWT issuer mismatch: expected {expected}, got {actual}")]
    IssuerMismatch { expected: String, actual: String },
    #[error("SAML signature invalid: {0}")]
    SamlSignatureInvalid(String),
    #[error("SAML assertion expired")]
    SamlAssertionExpired,
    #[error("SAML audience mismatch: expected {expected}, got {actual:?}")]
    SamlAudienceMismatch {
        expected: String,
        actual: Vec<String>,
    },
    #[error("SAML subject confirmation method invalid: expected 'bearer', got {actual:?}")]
    SamlSubjectConfirmationInvalid { actual: Vec<String> },
    #[error("SAML subject confirmation data expired: not_on_or_after {not_on_or_after}")]
    SamlSubjectConfirmationExpired { not_on_or_after: String },
    #[error("SAML replay detected: assertion_id {assertion_id}")]
    SamlReplayDetected { assertion_id: String },
    #[error("No API key mapping for SSO subject: {0}")]
    NoKeyMapping(String),
    #[error("SSO user deactivated: {0}")]
    UserDeactivated(String),
    #[error("SSO provider error: {0}")]
    ProviderError(String),
    #[error("SSO rate limited")]
    RateLimited,
}

impl SsoError {
    /// HTTP status code for this error
    pub fn status_code(&self) -> u16 {
        match self {
            Self::ProviderNotFound(_) | Self::ProviderDisabled(_) => 404,
            Self::InvalidState | Self::InvalidCode => 400,
            Self::TokenExpired
            | Self::TokenRevoked
            | Self::TokenInvalid(_)
            | Self::TokenAlgorithmUnsupported(_)
            | Self::TokenAlgorithmNone => 401,
            Self::AudienceMismatch { .. } | Self::IssuerMismatch { .. } => 401,
            Self::SamlSignatureInvalid(_)
            | Self::SamlAssertionExpired
            | Self::SamlAudienceMismatch { .. }
            | Self::SamlSubjectConfirmationInvalid { .. }
            | Self::SamlSubjectConfirmationExpired { .. }
            | Self::SamlReplayDetected { .. } => 401,
            Self::NoKeyMapping(_) | Self::UserDeactivated(_) => 403,
            Self::ProviderError(_) => 502,
            Self::RateLimited => 429,
        }
    }

    /// Error type string for JSON response
    pub fn error_type(&self) -> &'static str {
        match self {
            Self::ProviderNotFound(_) | Self::ProviderDisabled(_) => "not_found",
            Self::InvalidState | Self::InvalidCode => "invalid_request",
            Self::TokenExpired
            | Self::TokenRevoked
            | Self::TokenInvalid(_)
            | Self::TokenAlgorithmUnsupported(_)
            | Self::TokenAlgorithmNone => "authentication_error",
            Self::AudienceMismatch { .. } | Self::IssuerMismatch { .. } => "authentication_error",
            Self::SamlSignatureInvalid(_)
            | Self::SamlAssertionExpired
            | Self::SamlAudienceMismatch { .. }
            | Self::SamlSubjectConfirmationInvalid { .. }
            | Self::SamlSubjectConfirmationExpired { .. }
            | Self::SamlReplayDetected { .. } => "authentication_error",
            Self::NoKeyMapping(_) | Self::UserDeactivated(_) => "authorization_error",
            Self::ProviderError(_) => "provider_error",
            Self::RateLimited => "rate_limit_error",
        }
    }

    /// Error response JSON
    pub fn to_error_response(&self) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "message": self.to_string(),
                "type": self.error_type(),
                "code": self.error_code(),
                "status_code": self.status_code(),
            }
        })
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::ProviderNotFound(_) => "sso_provider_not_found",
            Self::ProviderDisabled(_) => "sso_provider_disabled",
            Self::InvalidState => "sso_invalid_state",
            Self::InvalidCode => "sso_invalid_code",
            Self::TokenExpired => "sso_token_expired",
            Self::TokenRevoked => "sso_token_revoked",
            Self::TokenInvalid(_) => "sso_token_invalid",
            Self::TokenAlgorithmUnsupported(_) => "sso_token_algorithm_unsupported",
            Self::TokenAlgorithmNone => "sso_token_algorithm_none",
            Self::AudienceMismatch { .. } => "sso_audience_mismatch",
            Self::IssuerMismatch { .. } => "sso_issuer_mismatch",
            Self::SamlSignatureInvalid(_) => "sso_saml_signature_invalid",
            Self::SamlAssertionExpired => "sso_saml_assertion_expired",
            Self::SamlAudienceMismatch { .. } => "sso_saml_audience_mismatch",
            Self::SamlSubjectConfirmationInvalid { .. } => "sso_saml_subject_confirmation_invalid",
            Self::SamlSubjectConfirmationExpired { .. } => "sso_saml_subject_confirmation_expired",
            Self::SamlReplayDetected { .. } => "sso_saml_replay_detected",
            Self::NoKeyMapping(_) => "sso_no_key_mapping",
            Self::UserDeactivated(_) => "sso_user_deactivated",
            Self::ProviderError(_) => "sso_provider_error",
            Self::RateLimited => "sso_rate_limited",
        }
    }
}

// ============================================================================
// SSO Flow Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SsoFlow {
    /// OAuth2 Authorization Code + PKCE (interactive users)
    AuthorizationCode {
        client_id: String,
        redirect_uri: String,
        scopes: Vec<String>,
    },
    /// OAuth2 Client Credentials (service accounts)
    ClientCredentials {
        client_id: String,
        client_secret: String,
        scopes: Vec<String>,
    },
    /// SAML 2.0 SP-initiated (enterprise IdPs)
    Saml {
        idp_metadata_url: String,
        sp_entity_id: String,
        acs_url: String,
    },
    /// OpenID Connect (hybrid)
    Oidc {
        issuer: String,
        client_id: String,
        scopes: Vec<String>,
    },
}

// ============================================================================
// Identity Provider
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityProvider {
    /// Unique provider ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Configuration
    pub config: ProviderConfig,
    /// Enabled status
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Auto-provision users on first SSO login
    #[serde(default)]
    pub auto_provision: bool,
    /// Default team for auto-provisioned users
    pub default_team: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProviderType {
    Okta,
    AzureAd,
    GoogleWorkspace,
    Auth0,
    GenericOidc,
    GenericSaml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// OAuth2/OIDC settings
    pub client_id: Option<String>,
    /// OAuth2 client secret.
    ///
    /// M4-saml-crypto-hygiene AC asked to wrap this in
    /// `Zeroizing<String>` / `Secret<String>`. **Deferred** to
    /// M6-saml-error-path-types which lands the comprehensive
    /// newtype work (`Secret<T>`, `EntityId`, `Audience`,
    /// `Recipient`, `Email`) including a transparent
    /// `Serialize`/`Deserialize` shim. The standalone wrap would
    /// require either (a) a `#[serde(with)]` adapter on every
    /// field, or (b) propagating `Zeroizing::new(...)` through
    /// ~25 construction sites in `mod.rs`, `oauth2.rs`, and
    /// `scim.rs` with non-trivial ergonomics for `bearer_auth`
    /// which expects `&str`. M6 reuses the ergonomic newtype
    /// path; until then, secret material sits in heap memory
    /// after drop. Findings F1-010 / F2-004 still partially
    /// mitigated because `SamlAssertionParserImpl.idp_certificate`
    /// IS zeroized on parser drop — the parser is the last
    /// in-memory holder of the cert bytes.
    pub client_secret: Option<String>,
    pub issuer: Option<String>,
    pub scopes: Option<Vec<String>>,
    /// SAML settings
    pub idp_metadata_url: Option<String>,
    pub sp_entity_id: Option<String>,
    pub acs_url: Option<String>,
    /// IdP certificate (DER-encoded) for SAML signature
    /// validation. Same deferral note as `client_secret`
    /// above — M6 will land `Option<Secret<Vec<u8>>>`. The
    /// parser side (`SamlAssertionParserImpl.idp_certificate`)
    /// IS zeroized on drop today (see saml.rs).
    pub idp_certificate: Option<Vec<u8>>,
    /// SCIM settings
    pub scim_url: Option<String>,
    /// SCIM bearer token. Deferral note per `client_secret`.
    pub scim_token: Option<String>,
}

impl ProviderConfig {
    /// Validate provider config based on provider type (RFC-0949 validation rules)
    pub fn validate(&self, provider_type: &ProviderType) -> Result<(), String> {
        match provider_type {
            ProviderType::Okta | ProviderType::AzureAd => {
                if self.client_id.is_none() {
                    return Err(format!("{:?} requires client_id", provider_type));
                }
                if self.client_secret.is_none() {
                    return Err(format!("{:?} requires client_secret", provider_type));
                }
                if self.issuer.is_none() {
                    return Err(format!("{:?} requires issuer", provider_type));
                }
            }
            ProviderType::GoogleWorkspace => {
                if self.client_id.is_none() {
                    return Err("GoogleWorkspace requires client_id".to_string());
                }
                if self.client_secret.is_none() {
                    return Err("GoogleWorkspace requires client_secret".to_string());
                }
            }
            ProviderType::Auth0 => {
                if self.client_id.is_none() {
                    return Err("Auth0 requires client_id".to_string());
                }
                if self.client_secret.is_none() {
                    return Err("Auth0 requires client_secret".to_string());
                }
                if self.issuer.is_none() {
                    return Err("Auth0 requires issuer".to_string());
                }
            }
            ProviderType::GenericOidc => {
                if self.client_id.is_none() {
                    return Err("GenericOidc requires client_id".to_string());
                }
                if self.issuer.is_none() {
                    return Err("GenericOidc requires issuer".to_string());
                }
            }
            ProviderType::GenericSaml => {
                if self.idp_metadata_url.is_none() {
                    return Err("GenericSaml requires idp_metadata_url".to_string());
                }
                if self.sp_entity_id.is_none() {
                    return Err("GenericSaml requires sp_entity_id".to_string());
                }
                if self.acs_url.is_none() {
                    return Err("GenericSaml requires acs_url".to_string());
                }
                // M5-saml-cert-pinning-weak-algo finding F1-009:
                // an IdP certificate MUST be pinned. Without it,
                // `SamlAssertionParserImpl::from_provider` returns
                // `ProviderError("Missing IdP certificate")` at
                // runtime; this surfaces the failure at config-
                // load time instead. The certificate may be empty
                // if the operator only set `idp_metadata_url`
                // intending metadata fetch to populate it later —
                // that path is allowed (length-only check).
                match self.idp_certificate.as_ref() {
                    None => {
                        return Err("GenericSaml requires idp_certificate (pinned DER cert) — \
                             idp_metadata_url alone is insufficient"
                            .to_string());
                    }
                    Some(bytes) if bytes.is_empty() => {
                        return Err(
                            "GenericSaml idp_certificate is empty — DER cert required".to_string()
                        );
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Warn-only startup health check (M5-saml-cert-pinning-weak-algo).
    ///
    /// Emits `tracing::warn!` (NOT an error — the provider may
    /// still bootstrap with metadata-only fetch in non-production
    /// deployments) when an IdP has `idp_metadata_url` configured
    /// without a pinned DER cert. Production deployers MUST pin
    /// a cert before trusting assertions.
    ///
    /// Returns the list of provider IDs that triggered the warn
    /// for callers who want to surface it in startup health
    /// endpoints.
    pub fn warn_unpinned_idp<'a, I>(provider_type: &ProviderType, providers: I) -> Vec<String>
    where
        I: IntoIterator<Item = (&'a String, &'a Self)>,
    {
        // Only relevant for SAML providers.
        if !matches!(provider_type, ProviderType::GenericSaml) {
            return Vec::new();
        }
        let mut warned = Vec::new();
        for (id, config) in providers {
            if config.idp_metadata_url.is_some()
                && config.idp_certificate.as_ref().is_none_or(|v| v.is_empty())
            {
                tracing::warn!(
                    provider_id = %id,
                    "SAML provider has idp_metadata_url set but no pinned IdP \
                     certificate (idp_certificate). Production deployments \
                     MUST pin a DER cert — relying on metadata-only fetch is \
                     not safe."
                );
                warned.push(id.clone());
            }
        }
        warned
    }
}

// ============================================================================
// Token Claims
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenClaims {
    /// Subject (user ID from IdP)
    pub sub: String,
    /// Email address
    pub email: Option<String>,
    /// Display name
    pub name: Option<String>,
    /// IdP groups
    #[serde(default)]
    pub groups: Vec<String>,
    /// Mapped roles
    #[serde(default)]
    pub roles: Vec<String>,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at (Unix timestamp)
    pub iat: i64,
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
}

// ============================================================================
// SSO User (after authentication)
// ============================================================================

#[derive(Debug, Clone)]
pub struct SsoUser {
    /// IdP subject identifier (stable across sessions)
    pub sub: String,
    /// Email address
    pub email: Option<String>,
    /// Display name
    pub name: Option<String>,
    /// IdP groups
    pub groups: Vec<String>,
    /// Mapped roles
    pub roles: Vec<String>,
    /// Provider ID that authenticated this user
    pub provider_id: String,
}

// ============================================================================
// SSO Key Metadata (extension for VirtualKey.metadata)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SsoKeyMetadata {
    /// IdP subject identifier (stable across sessions)
    pub sso_subject: Option<String>,
    /// SSO provider ID (references IdentityProvider.id)
    pub sso_provider: Option<String>,
}

// ============================================================================
// SsoConfig (added to config.rs)
// ============================================================================

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SsoConfig {
    /// SSO enabled
    #[serde(default)]
    pub enabled: bool,
    /// Identity providers
    #[serde(default)]
    pub providers: Vec<IdentityProvider>,
    /// Role mapping (IdP group → quota-router role)
    #[serde(default)]
    pub role_mapping: HashMap<String, String>,
    /// Team mapping (IdP group → quota-router team)
    #[serde(default)]
    pub team_mapping: HashMap<String, String>,
    /// Token configuration
    #[serde(default)]
    pub token: TokenConfig,
    /// JWT validation configuration
    #[serde(default)]
    pub jwt: JwtValidationConfig,
    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: SsoRateLimitConfig,
    /// Optional token blacklist storage for logout/revocation support
    #[serde(skip)]
    pub blacklist_storage: Option<std::sync::Arc<dyn TokenBlacklistStorage>>,
}

impl std::fmt::Debug for SsoConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SsoConfig")
            .field("enabled", &self.enabled)
            .field("providers", &self.providers)
            .field("role_mapping", &self.role_mapping)
            .field("team_mapping", &self.team_mapping)
            .field("token", &self.token)
            .field("jwt", &self.jwt)
            .field("rate_limit", &self.rate_limit)
            .field(
                "blacklist_storage",
                &if self.blacklist_storage.is_some() {
                    "Some(...)"
                } else {
                    "None"
                },
            )
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConfig {
    /// Access token TTL in seconds (default: 3600 = 1h)
    #[serde(default = "default_access_token_ttl")]
    pub access_token_ttl: u64,
    /// Refresh token TTL in seconds (default: 604800 = 7d)
    #[serde(default = "default_refresh_token_ttl")]
    pub refresh_token_ttl: u64,
    /// Session TTL in seconds (default: 1800 = 30m)
    #[serde(default = "default_session_ttl")]
    pub session_ttl: u64,
}

impl Default for TokenConfig {
    fn default() -> Self {
        Self {
            access_token_ttl: default_access_token_ttl(),
            refresh_token_ttl: default_refresh_token_ttl(),
            session_ttl: default_session_ttl(),
        }
    }
}

fn default_access_token_ttl() -> u64 {
    3600
}
fn default_refresh_token_ttl() -> u64 {
    604800
}
fn default_session_ttl() -> u64 {
    1800
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtValidationConfig {
    /// JWKS cache TTL in seconds (default: 3600)
    #[serde(default = "default_jwks_cache_ttl")]
    pub jwks_cache_ttl: u64,
    /// Clock skew tolerance in seconds (default: 30)
    #[serde(default = "default_clock_skew")]
    pub clock_skew: u64,
    /// Supported JWT algorithms
    #[serde(default = "default_supported_algorithms")]
    pub supported_algorithms: Vec<JwtAlgorithm>,
}

impl Default for JwtValidationConfig {
    fn default() -> Self {
        Self {
            jwks_cache_ttl: default_jwks_cache_ttl(),
            clock_skew: default_clock_skew(),
            supported_algorithms: default_supported_algorithms(),
        }
    }
}

fn default_jwks_cache_ttl() -> u64 {
    3600
}
fn default_clock_skew() -> u64 {
    30
}
fn default_supported_algorithms() -> Vec<JwtAlgorithm> {
    vec![
        JwtAlgorithm::RS256,
        JwtAlgorithm::RS384,
        JwtAlgorithm::RS512,
        JwtAlgorithm::ES256,
        JwtAlgorithm::ES384,
        JwtAlgorithm::PS256,
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JwtAlgorithm {
    RS256,
    RS384,
    RS512,
    ES256,
    ES384,
    PS256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoRateLimitConfig {
    /// Login attempts per minute per IP (default: 10)
    #[serde(default = "default_login_rate")]
    pub login_per_minute: u32,
    /// Token refresh per minute per user (default: 30)
    #[serde(default = "default_refresh_rate")]
    pub refresh_per_minute: u32,
    /// Token revocation per minute per user (default: 20)
    #[serde(default = "default_revoke_rate")]
    pub revoke_per_minute: u32,
    /// SSO callback per minute per IP (default: 20)
    #[serde(default = "default_callback_rate")]
    pub callback_per_minute: u32,
}

impl Default for SsoRateLimitConfig {
    fn default() -> Self {
        Self {
            login_per_minute: default_login_rate(),
            refresh_per_minute: default_refresh_rate(),
            revoke_per_minute: default_revoke_rate(),
            callback_per_minute: default_callback_rate(),
        }
    }
}

fn default_login_rate() -> u32 {
    10
}
fn default_refresh_rate() -> u32 {
    30
}
fn default_revoke_rate() -> u32 {
    20
}
fn default_callback_rate() -> u32 {
    20
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sso_error_codes() {
        // Verify all 18 error codes
        let errors = vec![
            SsoError::ProviderNotFound("test".into()),
            SsoError::ProviderDisabled("test".into()),
            SsoError::InvalidState,
            SsoError::InvalidCode,
            SsoError::TokenExpired,
            SsoError::TokenRevoked,
            SsoError::TokenInvalid("test".into()),
            SsoError::TokenAlgorithmUnsupported("test".into()),
            SsoError::TokenAlgorithmNone,
            SsoError::AudienceMismatch {
                expected: "a".into(),
                actual: "b".into(),
            },
            SsoError::IssuerMismatch {
                expected: "a".into(),
                actual: "b".into(),
            },
            SsoError::SamlSignatureInvalid("test".into()),
            SsoError::SamlAssertionExpired,
            SsoError::SamlAudienceMismatch {
                expected: "test".into(),
                actual: vec!["test".into()],
            },
            SsoError::NoKeyMapping("test".into()),
            SsoError::UserDeactivated("test".into()),
            SsoError::ProviderError("test".into()),
            SsoError::RateLimited,
        ];
        assert_eq!(errors.len(), 18);

        for err in &errors {
            let resp = err.to_error_response();
            assert!(resp["error"]["code"].is_string());
            assert!(resp["error"]["status_code"].is_number());
        }
    }

    #[test]
    fn test_provider_config_validation() {
        // Okta requires client_id, client_secret, issuer
        let config = ProviderConfig {
            client_id: None,
            client_secret: None,
            issuer: None,
            scopes: None,
            idp_metadata_url: None,
            sp_entity_id: None,
            acs_url: None,
            idp_certificate: None,
            scim_url: None,
            scim_token: None,
        };
        assert!(config.validate(&ProviderType::Okta).is_err());

        let config = ProviderConfig {
            client_id: Some("id".into()),
            client_secret: Some("secret".into()),
            issuer: Some("https://okta.com".into()),
            scopes: None,
            idp_metadata_url: None,
            sp_entity_id: None,
            acs_url: None,
            idp_certificate: None,
            scim_url: None,
            scim_token: None,
        };
        assert!(config.validate(&ProviderType::Okta).is_ok());

        // GenericSaml requires idp_metadata_url, sp_entity_id, acs_url,
        // AND (M5-saml-cert-pinning-weak-algo finding F1-009)
        // a non-None, non-empty idp_certificate. The metadata URL
        // alone is silently accepted at provider startup today —
        // that fails closed starting with the M5 verify gate.
        let saml_config = ProviderConfig {
            client_id: None,
            client_secret: None,
            issuer: None,
            scopes: None,
            idp_metadata_url: Some("https://idp.com/metadata".into()),
            sp_entity_id: Some("sp-entity".into()),
            acs_url: Some("https://app.com/acs".into()),
            idp_certificate: Some(vec![0xDE, 0xAD, 0xBE, 0xEF]),
            scim_url: None,
            scim_token: None,
        };
        assert!(saml_config.validate(&ProviderType::GenericSaml).is_ok());

        // M5 negative: missing certificate MUST fail closed.
        let saml_missing_cert = ProviderConfig {
            idp_certificate: None,
            ..saml_config.clone()
        };
        let err = saml_missing_cert
            .validate(&ProviderType::GenericSaml)
            .expect_err("missing cert must fail");
        assert!(
            err.contains("idp_certificate"),
            "error must mention idp_certificate, got: {:?}",
            err
        );

        // M5 negative: empty certificate (zero-byte DER) MUST fail.
        let saml_empty_cert = ProviderConfig {
            idp_certificate: Some(vec![]),
            ..saml_config.clone()
        };
        let err = saml_empty_cert
            .validate(&ProviderType::GenericSaml)
            .expect_err("empty cert must fail");
        assert!(
            err.contains("empty"),
            "error must mention empty, got: {:?}",
            err
        );
    }

    #[test]
    fn test_sso_config_defaults() {
        let config = SsoConfig::default();
        assert!(!config.enabled);
        assert!(config.providers.is_empty());
        assert_eq!(config.token.access_token_ttl, 3600);
        assert_eq!(config.token.refresh_token_ttl, 604800);
        assert_eq!(config.token.session_ttl, 1800);
        assert_eq!(config.jwt.jwks_cache_ttl, 3600);
        assert_eq!(config.jwt.clock_skew, 30);
        assert_eq!(config.jwt.supported_algorithms.len(), 6);
        assert_eq!(config.rate_limit.login_per_minute, 10);
    }

    #[test]
    fn test_sso_error_status_codes() {
        let errors = vec![
            (SsoError::ProviderNotFound("test".into()), 404),
            (SsoError::ProviderDisabled("test".into()), 404),
            (SsoError::InvalidState, 400),
            (SsoError::InvalidCode, 400),
            (SsoError::TokenExpired, 401),
            (SsoError::TokenRevoked, 401),
            (SsoError::TokenInvalid("test".into()), 401),
            (SsoError::TokenAlgorithmUnsupported("test".into()), 401),
            (SsoError::TokenAlgorithmNone, 401),
            (
                SsoError::AudienceMismatch {
                    expected: "a".into(),
                    actual: "b".into(),
                },
                401,
            ),
            (
                SsoError::IssuerMismatch {
                    expected: "a".into(),
                    actual: "b".into(),
                },
                401,
            ),
            (SsoError::SamlSignatureInvalid("test".into()), 401),
            (SsoError::SamlAssertionExpired, 401),
            (
                SsoError::SamlAudienceMismatch {
                    expected: "test".into(),
                    actual: vec!["test".into()],
                },
                401,
            ),
            (SsoError::NoKeyMapping("test".into()), 403),
            (SsoError::UserDeactivated("test".into()), 403),
            (SsoError::ProviderError("test".into()), 502),
            (SsoError::RateLimited, 429),
        ];

        for (err, expected_code) in errors {
            assert_eq!(err.status_code(), expected_code);
        }
    }

    #[test]
    fn test_sso_error_types() {
        let errors = vec![
            (SsoError::ProviderNotFound("test".into()), "not_found"),
            (SsoError::ProviderDisabled("test".into()), "not_found"),
            (SsoError::InvalidState, "invalid_request"),
            (SsoError::InvalidCode, "invalid_request"),
            (SsoError::TokenExpired, "authentication_error"),
            (SsoError::TokenRevoked, "authentication_error"),
        ];

        for (err, expected_type) in errors {
            assert_eq!(err.error_type(), expected_type);
        }
    }

    #[test]
    fn test_provider_config_validation_generic_oauth() {
        let config = ProviderConfig {
            client_id: Some("id".into()),
            client_secret: Some("secret".into()),
            issuer: Some("https://oauth.example.com".into()),
            scopes: None,
            idp_metadata_url: None,
            sp_entity_id: None,
            acs_url: None,
            idp_certificate: None,
            scim_url: None,
            scim_token: None,
        };
        assert!(config.validate(&ProviderType::GenericOidc).is_ok());
    }

    #[test]
    fn test_provider_config_validation_generic_oidc() {
        let config = ProviderConfig {
            client_id: Some("id".into()),
            client_secret: Some("secret".into()),
            issuer: Some("https://oidc.example.com".into()),
            scopes: None,
            idp_metadata_url: None,
            sp_entity_id: None,
            acs_url: None,
            idp_certificate: None,
            scim_url: None,
            scim_token: None,
        };
        assert!(config.validate(&ProviderType::GenericOidc).is_ok());
    }

    #[test]
    fn test_warn_unpinned_idp_saml_no_cert_warns() {
        use std::collections::HashMap;
        let mut providers_map: HashMap<String, ProviderConfig> = HashMap::new();
        providers_map.insert(
            "unpinned-saml".to_string(),
            ProviderConfig {
                idp_metadata_url: Some("https://idp.example.com/metadata".into()),
                idp_certificate: None,
                ..make_minimal_saml_config()
            },
        );
        let warned =
            ProviderConfig::warn_unpinned_idp(&ProviderType::GenericSaml, providers_map.iter());
        assert_eq!(warned, vec!["unpinned-saml".to_string()]);
    }

    #[test]
    fn test_warn_unpinned_idp_saml_with_cert_no_warn() {
        use std::collections::HashMap;
        let mut providers_map: HashMap<String, ProviderConfig> = HashMap::new();
        providers_map.insert(
            "pinned-saml".to_string(),
            ProviderConfig {
                idp_metadata_url: Some("https://idp.example.com/metadata".into()),
                idp_certificate: Some(vec![0xDE, 0xAD, 0xBE, 0xEF]),
                ..make_minimal_saml_config()
            },
        );
        let warned =
            ProviderConfig::warn_unpinned_idp(&ProviderType::GenericSaml, providers_map.iter());
        assert!(warned.is_empty(), "pinned cert must NOT warn");
    }

    #[test]
    fn test_warn_unpinned_idp_non_saml_provider_no_warn() {
        use std::collections::HashMap;
        let mut providers_map: HashMap<String, ProviderConfig> = HashMap::new();
        providers_map.insert(
            "okta-1".to_string(),
            ProviderConfig {
                client_id: Some("id".into()),
                client_secret: Some("secret".into()),
                issuer: Some("https://okta.com".into()),
                ..make_minimal_saml_config()
            },
        );
        let warned = ProviderConfig::warn_unpinned_idp(&ProviderType::Okta, providers_map.iter());
        assert!(
            warned.is_empty(),
            "non-SAML provider type must skip the check"
        );
    }

    /// Minimal saml-default `ProviderConfig` used as base for the
    /// warn-helper tests. Field order matches the struct definition
    /// (required by struct-update syntax `..`).
    fn make_minimal_saml_config() -> ProviderConfig {
        ProviderConfig {
            client_id: None,
            client_secret: None,
            issuer: None,
            scopes: None,
            idp_metadata_url: None,
            sp_entity_id: None,
            acs_url: None,
            idp_certificate: None,
            scim_url: None,
            scim_token: None,
        }
    }
}
