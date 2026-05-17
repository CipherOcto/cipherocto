# RFC-0949 (Economics): Enterprise SSO

## Status

Draft

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

Define enterprise Single Sign-On (SSO) integration for quota-router that supports OAuth2 and SAML authentication, enabling organizations to use their existing identity providers (Okta, Azure AD, Google Workspace) for API key management and user authentication.

## Dependencies

**Requires:**

- RFC-0903 (Economics): Virtual API Key System
- RFC-0932 (Economics): Team Management

**Optional:**

- RFC-0905 (Economics): Observability and Logging (auth event logging)
- RFC-0933 (Economics): Rate Limiting Integration (auth endpoint rate limits)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | OAuth2 support | Authorization Code, Client Credentials, PKCE |
| G2 | SAML 2.0 support | SP-initiated SSO |
| G3 | OIDC support | OpenID Connect 1.0 |
| G4 | Zero-trust | JWT validation, token introspection |

## Motivation

### Problem

Enterprise organizations require SSO for:

1. **Centralized Identity** — Users authenticate with corporate credentials
2. **Access Control** — Role-based access tied to identity provider groups
3. **Audit Compliance** — Authentication events logged centrally
4. **User Lifecycle** — Automatic provisioning/deprovisioning via SCIM
5. **Multi-factor Authentication** — Enforced at identity provider level

### Use Cases

- **Corporate deployment**: Employees use Okta/Azure AD to access quota-router admin
- **Partner access**: External partners authenticate via OAuth2
- **API access**: Service accounts use client credentials flow
- **Compliance**: SOC2/HIPAA requires SSO for admin access

## Specification

### Authentication Flows

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
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
```

### Identity Provider Integration

```rust
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
    pub enabled: bool,
    /// Auto-provision users
    pub auto_provision: bool,
    /// Default team for new users
    pub default_team: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub client_secret: Option<String>,
    pub issuer: Option<String>,
    pub scopes: Option<Vec<String>>,
    /// SAML settings
    pub idp_metadata_url: Option<String>,
    pub sp_entity_id: Option<String>,
    pub acs_url: Option<String>,
    /// SCIM settings
    pub scim_url: Option<String>,
    pub scim_token: Option<String>,
}
```

**ProviderConfig Validation Rules:**

| ProviderType | Required Fields | Optional Fields |
|--------------|----------------|-----------------|
| Okta | client_id, client_secret, issuer | scopes |
| AzureAd | client_id, client_secret, issuer | scopes |
| GoogleWorkspace | client_id, client_secret | scopes |
| Auth0 | client_id, client_secret, issuer | scopes |
| GenericOidc | client_id, issuer | client_secret, scopes |
| GenericSaml | idp_metadata_url, sp_entity_id, acs_url | — |

Invalid combinations (e.g., `client_id` without `client_secret` for Okta) MUST be rejected at config load time with a descriptive error.

### SSO-to-API-Key Mapping

When an SSO user authenticates, the system MUST map them to a virtual API key for API access:

```rust
/// Extension trait for KeyStorage to support SSO lookups
#[async_trait]
pub trait SsoKeyStorageExt {
    /// Find virtual key by SSO subject identifier
    async fn get_key_by_sso_subject(&self, subject: &str) -> Result<Option<VirtualKey>>;
}

/// Schema extension for VirtualKey metadata (RFC-0903)
/// These fields are added to KeyMetadata for SSO-linked keys:
pub struct SsoKeyMetadata {
    /// IdP subject identifier (stable across sessions)
    pub sso_subject: Option<String>,
    /// SSO provider ID (references IdentityProvider.id)
    pub sso_provider: Option<String>,
}

/// SSO-to-API-key mapping
pub struct SsoKeyMapper {
    /// Key storage backend with SSO extension
    key_storage: Arc<dyn SsoKeyStorageExt>,
    /// Role mapping config (IdP group → quota-router role)
    role_mapping: HashMap<String, String>,
}

impl SsoKeyMapper {
    /// Get or create virtual key for SSO user
    pub async fn get_or_create_key(
        &self,
        user: &SsoUser,
        provider: &IdentityProvider,
    ) -> Result<VirtualKey> {
        // 1. Look up existing key by user.sub (IdP subject)
        if let Some(key) = self.key_storage.get_key_by_sso_subject(&user.sub).await? {
            return Ok(key);
        }

        // 2. Auto-provision if enabled
        if provider.auto_provision {
            let key = VirtualKey {
                key_id: generate_key_id(),
                name: format!("sso-{}", user.email.as_deref().unwrap_or(&user.sub)),
                team_id: provider.default_team.clone(),
                role: self.map_role(user),
                metadata: KeyMetadata {
                    sso_subject: Some(user.sub.clone()),
                    sso_provider: Some(provider.id.clone()),
                    ..Default::default()
                },
                ..Default::default()
            };
            self.key_storage.create_key(&key).await?;
            return Ok(key);
        }

        // 3. No auto-provision — require admin to create key
        Err(SsoError::NoKeyMapping { user_id: user.sub.clone() })
    }

    /// Map IdP groups to quota-router role using role_mapping config
    fn map_role(&self, user: &SsoUser) -> String {
        // Iterate user.groups, find first match in role_mapping
        // Default to "user" if no match
        for group in &user.groups {
            if let Some(role) = self.role_mapping.get(group) {
                return role.clone();
            }
        }
        "user".to_string()
    }
}
```

The mapping is keyed on `user.sub` (the IdP's subject identifier), which is stable across sessions. When auto-provision is disabled, an admin MUST explicitly create a virtual key and link it to the SSO user.

### Token Management

```rust
/// JWT token validation
pub struct TokenValidator {
    /// JWKS endpoint for key rotation
    jwks_url: String,
    /// Expected issuer
    issuer: String,
    /// Expected audience
    audience: String,
    /// Clock skew tolerance
    clock_skew: Duration,
    /// Supported signing algorithms
    supported_algorithms: Vec<JwtAlgorithm>,
}

/// Supported JWT signing algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JwtAlgorithm {
    RS256,  // RSA with SHA-256
    RS384,  // RSA with SHA-384
    RS512,  // RSA with SHA-512
    ES256,  // ECDSA with P-256 and SHA-256
    ES384,  // ECDSA with P-384 and SHA-384
    PS256,  // RSASSA-PSS with SHA-256
}

impl TokenValidator {
    /// Validate JWT token
    pub async fn validate(&self, token: &str) -> Result<TokenClaims> {
        // 1. Decode JWT header
        // 2. Validate algorithm is in supported_algorithms (reject "none")
        // 3. Fetch JWKS keys (cached, refresh on unknown kid)
        // 4. Verify signature using matching key
        // 5. Check exp claim (with clock_skew tolerance)
        // 6. Check iss claim matches self.issuer
        // 7. Check aud claim contains self.audience
        // 8. Return claims
    }

    /// Introspect opaque token
    pub async fn introspect(&self, token: &str) -> Result<TokenInfo> {
        // Call token introspection endpoint
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    pub sub: String,          // User ID
    pub email: Option<String>,
    pub name: Option<String>,
    pub groups: Vec<String>,  // IdP groups
    pub roles: Vec<String>,   // Mapped roles
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
    pub aud: String,          // Audience
}
```

**JWT Header Format:**

```json
{
  "alg": "RS256",
  "typ": "JWT",
  "kid": "key-id-from-jwks"
}
```

The `alg` field MUST be one of the supported algorithms. Tokens with `alg: "none"` MUST be rejected.

### Token Lifecycle

| Token Type | Lifetime | Refresh Window | Revocation |
|------------|----------|----------------|------------|
| Access Token | 1 hour (configurable) | N/A (use refresh token) | Immediate via blacklist |
| Refresh Token | 7 days (configurable) | Last 24 hours | Immediate via blacklist |
| Session Token | 30 minutes (configurable) | Sliding window | Immediate via blacklist |

**Refresh Token Rotation:**

When a refresh token is used, the old token is invalidated and a new refresh token is issued. This prevents token theft replay attacks.

**Token Revocation Propagation:**

```rust
/// Token blacklist storage trait (backed by stoolap)
#[async_trait]
pub trait TokenBlacklistStorage {
    /// Add token to blacklist with expiration
    async fn add(&self, token_id: &str, expires_at: DateTime<Utc>) -> Result<()>;
    /// Check if token is blacklisted
    async fn contains(&self, token_id: &str) -> Result<bool>;
    /// Remove expired entries (background cleanup)
    async fn cleanup_expired(&self) -> Result<u64>;
}

/// Token blacklist for cross-instance revocation
pub struct TokenBlacklist {
    /// Shared storage (stoolap)
    storage: Arc<dyn TokenBlacklistStorage>,
}

impl TokenBlacklist {
    /// Revoke a token
    pub async fn revoke(&self, token_id: &str, expires_at: DateTime<Utc>) -> Result<()> {
        self.storage.add(token_id, expires_at).await
    }

    /// Check if token is revoked
    pub async fn is_revoked(&self, token_id: &str) -> Result<bool> {
        self.storage.contains(token_id).await
    }
}
```

The blacklist uses stoolap for cross-instance propagation. Entries auto-expire when the token would have expired.

### SAML 2.0 Specification

```rust
/// SAML assertion parser
pub struct SamlAssertionParser {
    /// IdP certificate for signature validation
    idp_certificate: Vec<u8>,
    /// SP entity ID for audience validation
    sp_entity_id: String,
    /// ACS URL for recipient validation
    acs_url: String,
}

impl SamlAssertionParser {
    /// Parse and validate SAML assertion
    pub fn parse(&self, assertion_xml: &str) -> Result<SamlAssertion> {
        // 1. Parse XML
        // 2. Validate XML signature using idp_certificate
        // 3. Check Conditions/NotBefore and NotOnOrAfter (with clock skew)
        // 4. Validate Audience matches sp_entity_id
        // 5. Validate SubjectConfirmationData.Recipient matches acs_url
        // 6. Extract attributes
        // 7. Return SamlAssertion
    }

    /// Map SAML attributes to user properties
    pub fn map_attributes(&self, assertion: &SamlAssertion) -> SsoUser {
        SsoUser {
            sub: assertion.name_id.clone(),
            email: assertion.attributes.get("email")
                .and_then(|v| v.first().cloned()),
            name: assertion.attributes.get("displayName")
                .and_then(|v| v.first().cloned()),
            groups: assertion.attributes.get("groups")
                .cloned()
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SamlAssertion {
    pub name_id: String,
    pub session_index: Option<String>,
    /// Multi-valued SAML attributes (e.g., groups may have multiple values)
    pub attributes: HashMap<String, Vec<String>>,
    pub not_before: DateTime<Utc>,
    pub not_on_or_after: DateTime<Utc>,
}
```

**SP Metadata Generation:**

The system MUST generate SP metadata XML at `GET /auth/sso/saml/metadata`:

```xml
<EntityDescriptor xmlns="urn:oasis:names:tc:SAML:2.0:metadata"
                  entityID="{sp_entity_id}">
  <SPSSODescriptor
      AuthnRequestsSigned="true"
      WantAssertionsSigned="true"
      protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
    <SingleLogoutService
        Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect"
        Location="{base_url}/auth/sso/saml/slo"/>
    <AssertionConsumerService
        Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST"
        Location="{acs_url}"
        index="0" isDefault="true"/>
  </SPSSODescriptor>
</EntityDescriptor>
```

### SCIM 2.0 Specification

```rust
/// SCIM 2.0 user provisioning
pub struct ScimProvisioner {
    /// SCIM endpoint URL
    url: String,
    /// Bearer token for SCIM API
    token: String,
    /// HTTP client
    client: reqwest::Client,
}

impl ScimProvisioner {
    /// List users with filter
    pub async fn list_users(
        &self,
        filter: Option<&str>,
        start_index: Option<u32>,
        count: Option<u32>,
    ) -> Result<ScimListResponse> {
        // GET /Users?filter={filter}&startIndex={start_index}&count={count}
        // Supports SCIM filter syntax: userName eq "user@example.com"
        // Supports pagination via startIndex and count
    }

    /// Get user by ID
    pub async fn get_user(&self, user_id: &str) -> Result<ScimUser> {
        // GET /Users/{user_id}
    }

    /// Create user
    pub async fn create_user(&self, user: &ScimUser) -> Result<ScimUser> {
        // POST /Users
    }

    /// Update user (full replace)
    pub async fn update_user(&self, user_id: &str, user: &ScimUser) -> Result<ScimUser> {
        // PUT /Users/{user_id}
    }

    /// Patch user (partial update)
    pub async fn patch_user(
        &self,
        user_id: &str,
        operations: &[ScimPatchOp],
    ) -> Result<ScimUser> {
        // PATCH /Users/{user_id}
        // Supports: add, remove, replace operations
    }

    /// Deactivate user (soft delete)
    pub async fn deactivate_user(&self, user_id: &str) -> Result<()> {
        // PATCH /Users/{user_id} with { "op": "replace", "path": "active", "value": false }
        // Deactivation preserves user data; deletion removes it
    }

    /// Sync users from IdP
    pub async fn sync_users(&self) -> Result<Vec<ScimUser>> {
        // Paginate through all users using startIndex/count
        // Map to quota-router users
        // Create/update/deactivate as needed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimUser {
    pub id: String,
    pub user_name: String,
    pub emails: Vec<ScimEmail>,
    pub active: bool,
    pub groups: Vec<ScimGroup>,
    pub display_name: Option<String>,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimEmail {
    pub value: String,
    pub primary: bool,
    #[serde(rename = "type")]
    pub email_type: Option<String>,  // "work", "home", "other"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimGroup {
    pub value: String,
    pub display: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimPatchOp {
    pub op: String,  // "add", "remove", "replace"
    pub path: Option<String>,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimListResponse {
    pub schemas: Vec<String>,
    pub total_results: u32,
    pub start_index: u32,
    pub items_per_page: u32,
    pub resources: Vec<ScimUser>,
}

/// SCIM filter syntax support
/// - eq: userName eq "user@example.com"
/// - ne: active ne false
/// - co: displayName co "John"
/// - sw: userName sw "admin"
/// - gt/lt/ge/le: comparison operators
/// - and/or: logical operators
/// - not: negation
/// - group filter: groups eq "group-id"
```

**Deactivation vs Deletion:**

- **Deactivation** (`active: false`): User is disabled but data is preserved. Default for SCIM deprovisioning.
- **Deletion** (`DELETE /Users/{id}`): User and data are permanently removed. Only used for GDPR right-to-erasure requests.

### Configuration

```yaml
# In config.yaml
sso:
  enabled: true

  providers:
    - id: okta-production
      name: "Okta SSO"
      type: okta
      config:
        client_id: "${OKTA_CLIENT_ID}"
        client_secret: "${OKTA_CLIENT_SECRET}"
        issuer: "https://example.okta.com"
        scopes: [openid, profile, email]
      auto_provision: true
      default_team: external

    - id: azure-internal
      name: "Azure AD"
      type: azure_ad
      config:
        client_id: "${AZURE_CLIENT_ID}"
        client_secret: "${AZURE_CLIENT_SECRET}"
        issuer: "https://login.microsoftonline.com/${AZURE_TENANT_ID}/v2.0"
        scopes: [openid, profile, email]
      auto_provision: true
      default_team: internal

  # Role mapping from IdP groups to quota-router roles
  role_mapping:
    "quota-router-admins": admin
    "quota-router-users": user
    "quota-router-viewers": viewer

  # Team mapping from IdP groups to quota-router teams
  team_mapping:
    "engineering": engineering-team
    "data-science": ds-team

  # Token lifecycle
  token:
    access_token_lifetime: 3600      # 1 hour (seconds)
    refresh_token_lifetime: 604800   # 7 days (seconds)
    session_lifetime: 1800           # 30 minutes (seconds)
    refresh_window: 86400            # Last 24 hours (seconds)

  # JWT validation
  jwt:
    supported_algorithms: [RS256, ES256]
    clock_skew: 30                   # seconds
    jwks_cache_ttl: 3600             # 1 hour

  # Rate limiting for auth endpoints
  rate_limit:
    login: 10/minute                 # Per IP
    token_refresh: 30/minute         # Per user
    token_revoke: 30/minute          # Per user
```

### OAuth2 State Parameter and PKCE

**State Parameter:**

The state parameter prevents CSRF attacks in the OAuth2 authorization code flow:

```rust
pub struct OAuth2State {
    /// Random nonce (32 bytes, base64url-encoded)
    pub nonce: String,
    /// Timestamp when state was generated
    pub created_at: DateTime<Utc>,
    /// Redirect URI after successful auth
    pub redirect_uri: Option<String>,
    /// PKCE code_verifier (stored server-side)
    pub code_verifier: Option<String>,
}

impl OAuth2State {
    /// Generate new state with PKCE code_challenge
    pub fn new_with_pkce() -> (Self, String) {
        let code_verifier = generate_random_string(43); // 43-128 chars per RFC 7636
        let code_challenge = base64url(sha256(&code_verifier)); // S256 method
        let state = Self {
            nonce: generate_random_string(32),
            created_at: Utc::now(),
            redirect_uri: None,
            code_verifier: Some(code_verifier),
        };
        (state, code_challenge)
    }

    /// Validate state: must exist, not expired (5 min max), nonce match
    pub fn validate(&self, received_nonce: &str) -> Result<()> {
        if Utc::now() - self.created_at > Duration::minutes(5) {
            return Err(SsoError::InvalidState { reason: "expired" });
        }
        if self.nonce != received_nonce {
            return Err(SsoError::InvalidState { reason: "nonce mismatch" });
        }
        Ok(())
    }
}
```

**PKCE Implementation:**

- `code_challenge_method`: `S256` (SHA-256) — plain method MUST NOT be used
- `code_verifier`: 43-128 characters, unreserved characters `[A-Z] / [a-z] / [0-9] / "-" / "." / "_" / "~"`
- `code_challenge`: `BASE64URL(SHA256(code_verifier))`
- The server stores `code_verifier` in the state session, then sends `code_verifier` in the token exchange request

### API Endpoints

```rust
// SSO authentication
GET  /auth/sso/:provider          // Initiate SSO flow (generates state + PKCE challenge)
GET  /auth/sso/:provider/callback // OAuth2/SAML callback (validates state, exchanges code)
POST /auth/token                  // Token exchange (sends code_verifier for PKCE)
POST /auth/token/refresh          // Refresh token
POST /auth/token/revoke           // Revoke token
POST /auth/logout                 // Logout: revoke session, clear cookies, SAML SLO

// SAML metadata
GET  /auth/sso/saml/metadata      // SP metadata XML

// User info
GET  /auth/userinfo               // Get current user info
GET  /auth/userinfo/claims        // Get token claims

// Token introspection (RFC 7662)
POST /auth/token/introspect       // Introspect opaque token (for resource servers)

// Provider management (admin)
GET    /auth/providers             // List providers
POST   /auth/providers             // Add provider
PUT    /auth/providers/:id         // Update provider
DELETE /auth/providers/:id         // Delete provider

// SCIM 2.0 server endpoints (for IdPs to call)
GET    /scim/v2/Users              // List users (SCIM filter + pagination)
GET    /scim/v2/Users/:id          // Get user
POST   /scim/v2/Users              // Create user
PUT    /scim/v2/Users/:id          // Replace user
PATCH  /scim/v2/Users/:id          // Patch user
DELETE /scim/v2/Users/:id          // Delete user
GET    /scim/v2/Groups             // List groups
GET    /scim/v2/ServiceProviderConfig  // SCIM service provider config
GET    /scim/v2/ResourceTypes      // SCIM resource types
```

### Error Handling

| Error Code | HTTP Status | Description |
|------------|-------------|-------------|
| `sso_provider_not_found` | 404 | SSO provider ID not configured |
| `sso_provider_disabled` | 403 | SSO provider is disabled |
| `sso_invalid_state` | 400 | OAuth2 state parameter mismatch or expired |
| `sso_invalid_code` | 400 | OAuth2 authorization code invalid or expired |
| `sso_token_expired` | 401 | JWT token has expired |
| `sso_token_revoked` | 401 | JWT token has been revoked |
| `sso_token_invalid` | 401 | JWT signature validation failed |
| `sso_token_algorithm_unsupported` | 400 | JWT algorithm not in supported_algorithms |
| `sso_token_algorithm_none` | 400 | JWT uses alg=none (rejected) |
| `sso_audience_mismatch` | 401 | JWT aud claim doesn't match expected audience |
| `sso_issuer_mismatch` | 401 | JWT iss claim doesn't match expected issuer |
| `sso_saml_signature_invalid` | 400 | SAML assertion signature validation failed |
| `sso_saml_assertion_expired` | 400 | SAML assertion NotOnOrAfter has passed |
| `sso_saml_audience_mismatch` | 400 | SAML Audience doesn't match SP entity ID |
| `sso_no_key_mapping` | 403 | SSO user has no virtual key and auto_provision is disabled |
| `sso_user_deactivated` | 403 | SCIM user is deactivated |
| `sso_provider_error` | 502 | External IdP returned an error |
| `sso_rate_limited` | 429 | Auth endpoint rate limit exceeded |

**Error Response Format:**

```json
{
  "error": {
    "code": "sso_token_expired",
    "message": "JWT token has expired",
    "details": {
      "expired_at": "2026-05-17T12:00:00Z"
    }
  }
}
```

### Session Management

Sessions track authenticated users and are stored in stoolap for cross-instance consistency:

```rust
pub struct Session {
    pub session_id: String,          // UUID
    pub user_id: String,             // SSO sub claim
    pub provider_id: String,         // IdentityProvider.id
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,   // Sliding window
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

pub trait SessionStorage {
    async fn create(&self, session: &Session) -> Result<()>;
    async fn get(&self, session_id: &str) -> Result<Option<Session>>;
    async fn refresh(&self, session_id: &str, new_expiry: DateTime<Utc>) -> Result<()>;
    async fn invalidate(&self, session_id: &str) -> Result<()>;
    async fn invalidate_all_for_user(&self, user_id: &str) -> Result<u64>;
    async fn cleanup_expired(&self) -> Result<u64>;
}
```

**Session lifecycle:**
- Created on successful SSO authentication
- Sliding window: each request extends `last_accessed_at` and `expires_at`
- Max idle timeout: configurable `session_lifetime` (default 30 min)
- Absolute max lifetime: 24 hours (configurable) — prevents infinite sliding
- Cleanup: background task removes expired sessions every 5 minutes

**Logout endpoint (`POST /auth/logout`):**
- Revokes the current session
- Revokes the refresh token (if provided)
- For SAML providers: initiates Single Logout (SLO) by redirecting to IdP's SingleLogoutService
- Clears session cookie
- Returns `204 No Content`

### SCIM Server-Side Endpoints

quota-router exposes SCIM 2.0 endpoints at `/scim/v2/` for enterprise IdPs to call for user provisioning:

```rust
/// SCIM server-side request handler
pub struct ScimServer {
    user_storage: Arc<dyn UserStorage>,
    group_storage: Arc<dyn GroupStorage>,
}

impl ScimServer {
    /// Handle SCIM errors with proper status codes
    fn scim_error(status: u16, scim_type: &str, detail: &str) -> ScimError {
        ScimError {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:Error".to_string()],
            status,
            scim_type: Some(scim_type.to_string()),
            detail: detail.to_string(),
        }
    }
}
```

**sync_users() error handling:**

```rust
/// Sync result tracks successes and failures per-user
pub struct SyncResult {
    pub created: u32,
    pub updated: u32,
    pub deactivated: u32,
    pub errors: Vec<SyncError>,
}

pub struct SyncError {
    pub user_id: String,
    pub error: String,
    pub is_retryable: bool,
}

impl ScimProvisioner {
    /// Sync users from IdP with per-user error isolation
    pub async fn sync_users(&self) -> Result<SyncResult> {
        let mut result = SyncResult::default();
        let mut start_index = 1;
        loop {
            let page = self.list_users(None, Some(start_index), Some(100)).await?;
            for user in &page.resources {
                match self.sync_single_user(user).await {
                    Ok(action) => match action {
                        SyncAction::Created => result.created += 1,
                        SyncAction::Updated => result.updated += 1,
                        SyncAction::Deactivated => result.deactivated += 1,
                    },
                    Err(e) => result.errors.push(SyncError {
                        user_id: user.id.clone(),
                        error: e.to_string(),
                        is_retryable: e.is_retryable(),
                    }),
                }
            }
            if start_index + page.items_per_page > page.total_results { break; }
            start_index += page.items_per_page;
        }
        Ok(result)
    }
}
```

### Rate Limiting

Auth endpoints MUST have rate limiting to prevent brute-force and token-harvesting attacks:

| Endpoint | Limit | Scope | Rationale |
|----------|-------|-------|-----------|
| `POST /auth/token` | 10/minute | Per IP | Prevent credential stuffing |
| `POST /auth/token/refresh` | 30/minute | Per user | Prevent refresh token abuse |
| `POST /auth/token/revoke` | 30/minute | Per user | Prevent revocation spam |
| `GET /auth/sso/:provider/callback` | 20/minute | Per IP | Prevent callback abuse |

Rate limits use the same mechanism as RFC-0933 (Rate Limiting Integration). Exceeding limits returns `429 Too Many Requests` with `Retry-After` header.

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| JWT validation | <1ms | Cached JWKS |
| Token introspection | <10ms | External API call |
| SSO redirect | <5ms | HTTP redirect |
| User provisioning | <100ms | Per user sync |

## Security Considerations

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Token theft | High | Short-lived tokens, refresh rotation |
| CSRF in SSO flow | High | State parameter, PKCE |
| JWT signature bypass | Critical | Strict validation, reject none algorithm |
| IdP impersonation | High | Certificate pinning, metadata validation |
| Token replay | Medium | Token binding, audience validation |
| Brute-force login | High | Rate limiting (10/min per IP) |
| Token harvesting | Medium | Rate limiting on /auth/token |
| Refresh token theft | High | One-time use rotation |
| SAML assertion forgery | Critical | XML signature validation |
| SCIM endpoint abuse | Medium | Rate limiting, IP allowlist |
| Token leakage via logs | High | Never log tokens, redact all auth headers |

## Adversarial Review

| Threat | Impact | Mitigation |
|--------|--------|------------|
| State parameter tampering | High | Server-side state storage, 5 min expiry, nonce validation |
| PKCE bypass | High | S256 only, reject plain method, verify code_verifier on token exchange |
| Session fixation | High | New session ID on each login, rotate on privilege escalation |
| SAML assertion replay | Critical | One-time use (InResponseTo check), NotOnOrAfter validation |
| SCIM token theft | High | IP allowlist, rate limiting, token rotation |
| OAuth2 code interception | High | PKCE ensures code alone is insufficient, state prevents CSRF |
| Token introspection abuse | Medium | Rate limiting, require bearer token with appropriate scopes |
| Session cookie theft | Medium | Secure flag, HttpOnly, SameSite=Lax, short lifetime |
| Cross-tenant session access | High | Session scoped to provider_id, cannot access other tenants |

**Design decisions:**
- State parameter: server-side storage (not JWT) prevents client-side tampering
- PKCE: S256 mandatory — plain method vulnerable to authorization code interception
- Session: sliding window with absolute max prevents both idle timeout and infinite sessions
- SCIM server: quota-router acts as SP (receives provisioning), not just client (calls IdP)

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/auth/mod.rs` | New — SSO orchestration |
| `crates/quota-router-core/src/auth/oauth.rs` | New — OAuth2 flows |
| `crates/quota-router-core/src/auth/saml.rs` | New — SAML 2.0 |
| `crates/quota-router-core/src/auth/jwt.rs` | New — JWT validation |
| `crates/quota-router-core/src/auth/scim.rs` | New — SCIM provisioning |
| `crates/quota-router-core/src/auth/key_mapper.rs` | New — SSO-to-API-key mapping |
| `crates/quota-router-core/src/auth/blacklist.rs` | New — Token blacklist |
| `crates/quota-router-core/src/config.rs` | Add SsoConfig |
| `crates/quota-router-core/src/admin.rs` | Add auth endpoints |
| `crates/quota-router-core/src/proxy.rs` | Validate JWT in auth middleware |

## Implementation Phases

### Phase 1: Core Infrastructure

- [ ] Define SsoFlow, IdentityProvider, TokenClaims types
- [ ] Implement JWT validation with JWKS caching
- [ ] Add SsoConfig to config.rs
- [ ] Add /auth/token endpoints
- [ ] Implement SsoKeyMapper
- [ ] Implement TokenBlacklist

### Phase 2: OAuth2/OIDC

- [ ] Implement Authorization Code + PKCE flow
- [ ] Implement Client Credentials flow
- [ ] Add provider management endpoints
- [ ] Integrate with virtual key system

### Phase 3: SAML

- [ ] Implement SP-initiated SAML SSO
- [ ] Parse SAML assertions with XML signature validation
- [ ] Map SAML attributes to user properties
- [ ] Generate SP metadata XML

### Phase 4: SCIM & Advanced

- [ ] Implement SCIM 2.0 user provisioning
- [ ] Add role/team mapping from IdP groups
- [ ] Add SSO analytics and audit log

## Future Work

- F1: Just-in-time provisioning
- F2: Multi-factor authentication enforcement
- F3: Conditional access policies
- F4: Federation across multiple IdPs

## Rationale

### Why OAuth2 + SAML + OIDC?

- OAuth2: Modern standard, widely supported
- SAML: Enterprise standard, required by many IdPs
- OIDC: Best of both (OAuth2 + identity layer)

### Why SCIM for Provisioning?

- Industry standard for user lifecycle management
- Automatic deprovisioning (security requirement)
- Reduces manual user management overhead

### Why SSO-to-API-Key Mapping?

SSO authenticates users, but quota-router's core authorization model uses virtual API keys (RFC-0903). The SsoKeyMapper bridges this gap: SSO identity → virtual key → authorization. This preserves the existing key-based authorization model while adding SSO authentication.

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| API key only | Simple | No user identity, no SSO |
| LDAP integration | Works with AD | Legacy, complex |
| Custom auth | Flexible | Maintenance burden, security risk |
| Proxy auth (nginx) | Simple | No user management integration |

## Test Vectors

### JWT Validation

```
# Valid RS256 token
Header: {"alg":"RS256","typ":"JWT","kid":"key-1"}
Payload: {"sub":"user-123","email":"user@example.com","iss":"https://example.okta.com","aud":"quota-router","exp":9999999999}
Result: PASS

# Expired token
Header: {"alg":"RS256","typ":"JWT","kid":"key-1"}
Payload: {"sub":"user-123","iss":"https://example.okta.com","aud":"quota-router","exp":1000000000}
Result: FAIL — sso_token_expired

# alg=none attack
Header: {"alg":"none","typ":"JWT"}
Payload: {"sub":"admin","iss":"https://example.okta.com","aud":"quota-router","exp":9999999999}
Result: FAIL — sso_token_algorithm_none

# Audience mismatch
Header: {"alg":"RS256","typ":"JWT","kid":"key-1"}
Payload: {"sub":"user-123","iss":"https://example.okta.com","aud":"other-app","exp":9999999999}
Result: FAIL — sso_audience_mismatch
```

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1 | 2026-05-17 | Initial draft |
| v2 | 2026-05-17 | Adversarial review Round 1 fixes — SSO-to-API-key mapping, token lifecycle, SAML spec, SCIM spec, JWT algorithm spec, rate limiting, error handling |
| v3 | 2026-05-17 | Adversarial review Round 2 fixes — SsoKeyStorageExt trait, TokenBlacklistStorage trait, OAuth2 state/PKCE spec, SCIM server endpoints, session management, /auth/logout, SAML multi-valued attributes, Adversarial Review section, RFC-0933 dependency |

## Related RFCs

- RFC-0903 (Economics): Virtual API Key System
- RFC-0932 (Economics): Team Management
- RFC-0905 (Economics): Observability and Logging

## Related Use Cases

- [Enterprise AI Gateway](../../docs/use-cases/enhanced-quota-router-gateway.md)
- [Enhanced Quota Router Gateway](../../docs/use-cases/enhanced-quota-router-gateway.md)
