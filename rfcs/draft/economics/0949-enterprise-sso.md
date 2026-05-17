# RFC-0949 (Economics): Enterprise SSO Integration

## Status

Draft

## Authors

- Author: @cipherocto

## Summary

Define enterprise Single Sign-On (SSO) integration for quota-router, supporting OAuth2, SAML 2.0, and OpenID Connect (OIDC) protocols. Enables enterprise users to authenticate via their existing identity providers (Okta, Azure AD, Google Workspace, etc.) instead of managing separate quota-router credentials.

## Dependencies

**Requires:**

- RFC-0932 (Economics): Gateway Auth & API Key Management
- RFC-0903 (Economics): Virtual API Key System

**Optional:**

- RFC-0905 (Economics): Observability and Logging (for auth audit logs)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | SSO login in <3s | End-to-end auth latency |
| G2 | Zero separate credentials | Users authenticate via IdP only |
| G3 | Automatic provisioning | Users created on first SSO login |
| G4 | Role mapping | IdP groups → quota-router roles |

## Motivation

Enterprise customers require SSO for:

1. **Compliance** — SOC2, HIPAA mandate centralized identity management
2. **User lifecycle** — Deactivate in IdP → deactivates everywhere
3. **Password policy** — Enforce enterprise password/MFA policies
4. **Audit trail** — Centralized login audit in IdP

Currently quota-router only supports API key auth (RFC-0903) and local user accounts (RFC-0932). Enterprise users must manage separate credentials, which blocks adoption.

## Specification

### 1. SSO Provider Configuration

```yaml
# config.yaml
sso:
  enabled: true
  providers:
    - name: okta
      type: oidc
      issuer: https://company.okta.com/oauth2/default
      client_id: ${OKTA_CLIENT_ID}
      client_secret: ${OKTA_CLIENT_SECRET}
      scopes: [openid, profile, email, groups]
      role_mapping:
        admins: admin
        developers: member
        viewers: viewer

    - name: azure-ad
      type: oidc
      issuer: https://login.microsoftonline.com/${TENANT_ID}/v2.0
      client_id: ${AZURE_CLIENT_ID}
      client_secret: ${AZURE_CLIENT_SECRET}
      scopes: [openid, profile, email]
      role_mapping:
        "quota-router-admins": admin
        "quota-router-users": member

    - name: okta-saml
      type: saml
      metadata_url: https://company.okta.com/app/xxx/sso/saml/metadata
      certificate: ${SAML_CERTIFICATE}
      role_attribute: groups
      role_mapping:
        admins: admin
        developers: member

  default_role: viewer
  auto_create_users: true
  session_ttl: 3600  # seconds
```

### 2. OAuth2/OIDC Flow

```rust
// auth/sso/oauth2.rs

/// Authorization Code Flow with PKCE
pub struct OAuth2Provider {
    issuer: String,
    client_id: String,
    client_secret: String,
    scopes: Vec<String>,
    role_mapping: HashMap<String, Role>,
}

impl OAuth2Provider {
    /// Generate authorization URL with PKCE challenge
    pub fn authorize_url(&self, state: &str, code_challenge: &str) -> String {
        // GET /authorize?
        //   response_type=code
        //   &client_id={client_id}
        //   &redirect_uri={redirect_uri}
        //   &scope={scopes}
        //   &state={state}
        //   &code_challenge={code_challenge}
        //   &code_challenge_method=S256
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str, code_verifier: &str) -> Result<TokenResponse> {
        // POST /token
        //   grant_type=authorization_code
        //   &code={code}
        //   &redirect_uri={redirect_uri}
        //   &client_id={client_id}
        //   &client_secret={client_secret}
        //   &code_verifier={code_verifier}
    }

    /// Fetch user info from userinfo endpoint
    pub async fn userinfo(&self, access_token: &str) -> Result<UserInfo> {
        // GET /userinfo
        // Authorization: Bearer {access_token}
    }

    /// Map IdP groups to quota-router role
    pub fn map_role(&self, groups: &[String]) -> Role {
        // Check role_mapping for first matching group
        // Default to configured default_role
    }
}
```

### 3. SAML 2.0 Flow

```rust
// auth/sso/saml.rs

pub struct SamlProvider {
    metadata_url: String,
    certificate: String,
    role_attribute: String,
    role_mapping: HashMap<String, Role>,
}

impl SamlProvider {
    /// Generate AuthnRequest
    pub fn authn_request(&self, relay_state: &str) -> String {
        // SAML AuthnRequest XML
    }

    /// Parse and validate SAML Response
    pub fn validate_response(&self, saml_response: &str) -> Result<UserInfo> {
        // 1. Base64 decode
        // 2. Parse XML
        // 3. Validate signature against certificate
        // 4. Check conditions (NotBefore, NotOnOrAfter)
        // 5. Extract attributes (email, name, groups)
    }
}
```

### 4. User Provisioning

```rust
// auth/sso/provisioning.rs

/// Auto-provision user on first SSO login
pub async fn provision_user(
    storage: &dyn KeyStorage,
    userinfo: &UserInfo,
    provider: &str,
    role: Role,
) -> Result<User> {
    // 1. Check if user exists by email
    if let Some(user) = storage.get_user_by_email(&userinfo.email).await? {
        // Update last login, SSO provider link
        storage.update_user_login(user.id, provider).await?;
        return Ok(user);
    }

    // 2. Create new user
    let user = User {
        user_id: generate_user_id(),
        email: userinfo.email.clone(),
        name: userinfo.name.clone(),
        role,
        auth_method: AuthMethod::Sso(provider.to_string()),
        created_at: now(),
        last_login: now(),
    };
    storage.create_user(&user).await?;
    Ok(user)
}
```

### 5. Session Management

```rust
// auth/sso/session.rs

pub struct SsoSession {
    session_id: String,
    user_id: String,
    provider: String,
    access_token: String,  // encrypted at rest
    refresh_token: Option<String>,  // encrypted at rest
    expires_at: DateTime<Utc>,
}

impl SsoSession {
    /// Create session after successful SSO login
    pub async fn create(storage: &dyn KeyStorage, user: &User, tokens: &TokenResponse) -> Result<Self>;

    /// Validate session (check expiry, not revoked)
    pub async fn validate(storage: &dyn KeyStorage, session_id: &str) -> Result<Self>;

    /// Refresh session using refresh_token
    pub async fn refresh(&self, provider: &OAuth2Provider) -> Result<Self>;

    /// Revoke session (logout)
    pub async fn revoke(storage: &dyn KeyStorage, session_id: &str) -> Result<()>;
}
```

### 6. Admin Endpoints

```
GET  /sso/providers           — List configured SSO providers
GET  /sso/{provider}/authorize — Redirect to IdP login
POST /sso/{provider}/callback  — Handle IdP callback (code exchange)
POST /sso/logout              — Revoke session
GET  /sso/session             — Get current session info
```

### 7. Integration with RFC-0932 User Management

SSO-provisioned users are stored in the same `users` table as local users, with `auth_method: Sso(provider)`. This enables:

- Unified user listing (`/user/list`)
- Unified role management (`/user/update`)
- SSO users cannot change password (managed by IdP)
- SSO users can still receive API keys (RFC-0903)

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/auth/mod.rs` | New — auth module |
| `crates/quota-router-core/src/auth/sso.rs` | New — SSO coordinator |
| `crates/quota-router-core/src/auth/oauth2.rs` | New — OAuth2/OIDC provider |
| `crates/quota-router-core/src/auth/saml.rs` | New — SAML 2.0 provider |
| `crates/quota-router-core/src/auth/session.rs` | New — session management |
| `crates/quota-router-core/src/config.rs` | Add SsoConfig |
| `crates/quota-router-core/src/admin.rs` | Add /sso/* endpoints |
| `crates/quota-router-core/src/storage.rs` | Add session storage methods |

## Security Considerations

| Threat | Mitigation |
|--------|------------|
| CSRF | State parameter + PKCE |
| Token theft | Encrypt tokens at rest, short TTL |
| Session fixation | New session ID on each login |
| Replay attacks | Nonce validation in SAML |
| Open redirect | Validate redirect_uri against allowlist |
| IdP impersonation | Certificate pinning for SAML, issuer validation for OIDC |

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| SSO login latency | <3s | Including IdP redirect |
| Token refresh | <500ms | Background refresh |
| Session validation | <1ms | Cache-backed |

## Alternatives Considered

| Approach | Pros | Cons |
|----------|------|------|
| LDAP/AD direct | No IdP dependency | Complex setup, security risk |
| Proxy-based (oauth2-proxy) | Simple | Extra infra, limited integration |
| JWT-only (no SAML) | Simpler | Misses enterprise SAML requirement |

## Implementation Phases

### Phase 1: Core (MVP)

- [ ] OAuth2/OIDC provider with PKCE
- [ ] User auto-provisioning
- [ ] Session management
- [ ] Admin endpoints (/sso/authorize, /sso/callback)
- [ ] Config parsing

### Phase 2: SAML

- [ ] SAML 2.0 provider
- [ ] SAML response validation
- [ ] Certificate management

### Phase 3: Enterprise

- [ ] Multi-provider support
- [ ] Just-in-time provisioning
- [ ] Session revocation across instances
- [ ] Audit logging (RFC-0905 integration)

## Future Work

- F1: SCIM 2.0 for user provisioning from IdP
- F2: MFA enforcement via IdP
- F3: IdP-initiated SSO
- F4: SSO analytics dashboard

## Rationale

OAuth2/OIDC as primary (simpler, modern) with SAML for legacy enterprise. PKCE prevents authorization code interception. Auto-provisioning reduces admin overhead. Session-based auth complements API key auth (RFC-0903) — users login via SSO, then generate API keys for programmatic access.

---

**Version:** 1.0
**Submission Date:** 2026-05-17
**Related RFCs:** RFC-0903, RFC-0932, RFC-0905
**Related Use Case:** Enhanced Quota Router Gateway
