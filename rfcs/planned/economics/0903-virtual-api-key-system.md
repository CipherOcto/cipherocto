# RFC-0903 (Economics): Virtual API Key System

## Status

Planned

## Authors

- Author: @cipherocto

## Summary

Define the virtual API key system for the enhanced quota router, enabling key generation, validation, per-key budgets, rate limiting, and access control.

## Dependencies

**Requires:**

- RFC-0900 (Economics): AI Quota Marketplace Protocol
- RFC-0901 (Economics): Quota Router Agent Specification

**Optional:**

- RFC-0902: Multi-Provider Routing (for key-specific routing)
- RFC-0904: Real-Time Cost Tracking (for budget tracking)

## Why Needed

The enhanced quota router must support multiple users with:

- **Key-based authentication** - Users authenticate via API keys
- **Per-key budgets** - Each key has its own spend limit
- **Rate limiting** - Per-key RPM/TPM limits
- **Team organization** - Keys belong to teams with shared budgets

## Scope

### In Scope

- API key generation (UUID-based)
- Key validation middleware
- Per-key budget limits (daily, weekly, monthly)
- Per-key rate limiting (RPM, TPM)
- Key expiry and rotation
- Key metadata (name, team, created date)

### Out of Scope

- OAuth2/JWT authentication (future)
- SSO integration (future)
- Key usage analytics (RFC-0905)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | <1ms key validation | Auth latency |
| G2 | Support 10K+ keys | Key count |
| G3 | Atomic budget updates | No overspend |
| G4 | Key rotation without downtime | Availability |

## Specification

### Key Model

```rust
struct ApiKey {
    key_id: Uuid,           // Public identifier
    key_hash: String,        // Hashed key for validation
    key_prefix: String,      // First 7 chars for display (e.g., "sk-qr-abc")

    // Organization
    team_id: Option<Uuid>,   // Team membership

    // Budget limits (OCTO-W or USD)
    budget_type: BudgetType,
    budget_limit: i64,       // -1 for unlimited

    // Rate limits
    rpm_limit: Option<u32>,  // Requests per minute
    tpm_limit: Option<u32>,  // Tokens per minute

    // Validity
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    revoked: bool,

    // Metadata
    description: Option<String>,
    metadata: HashMap<String, String>,
}

enum BudgetType {
    OCTOW(i64),    // OCTO-W tokens
    USD(i64),      // USD equivalent (converted via price feed)
}
```

### Team Model

```rust
struct Team {
    team_id: Uuid,
    name: String,

    // Shared budget
    budget_type: BudgetType,
    budget_limit: i64,
    current_spend: i64,

    // Team settings
    created_at: DateTime<Utc>,
}
```

### API Endpoints

```rust
// Key management (Admin)
POST   /key/generate     // Create new API key
GET    /key/list         // List keys (with filters)
DELETE /key/{key_id}     // Revoke key
PUT    /key/{key_id}     // Update key (budget, limits)

// Team management
POST   /team             // Create team
GET    /team/{team_id}   // Get team info
PUT    /team/{team_id}   // Update team
```

### Key Validation Middleware

```rust
async fn validate_key(
    key: &str,
    request: &Request,
) -> Result<ApiKey, KeyError> {
    // 1. Extract key from Authorization header
    let key = extract_bearer_token(request)?;

    // 2. Hash and lookup
    let key_hash = hash(key);
    let api_key = lookup_key(&key_hash)?;

    // 3. Check expiry
    if let Some(expires) = api_key.expires_at {
        if Utc::now() > expires {
            return Err(KeyError::Expired);
        }
    }

    // 4. Check revoked
    if api_key.revoked {
        return Err(KeyError::Revoked);
    }

    // 5. Check budget (requires RFC-0904 integration)
    check_budget(&api_key)?;

    // 6. Check rate limits
    check_rate_limit(&api_key)?;

    Ok(api_key)
}
```

### LiteLLM Compatibility

> **Critical:** Must track LiteLLM's key management API.

Reference LiteLLM's virtual key system:
- `/key/generate` endpoint compatibility
- Key format: `sk-...` prefix
- Budget tracking via `litellm.max_budget`
- Rate limiting via `litellm.rpm_limit`

### Persistence

> **Critical:** Use CipherOcto/stoolap as the persistence layer.

All key and team data stored in stoolap:
- API keys table
- Teams table
- Key usage logs

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-cli/src/auth.rs` | New - key validation |
| `crates/quota-router-cli/src/keys.rs` | New - key management |
| `crates/quota-router-cli/src/teams.rs` | New - team management |
| `crates/quota-router-cli/src/middleware.rs` | Add auth middleware |

## Future Work

- F1: OAuth2/JWT authentication
- F2: API key rotation automation
- F3: Key usage analytics dashboard
- F4: Team-based access control (RBAC)

## Rationale

Virtual API keys are essential for:

1. **Multi-tenancy** - Multiple users on single router
2. **Budget control** - Prevent runaway spend
3. **Rate limiting** - Prevent abuse
4. **Enterprise ready** - Teams with shared budgets
5. **LiteLLM migration** - Match key management features

---

**Planned Date:** 2026-03-12
**Related Use Case:** Enhanced Quota Router Gateway
**Related Research:** LiteLLM Analysis and Quota Router Comparison
