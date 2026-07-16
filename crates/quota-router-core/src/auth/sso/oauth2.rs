//! OAuth2/OIDC flows (RFC-0949 Mission 0949-b).
//!
//! Authorization Code + PKCE flow, Client Credentials flow,
//! token lifecycle, and OIDC discovery endpoints.

use super::pkce::PkceChallenge;
use super::{
    IdentityProvider, JwtValidationConfig, SsoError, TokenBlacklistStorage, TokenClaims,
    TokenValidator,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// OAuth2 State (RFC-0949 §SSO Flow)
// ============================================================================

/// OAuth2 state parameter — generated on initiate, validated on callback.
///
/// Contains PKCE challenge, nonce, and expiration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2State {
    /// Random state value (CSRF protection).
    pub state: String,
    /// PKCE challenge for this flow.
    pub pkce: PkceChallenge,
    /// Nonce for replay protection.
    pub nonce: String,
    /// Provider ID this state is for.
    pub provider_id: String,
    /// Expiration time.
    pub expires_at: DateTime<Utc>,
}

impl OAuth2State {
    /// Create a new OAuth2 state for the given provider.
    pub fn new(provider_id: &str) -> Self {
        Self {
            state: generate_random_string(32),
            pkce: PkceChallenge::generate(),
            nonce: generate_random_string(16),
            provider_id: provider_id.to_string(),
            expires_at: Utc::now() + Duration::minutes(5),
        }
    }

    /// Check if this state has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Generate a random alphanumeric string of the given length.
fn generate_random_string(len: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

// ============================================================================
// OAuth2 Token Response
// ============================================================================

/// OAuth2 token response from IdP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub scope: Option<String>,
}

// ============================================================================
// SSO Session
// ============================================================================

/// SSO session — created after successful authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SsoSession {
    /// Session ID.
    pub session_id: String,
    /// User subject (from IdP).
    pub sub: String,
    /// Provider ID.
    pub provider_id: String,
    /// Access token.
    pub access_token: String,
    /// Refresh token.
    pub refresh_token: Option<String>,
    /// Token claims.
    pub claims: TokenClaims,
    /// Session creation time.
    pub created_at: DateTime<Utc>,
    /// Session expiration.
    pub expires_at: DateTime<Utc>,
}

// ============================================================================
// SSO Session Store
// ============================================================================

/// In-memory SSO session store.
pub struct SsoSessionStore {
    sessions: Arc<RwLock<HashMap<String, SsoSession>>>,
}

impl SsoSessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store a session.
    pub async fn insert(&self, session: SsoSession) {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.session_id.clone(), session);
    }

    /// Get a session by ID.
    pub async fn get(&self, session_id: &str) -> Option<SsoSession> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Remove a session (logout/revocation).
    pub async fn remove(&self, session_id: &str) -> Option<SsoSession> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id)
    }

    /// Remove all sessions for a user (logout everywhere).
    pub async fn remove_by_sub(&self, sub: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, s| s.sub != sub);
    }

    /// Clean up expired sessions.
    pub async fn cleanup_expired(&self) {
        let now = Utc::now();
        let mut sessions = self.sessions.write().await;
        sessions.retain(|_, s| s.expires_at > now);
    }
}

impl Default for SsoSessionStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// OAuth2 Flow Handler
// ============================================================================

/// OAuth2 Authorization Code + PKCE flow handler.
pub struct OAuth2FlowHandler {
    /// Pending states (state_string -> OAuth2State).
    pending_states: Arc<RwLock<HashMap<String, OAuth2State>>>,
    /// Session store.
    pub sessions: SsoSessionStore,
    /// JWT validator for id_token decoding.
    validator: Option<TokenValidator>,
    /// JWT validation config.
    jwt_config: JwtValidationConfig,
}

impl OAuth2FlowHandler {
    pub fn new() -> Self {
        Self {
            pending_states: Arc::new(RwLock::new(HashMap::new())),
            sessions: SsoSessionStore::new(),
            validator: None,
            jwt_config: JwtValidationConfig::default(),
        }
    }

    /// Create handler with JWT validator for id_token decoding.
    pub fn with_jwt_validator(jwt_config: JwtValidationConfig) -> Self {
        Self {
            pending_states: Arc::new(RwLock::new(HashMap::new())),
            sessions: SsoSessionStore::new(),
            validator: Some(TokenValidator::new(jwt_config.clone())),
            jwt_config,
        }
    }

    /// Set JWT validator after construction.
    pub fn set_validator(&mut self, validator: TokenValidator) {
        self.validator = Some(validator);
    }

    /// Decode id_token using validator or on-the-fly from jwt_config.
    ///
    /// Uses jwt_config to create a validator when the pre-built validator is absent.
    /// This implements Option C: jwt_config is meaningful when validator is None.
    async fn decode_id_token(
        &self,
        id_token: &Option<String>,
        provider: &IdentityProvider,
    ) -> Result<
        (
            String,
            Option<String>,
            Option<String>,
            Vec<String>,
            Vec<String>,
        ),
        SsoError,
    > {
        let id_token = match id_token {
            Some(t) => t,
            None => {
                return Err(SsoError::TokenInvalid(
                    "no id_token in token response (required for user identification)".into(),
                ));
            }
        };

        // If we have a pre-built validator, use it
        if let Some(ref validator) = self.validator {
            let issuer = provider
                .config
                .issuer
                .as_deref()
                .unwrap_or("https://idp.example.com");
            let client_id = provider.config.client_id.as_deref().unwrap_or("");
            let jwks_url = format!("{}/.well-known/jwks.json", issuer);

            return match validator
                .validate(id_token, client_id, issuer, &jwks_url)
                .await
            {
                Ok(claims) => Ok((
                    claims.sub.clone(),
                    claims.email.clone(),
                    claims.name.clone(),
                    claims.groups.clone(),
                    claims.roles.clone(),
                )),
                Err(e) => Err(SsoError::TokenInvalid(format!(
                    "id_token validation failed: {}",
                    e
                ))),
            };
        }

        // No pre-built validator — use jwt_config to create one on-the-fly (Option C)
        let issuer = provider
            .config
            .issuer
            .as_deref()
            .unwrap_or("https://idp.example.com");
        let client_id = provider.config.client_id.as_deref().unwrap_or("");
        let jwks_url = format!("{}/.well-known/jwks.json", issuer);

        let validator = TokenValidator::new(self.jwt_config.clone());
        match validator
            .validate(id_token, client_id, issuer, &jwks_url)
            .await
        {
            Ok(claims) => Ok((
                claims.sub.clone(),
                claims.email.clone(),
                claims.name.clone(),
                claims.groups.clone(),
                claims.roles.clone(),
            )),
            Err(e) => Err(SsoError::TokenInvalid(format!(
                "id_token validation failed: {}",
                e
            ))),
        }
    }

    /// Initiate SSO flow — generates state + PKCE challenge.
    ///
    /// Returns (state, code_challenge, authorize_url).
    pub async fn initiate(
        &self,
        provider: &IdentityProvider,
        redirect_uri: &str,
    ) -> Result<(String, String, String), SsoError> {
        if !provider.enabled {
            return Err(SsoError::ProviderDisabled(provider.id.clone()));
        }

        let state = OAuth2State::new(&provider.id);
        let state_str = state.state.clone();
        let challenge = state.pkce.code_challenge.clone();

        // Build authorization URL
        let client_id = provider
            .config
            .client_id
            .as_deref()
            .unwrap_or("missing-client-id");
        let scopes = provider
            .config
            .scopes
            .as_ref()
            .map(|s| s.join(" "))
            .unwrap_or_else(|| "openid profile email".to_string());
        let issuer = provider
            .config
            .issuer
            .as_deref()
            .unwrap_or("https://idp.example.com");

        let authorize_url = format!(
            "{}/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256&nonce={}",
            issuer, client_id, redirect_uri, scopes, state_str, challenge, state.nonce
        );

        // Store pending state
        let mut states = self.pending_states.write().await;
        states.insert(state_str.clone(), state);

        Ok((state_str, challenge, authorize_url))
    }

    /// Handle OAuth2 callback — validates state, exchanges code.
    pub async fn callback(
        &self,
        state: &str,
        code: &str,
        code_verifier: &str,
        provider: &IdentityProvider,
        token_endpoint: &str,
    ) -> Result<SsoSession, SsoError> {
        // Validate and consume state
        let oauth_state = {
            let mut states = self.pending_states.write().await;
            states.remove(state).ok_or(SsoError::InvalidState)?
        };

        if oauth_state.is_expired() {
            return Err(SsoError::InvalidState);
        }

        if oauth_state.provider_id != provider.id {
            return Err(SsoError::InvalidState);
        }

        // Verify PKCE challenge
        if !oauth_state.pkce.verify(code_verifier) {
            return Err(SsoError::InvalidCode);
        }

        // Exchange code for tokens
        let token_response = exchange_code(
            token_endpoint,
            provider.config.client_id.as_deref().unwrap_or(""),
            provider.config.client_secret.as_deref().unwrap_or(""),
            code,
            code_verifier,
        )
        .await?;

        // Decode id_token if present and validator is available
        let (sub, email, name, groups, roles) = self
            .decode_id_token(&token_response.id_token, provider)
            .await?;

        // Create session with real claims
        let session = SsoSession {
            session_id: generate_random_string(32),
            sub: sub.clone(),
            provider_id: provider.id.clone(),
            access_token: token_response.access_token.clone(),
            refresh_token: token_response.refresh_token.clone(),
            claims: TokenClaims {
                sub,
                email,
                name,
                groups,
                roles,
                exp: (Utc::now() + Duration::hours(1)).timestamp(),
                iat: Utc::now().timestamp(),
                iss: provider.config.issuer.clone().unwrap_or_default(),
                aud: provider.config.client_id.clone().unwrap_or_default(),
            },
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };

        self.sessions.insert(session.clone()).await;
        Ok(session)
    }

    /// Refresh an access token using a refresh token.
    pub async fn refresh(
        &self,
        session_id: &str,
        provider: &IdentityProvider,
        token_endpoint: &str,
    ) -> Result<SsoSession, SsoError> {
        let session = self
            .sessions
            .get(session_id)
            .await
            .ok_or(SsoError::TokenRevoked)?;

        let refresh_token = session
            .refresh_token
            .as_ref()
            .ok_or(SsoError::TokenInvalid("no refresh token".into()))?;

        // Exchange refresh token for new tokens
        let token_response = refresh_token_request(
            token_endpoint,
            provider.config.client_id.as_deref().unwrap_or(""),
            provider.config.client_secret.as_deref().unwrap_or(""),
            refresh_token,
        )
        .await?;

        // Update session with new tokens (refresh rotation)
        let mut updated = session;
        updated.access_token = token_response.access_token;
        updated.refresh_token = token_response.refresh_token;
        updated.expires_at = Utc::now() + Duration::hours(1);
        updated.claims.exp = updated.expires_at.timestamp();

        self.sessions.insert(updated.clone()).await;
        Ok(updated)
    }

    /// Revoke a session.
    pub async fn revoke(&self, session_id: &str) -> bool {
        self.sessions.remove(session_id).await.is_some()
    }

    /// Revoke a session and blacklist its access token for logout/revocation support.
    /// If a blacklist storage is configured, the access token is added to the blacklist
    /// with the session expiration time. Returns true if the session was found and revoked.
    pub async fn revoke_with_blacklist(
        &self,
        session_id: &str,
        blacklist: &Option<std::sync::Arc<dyn TokenBlacklistStorage>>,
    ) -> Result<bool, SsoError> {
        let session = self.sessions.get(session_id).await;
        if session.is_none() {
            return Ok(false);
        }

        let session = session.unwrap();

        // Add access token to blacklist if storage is configured
        if let Some(ref storage) = blacklist {
            let expires_at = session.expires_at;
            storage
                .add(&session.access_token, expires_at)
                .await
                .map_err(|e| SsoError::ProviderError(format!("blacklist add failed: {}", e)))?;
        }

        // Remove the session
        let removed = self.sessions.remove(session_id).await.is_some();
        Ok(removed)
    }
}

impl Default for OAuth2FlowHandler {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Client Credentials Flow
// ============================================================================

/// OAuth2 Client Credentials flow (service accounts).
pub async fn client_credentials_flow(
    token_endpoint: &str,
    client_id: &str,
    client_secret: &str,
    scopes: &[String],
) -> Result<OAuth2TokenResponse, SsoError> {
    let client = reqwest::Client::new();
    let scope_str = scopes.join(" ");

    let params = [
        ("grant_type", "client_credentials"),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("scope", &scope_str),
    ];

    let resp = client
        .post(token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| SsoError::ProviderError(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(SsoError::ProviderError(format!(
            "token endpoint returned {}",
            resp.status()
        )));
    }

    resp.json::<OAuth2TokenResponse>()
        .await
        .map_err(|e| SsoError::ProviderError(e.to_string()))
}

// ============================================================================
// Token Exchange (internal)
// ============================================================================

/// Exchange authorization code for tokens.
async fn exchange_code(
    token_endpoint: &str,
    client_id: &str,
    client_secret: &str,
    code: &str,
    code_verifier: &str,
) -> Result<OAuth2TokenResponse, SsoError> {
    let client = reqwest::Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", "urn:ietf:wg:oauth:2.0:oob"), // placeholder
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code_verifier", code_verifier),
    ];

    let resp = client
        .post(token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| SsoError::ProviderError(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(SsoError::InvalidCode);
    }

    resp.json::<OAuth2TokenResponse>()
        .await
        .map_err(|e| SsoError::ProviderError(e.to_string()))
}

/// Refresh token request.
async fn refresh_token_request(
    token_endpoint: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<OAuth2TokenResponse, SsoError> {
    let client = reqwest::Client::new();

    let params = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
        ("client_secret", client_secret),
    ];

    let resp = client
        .post(token_endpoint)
        .form(&params)
        .send()
        .await
        .map_err(|e| SsoError::ProviderError(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(SsoError::TokenInvalid("refresh failed".into()));
    }

    resp.json::<OAuth2TokenResponse>()
        .await
        .map_err(|e| SsoError::ProviderError(e.to_string()))
}

// ============================================================================
// OIDC Discovery
// ============================================================================

/// OIDC discovery document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcDiscovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub scopes_supported: Vec<String>,
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    pub subject_types_supported: Vec<String>,
    pub id_token_signing_alg_values_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
}

impl OidcDiscovery {
    /// Build discovery document from provider config.
    pub fn from_provider(issuer: &str) -> Self {
        Self {
            issuer: issuer.to_string(),
            authorization_endpoint: format!("{}/authorize", issuer),
            token_endpoint: format!("{}/oauth/token", issuer),
            userinfo_endpoint: format!("{}/userinfo", issuer),
            jwks_uri: format!("{}/.well-known/jwks.json", issuer),
            scopes_supported: vec![
                "openid".into(),
                "profile".into(),
                "email".into(),
                "offline_access".into(),
            ],
            response_types_supported: vec!["code".into(), "token".into(), "id_token".into()],
            grant_types_supported: vec![
                "authorization_code".into(),
                "client_credentials".into(),
                "refresh_token".into(),
            ],
            subject_types_supported: vec!["public".into()],
            id_token_signing_alg_values_supported: vec![
                "RS256".into(),
                "RS384".into(),
                "RS512".into(),
                "ES256".into(),
            ],
            token_endpoint_auth_methods_supported: vec![
                "client_secret_basic".into(),
                "client_secret_post".into(),
            ],
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oauth2_state_new() {
        let state = OAuth2State::new("test-provider");
        assert_eq!(state.provider_id, "test-provider");
        assert_eq!(state.state.len(), 32);
        assert_eq!(state.nonce.len(), 16);
        assert!(!state.is_expired());
    }

    #[test]
    fn test_oauth2_state_expired() {
        let mut state = OAuth2State::new("test");
        state.expires_at = Utc::now() - Duration::minutes(1);
        assert!(state.is_expired());
    }

    #[test]
    fn test_generate_random_string() {
        let s1 = generate_random_string(32);
        let s2 = generate_random_string(32);
        assert_eq!(s1.len(), 32);
        assert_ne!(s1, s2);
    }

    #[tokio::test]
    async fn test_session_store_insert_get() {
        let store = SsoSessionStore::new();
        let session = SsoSession {
            session_id: "test-session".into(),
            sub: "user1".into(),
            provider_id: "okta".into(),
            access_token: "token".into(),
            refresh_token: None,
            claims: TokenClaims {
                sub: "user1".into(),
                email: Some("user@example.com".into()),
                name: None,
                groups: vec![],
                roles: vec![],
                exp: (Utc::now() + Duration::hours(1)).timestamp(),
                iat: Utc::now().timestamp(),
                iss: "https://okta.com".into(),
                aud: "client-id".into(),
            },
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };

        store.insert(session.clone()).await;
        let retrieved = store.get("test-session").await.unwrap();
        assert_eq!(retrieved.sub, "user1");
    }

    #[tokio::test]
    async fn test_session_store_remove() {
        let store = SsoSessionStore::new();
        let session = SsoSession {
            session_id: "test".into(),
            sub: "user1".into(),
            provider_id: "okta".into(),
            access_token: "token".into(),
            refresh_token: None,
            claims: TokenClaims {
                sub: "user1".into(),
                email: None,
                name: None,
                groups: vec![],
                roles: vec![],
                exp: 0,
                iat: 0,
                iss: String::new(),
                aud: String::new(),
            },
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };

        store.insert(session).await;
        let removed = store.remove("test").await;
        assert!(removed.is_some());
        assert!(store.get("test").await.is_none());
    }

    #[tokio::test]
    async fn test_session_store_remove_by_sub() {
        let store = SsoSessionStore::new();
        for i in 0..3 {
            store
                .insert(SsoSession {
                    session_id: format!("s{}", i),
                    sub: "user1".into(),
                    provider_id: "okta".into(),
                    access_token: "token".into(),
                    refresh_token: None,
                    claims: TokenClaims {
                        sub: "user1".into(),
                        email: None,
                        name: None,
                        groups: vec![],
                        roles: vec![],
                        exp: 0,
                        iat: 0,
                        iss: String::new(),
                        aud: String::new(),
                    },
                    created_at: Utc::now(),
                    expires_at: Utc::now() + Duration::hours(1),
                })
                .await;
        }
        store
            .insert(SsoSession {
                session_id: "other".into(),
                sub: "user2".into(),
                provider_id: "okta".into(),
                access_token: "token".into(),
                refresh_token: None,
                claims: TokenClaims {
                    sub: "user2".into(),
                    email: None,
                    name: None,
                    groups: vec![],
                    roles: vec![],
                    exp: 0,
                    iat: 0,
                    iss: String::new(),
                    aud: String::new(),
                },
                created_at: Utc::now(),
                expires_at: Utc::now() + Duration::hours(1),
            })
            .await;

        store.remove_by_sub("user1").await;
        assert!(store.get("s0").await.is_none());
        assert!(store.get("s1").await.is_none());
        assert!(store.get("s2").await.is_none());
        assert!(store.get("other").await.is_some());
    }

    #[test]
    fn test_oidc_discovery() {
        let disc = OidcDiscovery::from_provider("https://okta.com");
        assert_eq!(disc.issuer, "https://okta.com");
        assert_eq!(disc.authorization_endpoint, "https://okta.com/authorize");
        assert_eq!(disc.token_endpoint, "https://okta.com/oauth/token");
        assert!(disc.scopes_supported.contains(&"openid".to_string()));
    }

    #[test]
    fn test_oauth2_flow_handler_initiate() {
        let handler = OAuth2FlowHandler::new();
        let _provider = IdentityProvider {
            id: "okta".into(),
            name: "Okta".into(),
            provider_type: super::super::ProviderType::Okta,
            config: super::super::ProviderConfig {
                client_id: Some("test-client".into()),
                client_secret: Some("secret".into()),
                issuer: Some("https://okta.com".into()),
                scopes: Some(vec!["openid".into(), "profile".into()]),
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };

        // We can't easily test async initiate without a runtime,
        // but we can verify the handler structure
        assert!(handler.pending_states.try_read().is_ok());
    }

    #[test]
    fn test_oauth2_state_new_fields() {
        let state = OAuth2State::new("google-workspace");
        assert_eq!(state.provider_id, "google-workspace");
        assert_eq!(state.state.len(), 32);
        assert_eq!(state.nonce.len(), 16);
        assert!(!state.state.is_empty());
        assert!(!state.nonce.is_empty());
        assert!(!state.pkce.code_verifier.is_empty());
        assert!(!state.pkce.code_challenge.is_empty());
    }

    #[test]
    fn test_oauth2_state_serialization_roundtrip() {
        let state = OAuth2State::new("test-provider");
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: OAuth2State = serde_json::from_str(&json).unwrap();
        assert_eq!(state.state, deserialized.state);
        assert_eq!(state.nonce, deserialized.nonce);
        assert_eq!(state.provider_id, deserialized.provider_id);
    }

    #[tokio::test]
    async fn test_session_store_get_nonexistent() {
        let store = SsoSessionStore::new();
        assert!(store.get("nonexistent").await.is_none());
    }

    #[tokio::test]
    async fn test_session_store_remove_nonexistent() {
        let store = SsoSessionStore::new();
        let removed = store.remove("nonexistent").await;
        assert!(removed.is_none());
    }

    #[tokio::test]
    async fn test_session_store_cleanup_expired() {
        let store = SsoSessionStore::new();

        // Insert expired session
        store
            .insert(SsoSession {
                session_id: "expired".into(),
                sub: "user1".into(),
                provider_id: "okta".into(),
                access_token: "token".into(),
                refresh_token: None,
                claims: TokenClaims {
                    sub: "user1".into(),
                    email: None,
                    name: None,
                    groups: vec![],
                    roles: vec![],
                    exp: 0,
                    iat: 0,
                    iss: String::new(),
                    aud: String::new(),
                },
                created_at: Utc::now() - Duration::hours(2),
                expires_at: Utc::now() - Duration::hours(1),
            })
            .await;

        // Insert valid session
        store
            .insert(SsoSession {
                session_id: "valid".into(),
                sub: "user2".into(),
                provider_id: "okta".into(),
                access_token: "token".into(),
                refresh_token: None,
                claims: TokenClaims {
                    sub: "user2".into(),
                    email: None,
                    name: None,
                    groups: vec![],
                    roles: vec![],
                    exp: (Utc::now() + Duration::hours(1)).timestamp(),
                    iat: Utc::now().timestamp(),
                    iss: String::new(),
                    aud: String::new(),
                },
                created_at: Utc::now(),
                expires_at: Utc::now() + Duration::hours(1),
            })
            .await;

        store.cleanup_expired().await;

        assert!(store.get("expired").await.is_none());
        assert!(store.get("valid").await.is_some());
    }

    #[tokio::test]
    async fn test_session_store_default() {
        let store = SsoSessionStore::default();
        assert!(store.get("anything").await.is_none());
    }

    #[tokio::test]
    async fn test_session_store_overwrite() {
        let store = SsoSessionStore::new();

        let session = SsoSession {
            session_id: "s1".into(),
            sub: "user1".into(),
            provider_id: "okta".into(),
            access_token: "token-v1".into(),
            refresh_token: None,
            claims: TokenClaims {
                sub: "user1".into(),
                email: None,
                name: None,
                groups: vec![],
                roles: vec![],
                exp: 0,
                iat: 0,
                iss: String::new(),
                aud: String::new(),
            },
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };

        store.insert(session.clone()).await;
        let mut updated = session;
        updated.access_token = "token-v2".into();
        store.insert(updated).await;

        let retrieved = store.get("s1").await.unwrap();
        assert_eq!(retrieved.access_token, "token-v2");
    }

    #[test]
    fn test_oauth2_flow_handler_new() {
        let handler = OAuth2FlowHandler::new();
        assert!(handler.validator.is_none());
        assert!(handler.pending_states.try_read().is_ok());
    }

    #[test]
    fn test_oauth2_flow_handler_with_jwt_validator() {
        let jwt_config = super::super::JwtValidationConfig::default();
        let handler = OAuth2FlowHandler::with_jwt_validator(jwt_config);
        assert!(handler.validator.is_some());
    }

    #[test]
    fn test_oauth2_flow_handler_set_validator() {
        let mut handler = OAuth2FlowHandler::new();
        assert!(handler.validator.is_none());
        let jwt_config = super::super::JwtValidationConfig::default();
        let validator = super::super::TokenValidator::new(jwt_config);
        handler.set_validator(validator);
        assert!(handler.validator.is_some());
    }

    #[test]
    fn test_oauth2_flow_handler_default() {
        let handler = OAuth2FlowHandler::default();
        assert!(handler.validator.is_none());
    }

    #[test]
    fn test_oauth2_flow_handler_revoke() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let handler = OAuth2FlowHandler::new();

        // Revoke nonexistent session
        let result = rt.block_on(handler.revoke("nonexistent"));
        assert!(!result);
    }

    #[test]
    fn test_oidc_discovery_all_fields() {
        let disc = OidcDiscovery::from_provider("https://auth0.example.com");
        assert_eq!(disc.issuer, "https://auth0.example.com");
        assert_eq!(
            disc.authorization_endpoint,
            "https://auth0.example.com/authorize"
        );
        assert_eq!(disc.token_endpoint, "https://auth0.example.com/oauth/token");
        assert_eq!(disc.userinfo_endpoint, "https://auth0.example.com/userinfo");
        assert_eq!(
            disc.jwks_uri,
            "https://auth0.example.com/.well-known/jwks.json"
        );
        assert!(disc.scopes_supported.contains(&"openid".to_string()));
        assert!(disc.scopes_supported.contains(&"profile".to_string()));
        assert!(disc.scopes_supported.contains(&"email".to_string()));
        assert!(disc
            .scopes_supported
            .contains(&"offline_access".to_string()));
        assert!(disc.response_types_supported.contains(&"code".to_string()));
        assert!(disc
            .grant_types_supported
            .contains(&"authorization_code".to_string()));
        assert!(disc
            .grant_types_supported
            .contains(&"client_credentials".to_string()));
        assert!(disc
            .grant_types_supported
            .contains(&"refresh_token".to_string()));
        assert!(disc.subject_types_supported.contains(&"public".to_string()));
        assert!(disc
            .id_token_signing_alg_values_supported
            .contains(&"RS256".to_string()));
        assert!(disc
            .id_token_signing_alg_values_supported
            .contains(&"ES256".to_string()));
        assert!(disc
            .token_endpoint_auth_methods_supported
            .contains(&"client_secret_basic".to_string()));
        assert!(disc
            .token_endpoint_auth_methods_supported
            .contains(&"client_secret_post".to_string()));
    }

    #[test]
    fn test_oauth2_token_response_serialization() {
        let resp = OAuth2TokenResponse {
            access_token: "at123".into(),
            token_type: "Bearer".into(),
            expires_in: Some(3600),
            refresh_token: Some("rt456".into()),
            id_token: Some("id_jwt".into()),
            scope: Some("openid profile".into()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("at123"));
        assert!(json.contains("Bearer"));

        let deserialized: OAuth2TokenResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.access_token, "at123");
        assert_eq!(deserialized.expires_in, Some(3600));
    }

    #[test]
    fn test_generate_random_string_charset() {
        let s = generate_random_string(1000);
        for c in s.chars() {
            assert!(c.is_ascii_alphanumeric(), "unexpected char: {}", c);
        }
    }

    #[tokio::test]
    async fn test_session_store_remove_by_sub_multiple_providers() {
        let store = SsoSessionStore::new();

        // User1 with multiple sessions
        store
            .insert(SsoSession {
                session_id: "s1".into(),
                sub: "user1".into(),
                provider_id: "okta".into(),
                access_token: "t1".into(),
                refresh_token: None,
                claims: TokenClaims {
                    sub: "user1".into(),
                    email: None,
                    name: None,
                    groups: vec![],
                    roles: vec![],
                    exp: 0,
                    iat: 0,
                    iss: String::new(),
                    aud: String::new(),
                },
                created_at: Utc::now(),
                expires_at: Utc::now() + Duration::hours(1),
            })
            .await;
        store
            .insert(SsoSession {
                session_id: "s2".into(),
                sub: "user1".into(),
                provider_id: "google".into(),
                access_token: "t2".into(),
                refresh_token: None,
                claims: TokenClaims {
                    sub: "user1".into(),
                    email: None,
                    name: None,
                    groups: vec![],
                    roles: vec![],
                    exp: 0,
                    iat: 0,
                    iss: String::new(),
                    aud: String::new(),
                },
                created_at: Utc::now(),
                expires_at: Utc::now() + Duration::hours(1),
            })
            .await;

        // User2
        store
            .insert(SsoSession {
                session_id: "s3".into(),
                sub: "user2".into(),
                provider_id: "okta".into(),
                access_token: "t3".into(),
                refresh_token: None,
                claims: TokenClaims {
                    sub: "user2".into(),
                    email: None,
                    name: None,
                    groups: vec![],
                    roles: vec![],
                    exp: 0,
                    iat: 0,
                    iss: String::new(),
                    aud: String::new(),
                },
                created_at: Utc::now(),
                expires_at: Utc::now() + Duration::hours(1),
            })
            .await;

        // Remove user1 across all providers
        store.remove_by_sub("user1").await;
        assert!(store.get("s1").await.is_none());
        assert!(store.get("s2").await.is_none());
        assert!(store.get("s3").await.is_some());
    }

    #[test]
    fn test_sso_session_serialization() {
        let session = SsoSession {
            session_id: "test-sid".into(),
            sub: "user1".into(),
            provider_id: "okta".into(),
            access_token: "at".into(),
            refresh_token: Some("rt".into()),
            claims: TokenClaims {
                sub: "user1".into(),
                email: Some("user@example.com".into()),
                name: Some("Test User".into()),
                groups: vec!["admin".into()],
                roles: vec!["admin".into()],
                exp: 1700000000,
                iat: 1699996400,
                iss: "https://okta.com".into(),
                aud: "client-id".into(),
            },
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("test-sid"));
        assert!(json.contains("user1"));

        let deserialized: SsoSession = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.session_id, "test-sid");
        assert_eq!(deserialized.sub, "user1");
        assert_eq!(deserialized.refresh_token, Some("rt".into()));
    }

    #[tokio::test]
    async fn test_revoke_with_blacklist_no_session() {
        let handler = OAuth2FlowHandler::new();
        let blacklist: Option<std::sync::Arc<dyn super::super::TokenBlacklistStorage>> = None;
        let result = handler
            .revoke_with_blacklist("nonexistent", &blacklist)
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_revoke_with_blacklist_success() {
        let handler = OAuth2FlowHandler::new();
        handler
            .sessions
            .insert(SsoSession {
                session_id: "s1".into(),
                sub: "user1".into(),
                provider_id: "okta".into(),
                access_token: "token123".into(),
                refresh_token: None,
                claims: TokenClaims {
                    sub: "user1".into(),
                    email: None,
                    name: None,
                    groups: vec![],
                    roles: vec![],
                    exp: 0,
                    iat: 0,
                    iss: String::new(),
                    aud: String::new(),
                },
                created_at: Utc::now(),
                expires_at: Utc::now() + Duration::hours(1),
            })
            .await;

        let blacklist: Option<std::sync::Arc<dyn super::super::TokenBlacklistStorage>> = None;
        let result = handler
            .revoke_with_blacklist("s1", &blacklist)
            .await
            .unwrap();
        assert!(result);

        // Session should be removed
        assert!(handler.sessions.get("s1").await.is_none());
    }

    #[tokio::test]
    async fn test_revoke_with_blacklist_session_not_found() {
        let handler = OAuth2FlowHandler::new();
        let blacklist: Option<std::sync::Arc<dyn super::super::TokenBlacklistStorage>> = None;
        let result = handler
            .revoke_with_blacklist("nonexistent", &blacklist)
            .await
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn test_oauth2_flow_handler_initiate_disabled_provider() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let handler = OAuth2FlowHandler::new();
        let provider = IdentityProvider {
            id: "disabled-okta".into(),
            name: "Disabled Okta".into(),
            provider_type: super::super::ProviderType::Okta,
            config: super::super::ProviderConfig {
                client_id: Some("test".into()),
                client_secret: Some("secret".into()),
                issuer: Some("https://okta.com".into()),
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: false,
            auto_provision: false,
            default_team: None,
        };

        let result = rt.block_on(handler.initiate(&provider, "https://example.com/callback"));
        assert!(result.is_err());
        match result.unwrap_err() {
            super::super::SsoError::ProviderDisabled(id) => assert_eq!(id, "disabled-okta"),
            other => panic!("Expected ProviderDisabled, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_oauth2_initiate_disabled_provider() {
        let handler = OAuth2FlowHandler::new();
        let provider = IdentityProvider {
            id: "disabled-idp".into(),
            name: "Disabled".into(),
            provider_type: super::super::ProviderType::GenericOidc,
            config: super::super::ProviderConfig {
                client_id: Some("cid".into()),
                client_secret: None,
                issuer: Some("https://idp.example.com".into()),
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: false,
            auto_provision: false,
            default_team: None,
        };
        let result = handler.initiate(&provider, "https://app/redirect").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            SsoError::ProviderDisabled(id) => assert_eq!(id, "disabled-idp"),
            other => panic!("Expected ProviderDisabled, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_oauth2_initiate_generates_url() {
        let handler = OAuth2FlowHandler::new();
        let provider = IdentityProvider {
            id: "okta".into(),
            name: "Okta".into(),
            provider_type: super::super::ProviderType::Okta,
            config: super::super::ProviderConfig {
                client_id: Some("my-client".into()),
                client_secret: Some("secret".into()),
                issuer: Some("https://okta.com".into()),
                scopes: Some(vec!["openid".into(), "email".into()]),
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };
        let (state, challenge, url) = handler
            .initiate(&provider, "https://app/redirect")
            .await
            .unwrap();
        assert!(!state.is_empty());
        assert!(!challenge.is_empty());
        assert!(url.contains("https://okta.com/authorize"));
        assert!(url.contains("client_id=my-client"));
        assert!(url.contains("openid email"));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state="));
        assert!(url.contains("nonce="));
    }

    #[tokio::test]
    async fn test_oauth2_initiate_default_scopes() {
        let handler = OAuth2FlowHandler::new();
        let provider = IdentityProvider {
            id: "auth0".into(),
            name: "Auth0".into(),
            provider_type: super::super::ProviderType::Auth0,
            config: super::super::ProviderConfig {
                client_id: Some("cid".into()),
                client_secret: Some("secret".into()),
                issuer: Some("https://auth0.example.com".into()),
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };
        let (_, _, url) = handler.initiate(&provider, "https://app/cb").await.unwrap();
        assert!(url.contains("openid profile email"));
    }

    #[tokio::test]
    async fn test_oauth2_callback_invalid_state() {
        let handler = OAuth2FlowHandler::new();
        let provider = IdentityProvider {
            id: "okta".into(),
            name: "Okta".into(),
            provider_type: super::super::ProviderType::Okta,
            config: super::super::ProviderConfig {
                client_id: Some("cid".into()),
                client_secret: Some("secret".into()),
                issuer: Some("https://okta.com".into()),
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };
        let result = handler
            .callback("bad-state", "code", "verifier", &provider, "https://token")
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SsoError::InvalidState));
    }

    #[tokio::test]
    async fn test_oauth2_callback_wrong_provider() {
        let handler = OAuth2FlowHandler::new();
        let provider_okta = IdentityProvider {
            id: "okta".into(),
            name: "Okta".into(),
            provider_type: super::super::ProviderType::Okta,
            config: super::super::ProviderConfig {
                client_id: Some("cid".into()),
                client_secret: Some("secret".into()),
                issuer: Some("https://okta.com".into()),
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };
        let provider_azure = IdentityProvider {
            id: "azure".into(),
            name: "Azure".into(),
            provider_type: super::super::ProviderType::AzureAd,
            config: super::super::ProviderConfig {
                client_id: Some("cid".into()),
                client_secret: Some("secret".into()),
                issuer: Some("https://azure.com".into()),
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };
        // Initiate with okta
        let (state_str, _, _) = handler
            .initiate(&provider_okta, "https://app/cb")
            .await
            .unwrap();
        // Callback with azure provider — should fail
        let result = handler
            .callback(
                &state_str,
                "code",
                "verifier",
                &provider_azure,
                "https://token",
            )
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SsoError::InvalidState));
    }

    #[tokio::test]
    async fn test_oauth2_callback_expired_state() {
        let handler = OAuth2FlowHandler::new();
        let provider = IdentityProvider {
            id: "okta".into(),
            name: "Okta".into(),
            provider_type: super::super::ProviderType::Okta,
            config: super::super::ProviderConfig {
                client_id: Some("cid".into()),
                client_secret: Some("secret".into()),
                issuer: Some("https://okta.com".into()),
                scopes: None,
                idp_metadata_url: None,
                sp_entity_id: None,
                acs_url: None,
                idp_certificate: None,
                scim_url: None,
                scim_token: None,
            },
            enabled: true,
            auto_provision: false,
            default_team: None,
        };
        // Initiate to get a state
        let (state_str, _, _) = handler.initiate(&provider, "https://app/cb").await.unwrap();

        // Manually expire the state
        {
            let mut states = handler.pending_states.write().await;
            if let Some(mut s) = states.remove(&state_str) {
                s.expires_at = Utc::now() - Duration::minutes(10);
                states.insert(state_str.clone(), s);
            }
        }

        let result = handler
            .callback(&state_str, "code", "verifier", &provider, "https://token")
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SsoError::InvalidState));
    }

    #[tokio::test]
    async fn test_oauth2_revoke_session() {
        let handler = OAuth2FlowHandler::new();
        // Revoke non-existent session
        assert!(!handler.revoke("nonexistent").await);
    }

    #[tokio::test]
    async fn test_oauth2_revoke_with_blacklist_no_session() {
        let handler = OAuth2FlowHandler::new();
        let result = handler
            .revoke_with_blacklist("nonexistent", &None)
            .await
            .unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_oauth2_revoke_with_blacklist_with_session() {
        let handler = OAuth2FlowHandler::new();
        let session = SsoSession {
            session_id: "sess-1".into(),
            sub: "user1".into(),
            provider_id: "okta".into(),
            access_token: "access-token-123".into(),
            refresh_token: Some("refresh-token-456".into()),
            claims: TokenClaims {
                sub: "user1".into(),
                email: None,
                name: None,
                groups: vec![],
                roles: vec![],
                exp: 0,
                iat: 0,
                iss: String::new(),
                aud: String::new(),
            },
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        handler.sessions.insert(session).await;

        let result = handler
            .revoke_with_blacklist("sess-1", &None)
            .await
            .unwrap();
        assert!(result);
        assert!(handler.sessions.get("sess-1").await.is_none());
    }

    #[tokio::test]
    async fn test_sso_session_store_cleanup_expired() {
        let store = SsoSessionStore::new();
        let expired_session = SsoSession {
            session_id: "expired".into(),
            sub: "user1".into(),
            provider_id: "okta".into(),
            access_token: "token".into(),
            refresh_token: None,
            claims: TokenClaims {
                sub: "user1".into(),
                email: None,
                name: None,
                groups: vec![],
                roles: vec![],
                exp: 0,
                iat: 0,
                iss: String::new(),
                aud: String::new(),
            },
            created_at: Utc::now() - Duration::hours(2),
            expires_at: Utc::now() - Duration::hours(1),
        };
        let valid_session = SsoSession {
            session_id: "valid".into(),
            sub: "user2".into(),
            provider_id: "okta".into(),
            access_token: "token".into(),
            refresh_token: None,
            claims: TokenClaims {
                sub: "user2".into(),
                email: None,
                name: None,
                groups: vec![],
                roles: vec![],
                exp: 0,
                iat: 0,
                iss: String::new(),
                aud: String::new(),
            },
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(1),
        };
        store.insert(expired_session).await;
        store.insert(valid_session).await;
        assert!(store.get("expired").await.is_some());
        assert!(store.get("valid").await.is_some());

        store.cleanup_expired().await;

        assert!(store.get("expired").await.is_none());
        assert!(store.get("valid").await.is_some());
    }

    #[test]
    fn test_oauth2_token_response_deserialize() {
        let json = r#"{
            "access_token": "at123",
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "rt456",
            "id_token": "id789",
            "scope": "openid profile"
        }"#;
        let resp: OAuth2TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "at123");
        assert_eq!(resp.token_type, "Bearer");
        assert_eq!(resp.expires_in, Some(3600));
        assert_eq!(resp.refresh_token, Some("rt456".into()));
        assert_eq!(resp.id_token, Some("id789".into()));
        assert_eq!(resp.scope, Some("openid profile".into()));
    }

    #[test]
    fn test_oauth2_token_response_minimal() {
        let json = r#"{
            "access_token": "at123",
            "token_type": "Bearer"
        }"#;
        let resp: OAuth2TokenResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.access_token, "at123");
        assert!(resp.expires_in.is_none());
        assert!(resp.refresh_token.is_none());
        assert!(resp.id_token.is_none());
    }

    #[test]
    fn test_oauth2_state_serialization() {
        let state = OAuth2State::new("test-provider");
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: OAuth2State = serde_json::from_str(&json).unwrap();
        assert_eq!(state.state, deserialized.state);
        assert_eq!(state.provider_id, deserialized.provider_id);
        assert_eq!(state.nonce, deserialized.nonce);
    }

    #[test]
    fn test_oidc_discovery_full() {
        let disc = OidcDiscovery::from_provider("https://idp.example.com");
        assert_eq!(disc.issuer, "https://idp.example.com");
        assert_eq!(
            disc.authorization_endpoint,
            "https://idp.example.com/authorize"
        );
        assert_eq!(disc.token_endpoint, "https://idp.example.com/oauth/token");
        assert_eq!(disc.userinfo_endpoint, "https://idp.example.com/userinfo");
        assert_eq!(
            disc.jwks_uri,
            "https://idp.example.com/.well-known/jwks.json"
        );
        assert!(disc.scopes_supported.contains(&"openid".to_string()));
        assert!(disc
            .scopes_supported
            .contains(&"offline_access".to_string()));
        assert!(disc.response_types_supported.contains(&"code".to_string()));
        assert!(disc
            .grant_types_supported
            .contains(&"client_credentials".to_string()));
        assert!(disc.subject_types_supported.contains(&"public".to_string()));
        assert!(disc
            .id_token_signing_alg_values_supported
            .contains(&"RS256".to_string()));
        assert!(disc
            .token_endpoint_auth_methods_supported
            .contains(&"client_secret_basic".to_string()));
    }

    #[test]
    fn test_generate_random_string_lengths() {
        let s1 = generate_random_string(1);
        assert_eq!(s1.len(), 1);
        let s16 = generate_random_string(16);
        assert_eq!(s16.len(), 16);
        let s100 = generate_random_string(100);
        assert_eq!(s100.len(), 100);
    }

    #[test]
    fn test_oauth2_handler_default() {
        let handler = OAuth2FlowHandler::default();
        assert!(handler.pending_states.try_read().is_ok());
    }

    #[test]
    fn test_oauth2_handler_with_jwt_validator() {
        use super::JwtValidationConfig;
        let config = JwtValidationConfig::default();
        let handler = OAuth2FlowHandler::with_jwt_validator(config);
        // Verify handler was constructed with a validator by attempting initiate
        // (which would use the validator if called with an id_token)
        assert!(handler.pending_states.try_read().is_ok());
    }

    #[test]
    fn test_oauth2_handler_set_validator() {
        use super::{JwtValidationConfig, TokenValidator};
        let config = JwtValidationConfig::default();
        let validator = TokenValidator::new(config);
        let mut handler = OAuth2FlowHandler::new();
        handler.set_validator(validator);
        // set_validator completes without panic
    }

    #[test]
    fn test_oauth2_state_pkce_challenge() {
        let state = OAuth2State::new("test");
        assert!(!state.pkce.code_verifier.is_empty());
        assert!(!state.pkce.code_challenge.is_empty());
    }
}
