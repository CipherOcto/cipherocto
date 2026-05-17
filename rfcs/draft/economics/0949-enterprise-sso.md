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
}

impl TokenValidator {
    /// Validate JWT token
    pub async fn validate(&self, token: &str) -> Result<TokenClaims> {
        // 1. Fetch JWKS keys (cached)
        // 2. Verify signature
        // 3. Check expiration
        // 4. Check issuer and audience
        // 5. Return claims
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
}
```

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
```

### API Endpoints

```rust
// SSO authentication
GET  /auth/sso/:provider          // Initiate SSO flow
GET  /auth/sso/:provider/callback // OAuth2/SAML callback
POST /auth/token                  // Token exchange
POST /auth/token/refresh          // Refresh token
POST /auth/token/revoke           // Revoke token

// User info
GET  /auth/userinfo               // Get current user info
GET  /auth/userinfo/claims        // Get token claims

// Provider management (admin)
GET    /auth/providers             // List providers
POST   /auth/providers             // Add provider
PUT    /auth/providers/:id         // Update provider
DELETE /auth/providers/:id         // Delete provider
```

### User Provisioning

```rust
/// SCIM-based user provisioning
pub struct ScimProvisioner {
    /// SCIM endpoint URL
    url: String,
    /// Bearer token for SCIM API
    token: String,
    /// HTTP client
    client: reqwest::Client,
}

impl ScimProvisioner {
    /// Sync users from IdP
    pub async fn sync_users(&self) -> Result<Vec<ScimUser>> {
        // GET /Users from SCIM endpoint
        // Map to quota-router users
        // Create/update/deactivate as needed
    }

    /// Push user changes to IdP
    pub async fn push_user(&self, user: &User) -> Result<()> {
        // POST/PUT /Users to SCIM endpoint
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScimUser {
    pub id: String,
    pub user_name: String,
    pub emails: Vec<String>,
    pub active: bool,
    pub groups: Vec<String>,
}
```

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

## Adversarial Review

| Threat | Impact | Mitigation |
|--------|--------|------------|
| JWT alg=none attack | Critical | Reject tokens without valid signature |
| State parameter manipulation | High | Cryptographic state, validate on callback |
| Token leakage via logs | High | Never log tokens, redact all auth headers |
| SCIM endpoint abuse | Medium | Rate limiting, IP allowlist |
| Refresh token theft | High | Rotation, one-time use |

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/auth/mod.rs` | New — SSO orchestration |
| `crates/quota-router-core/src/auth/oauth.rs` | New — OAuth2 flows |
| `crates/quota-router-core/src/auth/saml.rs` | New — SAML 2.0 |
| `crates/quota-router-core/src/auth/jwt.rs` | New — JWT validation |
| `crates/quota-router-core/src/auth/scim.rs` | New — SCIM provisioning |
| `crates/quota-router-core/src/config.rs` | Add SsoConfig |
| `crates/quota-router-core/src/admin.rs` | Add auth endpoints |
| `crates/quota-router-core/src/proxy.rs` | Validate JWT in auth middleware |

## Implementation Phases

### Phase 1: Core Infrastructure

- [ ] Define SsoFlow, IdentityProvider, TokenClaims types
- [ ] Implement JWT validation with JWKS caching
- [ ] Add SsoConfig to config.rs
- [ ] Add /auth/token endpoints

### Phase 2: OAuth2/OIDC

- [ ] Implement Authorization Code + PKCE flow
- [ ] Implement Client Credentials flow
- [ ] Add provider management endpoints
- [ ] Integrate with virtual key system

### Phase 3: SAML

- [ ] Implement SP-initiated SAML SSO
- [ ] Parse SAML assertions
- [ ] Map SAML attributes to user properties

### Phase 4: SCIM & Advanced

- [ ] Implement SCIM user provisioning
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

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| API key only | Simple | No user identity, no SSO |
| LDAP integration | Works with AD | Legacy, complex |
| Custom auth | Flexible | Maintenance burden, security risk |
| Proxy auth (nginx) | Simple | No user management integration |

## Version History

| Version | Date | Changes |
|---------|------|---------|
| v1 | 2026-05-17 | Initial draft |

## Related RFCs

- RFC-0903 (Economics): Virtual API Key System
- RFC-0932 (Economics): Team Management
- RFC-0905 (Economics): Observability and Logging

## Related Use Cases

- Enterprise AI Gateway
- Enhanced Quota Router Gateway
