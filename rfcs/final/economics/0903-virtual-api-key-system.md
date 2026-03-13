# RFC-0903 (Economics): Virtual API Key System

## Status

Final (v28 - hygiene fixes)

## Authors

- Author: @cipherocto

## Summary

Define the virtual API key system for the enhanced quota router, enabling key generation, validation, per-key budgets, rate limiting, and access control. Based on LiteLLM's key management with CipherOcto/stoolap persistence.

## Dependencies

**Requires:**

- RFC-0126: Deterministic Serialization (for canonical JSON serialization)

**Optional:**

- RFC-0909: Deterministic Quota Accounting (defines ledger enforcement semantics)
- RFC-0900 (Economics): AI Quota Marketplace Protocol
- RFC-0901 (Economics): Quota Router Agent Specification
- RFC-0902: Multi-Provider Routing (for key-specific routing)
- RFC-0904: Real-Time Cost Tracking (for budget tracking)
- RFC-0910: Pricing Table Registry (for immutable pricing)

## Why Needed

The enhanced quota router must support multiple users with:

- **Key-based authentication** - Users authenticate via API keys
- **Per-key budgets** - Each key has its own spend limit
- **Rate limiting** - Per-key RPM/TPM limits
- **Team organization** - Keys belong to teams with shared budgets

## Scope

### In Scope

- API key generation (UUID-based, sk-qr- prefix for LiteLLM compatibility)
- Key validation middleware
- Per-key budget limits (daily, weekly, monthly)
- Per-key rate limiting (RPM, TPM)
- Key expiry and rotation (auto-rotate support)
- Key metadata (name, team, created date)
- Key types (LLM_API, MANAGEMENT, READ_ONLY, DEFAULT)
- Team-based access control

### Out of Scope

- OAuth2/JWT authentication (future)
- SSO integration (future)
- Key usage analytics (RFC-0905)

## Design Goals

| Goal | Target                        | Metric       |
| ---- | ----------------------------- | ------------ |
| G1   | <1ms key validation           | Auth latency |
| G2   | Support 10K+ keys             | Key count    |
| G3   | Atomic budget updates         | No overspend |
| G4   | Key rotation without downtime | Availability |

## LiteLLM Compatibility

> **Critical:** Must match LiteLLM's virtual key system for drop-in replacement.

Reference LiteLLM's key management (`litellm/proxy/_types.py`):

- **Key Types:** `LiteLLMKeyType` enum - LLM_API, MANAGEMENT, READ_ONLY, DEFAULT
- **Key hashing:** Uses SHA-256 (`hash_token()` in `_types.py:211-217`)
- **GenerateKeyRequest:** key, key_type, auto_rotate, rotation_interval, organization_id, project_id, budget, rpm/tpm limits
- **GenerateKeyResponse:** key, expires, user_id, token_id, organization_id
- **Rate limits:** `rpm_limit`, `tpm_limit` fields directly on keys
- **Authorization:** `allowed_routes` field for route permissions
- **Key format:** `sk-qr-...` prefix (quota-router variant of LiteLLM's `sk-...`)

> **Security Note:** LiteLLM uses plain SHA-256 for key hashing. This RFC improves security by using HMAC-SHA256 with a server secret, as recommended for production systems.

## Persistence Layer

> **Critical:** Use CipherOcto/stoolap as the embedded persistence layer.

Based on stoolap's API (`src/api/database.rs`):

```rust
use stoolap::{Database, params};

// Open embedded database (memory or file)
let db = Database::open("file:///data/keys.db")?;

// DDL
db.execute(
    "CREATE TABLE api_keys (
        key_id TEXT PRIMARY KEY,
        key_hash BYTEA NOT NULL,
        key_prefix TEXT NOT NULL CHECK (length(key_prefix) >= 8),
        team_id TEXT,
        budget_limit INTEGER NOT NULL,
        rpm_limit INTEGER,
        tpm_limit INTEGER,
        created_at INTEGER NOT NULL,
        expires_at INTEGER,
        revoked INTEGER DEFAULT 0,
        description TEXT,
        metadata TEXT,
        rotated_from TEXT,
        rotation_grace_until INTEGER
    )",
    ()
)?;

db.execute(
    "CREATE TABLE teams (
        team_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        budget_limit INTEGER NOT NULL,
        created_at INTEGER NOT NULL
    )",
    ()
)?;
```

## Specification

### Key Model

```rust
/// Key type enum - determines what routes a key can access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyType {
    /// Can call LLM API routes (chat/completions, embeddings, etc.)
    LlmApi,
    /// Can call management routes (user/team/key management)
    Management,
    /// Can only call info/read routes
    ReadOnly,
    /// Uses default allowed routes
    Default,
}

/// API Key entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Public identifier (UUID)
    pub key_id: Uuid,
    /// Hashed key for validation (HMAC-SHA256 with server secret)
    /// Stored as binary (Vec<u8>) for efficiency - avoids hex conversion
    pub key_hash: Vec<u8>,
    /// First 8 chars for display (e.g., "sk-qr-a1b2***" - rest hidden)
    /// 8 chars provides better collision resistance than 6
    pub key_prefix: String,

    /// Team membership
    pub team_id: Option<Uuid>,

    /// Budget limit in deterministic cost units (u64)
    /// All budgets stored as integer cost units for deterministic accounting
    pub budget_limit: u64,
    /// DERIVED CACHE - Computed from spend_ledger for fast lookups
    /// NOT authoritative - use ledger for exact balance
    /// Use: SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = ?
    pub current_spend: u64,

    /// Rate limits
    pub rpm_limit: Option<u32>,
    pub tpm_limit: Option<u32>,

    /// Validity (epoch timestamps in seconds - deterministic)
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub revoked: bool,
    pub revoked_at: Option<i64>,
    pub revoked_by: Option<String>,
    pub revocation_reason: Option<String>,

    /// Key type (LiteLLM compatibility)
    pub key_type: KeyType,

    /// Allowed routes (LiteLLM compatibility, JSON array format: ["\\/v1\\/chat","\\/v1\\/embeddings"])
    pub allowed_routes: Vec<String>,

    /// Auto-rotation
    pub auto_rotate: bool,
    pub rotation_interval_days: Option<u32>,
    /// Key rotation tracking
    pub rotated_from: Option<Uuid>,      // Previous key ID when rotated
    pub rotation_grace_until: Option<i64>, // Grace period end timestamp

    /// Metadata (use BTreeMap for deterministic serialization)
    pub description: Option<String>,
    pub metadata: BTreeMap<String, String>,
}
```

### Team Model

```rust
/// Team entity - shared budget and access control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub team_id: Uuid,
    pub name: String,

    /// Shared budget in deterministic cost units (u64)
    pub budget_limit: u64,
    /// DERIVED CACHE - Computed from spend_ledger for fast lookups
    /// NOT authoritative - use ledger for exact balance
    /// Use: SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE team_id = ?
    pub current_spend: u64,

    /// Settings (epoch timestamp - deterministic)
    pub created_at: i64,
}
```

### Request/Response Types

```rust
/// Key generation request (LiteLLM compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateKeyRequest {
    /// Optional existing key (for regeneration)
    pub key: Option<String>,
    /// Budget limit in deterministic cost units (u64)
    pub budget_limit: u64,
    /// Rate limits
    pub rpm_limit: Option<u32>,
    pub tpm_limit: Option<u32>,
    /// Key type
    #[serde(default)]
    pub key_type: KeyType,
    /// Auto-rotation
    pub auto_rotate: Option<bool>,
    /// Rotation interval - use RotationInterval enum for type-safe parsing
    pub rotation_interval_days: Option<u32>,
    /// Organization
    pub team_id: Option<Uuid>,
    /// Metadata (BTreeMap for deterministic serialization)
    pub metadata: Option<BTreeMap<String, String>>,
    pub description: Option<String>,
}

/// Type-safe rotation interval parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationInterval {
    Days(u32),
    Weeks(u32),
}

impl RotationInterval {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if let Some(days) = s.strip_suffix("d").or_else(|| s.strip_suffix("D")) {
            days.parse().ok().map(RotationInterval::Days)
        } else if let Some(weeks) = s.strip_suffix("w").or_else(|| s.strip_suffix("W")) {
            weeks.parse().ok().map(|w| RotationInterval::Days(w * 7))
        } else {
            None
        }
    }

    pub fn as_days(&self) -> u32 {
        match self {
            RotationInterval::Days(d) => *d,
            RotationInterval::Weeks(w) => w * 7,
        }
    }
}

/// Key generation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateKeyResponse {
    /// The actual API key (sk-qr-...)
    pub key: String,
    /// Public key identifier
    pub key_id: Uuid,
    /// Expiration timestamp (epoch seconds - deterministic)
    pub expires: Option<i64>,
    /// Team ID if associated
    pub team_id: Option<Uuid>,
    /// Key type
    pub key_type: KeyType,
    /// Created timestamp (epoch seconds - deterministic)
    pub created_at: i64,
}
```

### API Endpoints

```rust
// Key management (Admin)
POST   /key/generate     // Create new API key (LiteLLM compatible)
GET    /key/list         // List keys (with filters)
DELETE /key/{key_id}     // Revoke key
PUT    /key/{key_id}     // Update key (budget, limits)
POST   /key/regenerate   // Rotate key

// Team management
POST   /team             // Create team
GET    /team/{team_id}  // Get team info
PUT    /team/{team_id}  // Update team

// LiteLLM compatibility endpoints
GET    /global/supported   // List supported models
GET    /key/info           // Get key info from token
```

### Key Validation Middleware

```rust
/// Key validation errors
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyError {
    InvalidKey,
    Expired,
    Revoked,
    BudgetExceeded { current: u64, limit: u64 },
    RateLimited { retry_after: u64 },
    TeamBudgetExceeded { current: u64, limit: u64 },
    TeamKeyLimitExceeded { team_id: Uuid, current: u32, limit: u32 },
}

/// Validate API key middleware
/// Note: Budget validation happens atomically in record_spend() (ledger-based), not here.
/// This ensures no race conditions between check and update.
pub async fn validate_key(
    db: &Database,
    request: &Request,
) -> Result<ApiKey, KeyError> {
    // 1. Extract key from Authorization header
    let key = extract_bearer_token(request)?;

    // 2. Hash and lookup (HMAC-SHA256 with server secret)
    let key_hash = hmac_sha256(server_secret, key);
    let api_key = lookup_key(db, &key_hash)?;

    // 3. Check expiry
    if let Some(expires) = api_key.expires_at {
        if Utc::now().timestamp() > expires {
            return Err(KeyError::Expired);
        }
    }

    // 4. Check revoked
    if api_key.revoked {
        return Err(KeyError::Revoked);
    }

    // 5. Check rate limits (RPM/TPM) - in-memory check, no DB
    check_rate_limit(db, &api_key)?;

    Ok(api_key)
}

/// Soft budget pre-check to avoid wasted provider round-trips
/// This is a non-locking check and may race, but improves UX for obviously over-budget keys.
/// The authoritative check happens atomically in record_spend() after request completes.
///
/// # Parameters
/// - `db`: Database connection
/// - `key_id`: The API key to check
/// - `estimated_max_cost`: Upper bound cost estimate for the request
///
/// # How to estimate max_cost
/// For LLM requests where output tokens are unknown until response:
/// - Use a configured per-model ceiling (e.g., gpt-4 max is ~32k tokens × output_price)
/// - Or conservatively use `budget_limit` itself as the ceiling
/// - For streaming requests, skip this check as output tokens are unknown
pub fn check_budget_soft_limit(db: &Database, key_id: &Uuid, estimated_max_cost: u64) -> Result<(), KeyError> {
    // Query returns BIGINT (i64); cast to u64 is safe since cost_amount is non-negative
    let current: u64 = db.query_row(
        "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
        params![key_id.to_string()],
        |row| row.get::<_, i64>(0),
    ).map(|v: i64| v.try_into().unwrap_or(u64::MAX))?;

    // budget_limit is BIGINT NOT NULL CHECK (budget_limit >= 0), so always non-negative
    let budget: u64 = db.query_row(
        "SELECT budget_limit FROM api_keys WHERE key_id = $1",
        params![key_id.to_string()],
        |row| row.get::<_, i64>(0),
    ).map(|v: i64| v.try_into().unwrap_or(u64::MAX))?;

    if current.saturating_add(estimated_max_cost) > budget {
        return Err(KeyError::BudgetExceeded { current, limit: budget });
    }
    Ok(())
}
```

### Database Schema

```sql
-- Keys table
-- Note: current_spend is REMOVED - it's derived from spend_ledger for deterministic accounting
CREATE TABLE api_keys (
    key_id TEXT PRIMARY KEY,
    key_hash BYTEA NOT NULL,
    key_prefix TEXT NOT NULL CHECK (length(key_prefix) >= 8),
    team_id TEXT,
    budget_limit BIGINT NOT NULL CHECK (budget_limit >= 0),
    -- current_spend derived from: SELECT SUM(cost_amount) FROM spend_ledger WHERE key_id = ?
    rpm_limit INTEGER CHECK (rpm_limit >= 0),
    tpm_limit INTEGER CHECK (tpm_limit >= 0),
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked INTEGER DEFAULT 0,
    revoked_at INTEGER,
    revoked_by TEXT,
    revocation_reason TEXT,
    key_type TEXT DEFAULT 'default',
    allowed_routes TEXT,
    auto_rotate INTEGER DEFAULT 0,
    rotation_interval_days INTEGER,
    description TEXT,
    metadata TEXT,
    FOREIGN KEY (team_id) REFERENCES teams(team_id) ON DELETE SET NULL
);

-- Trigger to enforce MAX_KEYS_PER_TEAM (100 keys per team)
CREATE OR REPLACE FUNCTION check_team_key_limit()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.team_id IS NOT NULL THEN
        IF (SELECT COUNT(*) FROM api_keys WHERE team_id = NEW.team_id) >= 100 THEN
            RAISE EXCEPTION 'Team key limit exceeded (max 100 keys)';
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_team_key_limit
    BEFORE INSERT ON api_keys
    FOR EACH ROW
    WHEN (NEW.team_id IS NOT NULL)
    EXECUTE FUNCTION check_team_key_limit();

-- Teams table
-- Note: current_spend is REMOVED - it's derived from spend_ledger for deterministic accounting
CREATE TABLE teams (
    team_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    budget_limit BIGINT NOT NULL CHECK (budget_limit >= 0),
    -- current_spend derived from: SELECT SUM(cost_amount) FROM spend_ledger WHERE team_id = ?
    created_at INTEGER NOT NULL
);

-- Indexes for performance
-- CRITICAL: Index on key_hash for lookup path (not key_id)
-- This accelerates the actual lookup: WHERE key_hash = $1 AND revoked = 0
CREATE INDEX idx_api_keys_hash_active ON api_keys(key_hash) WHERE revoked = 0;
-- Ensure no duplicate key hashes
CREATE UNIQUE INDEX idx_api_keys_key_hash_unique ON api_keys(key_hash);
CREATE INDEX idx_api_keys_team_id ON api_keys(team_id);
CREATE INDEX idx_api_keys_expires ON api_keys(expires_at);
CREATE INDEX idx_teams_team_id ON teams(team_id);
```

### Atomic Budget Accounting

> **DEPRECATED APPROACH:** The counter-based approach below is deprecated.
> For deterministic accounting, use the **Ledger-Based Architecture** approach defined later in this RFC.
> The ledger approach (`FOR UPDATE` + `SUM from ledger`) is the canonical implementation.

Critical: Budget updates MUST be atomic to prevent overspend.

**Database Isolation Requirement:**

Database MUST guarantee at least **REPEATABLE READ** isolation level (SERIALIZABLE preferred). This ensures two concurrent transactions cannot both pass the budget check and overspend.

```sql
-- Set isolation level (PostgreSQL example)
SET TRANSACTION ISOLATION LEVEL SERIALIZABLE;
-- Or at connection level
ALTER DATABASE quota_router SET DEFAULT_TRANSACTION_ISOLATION TO 'SERIALIZABLE';
```

**Canonical Approach:** Use the ledger-based `record_spend()` function from the Ledger-Based Architecture section. It uses:
- `FOR UPDATE` row locking to prevent race conditions
- `SUM(cost_amount) FROM spend_ledger` for deterministic accounting
- No mutable `current_spend` counter

```rust
/// DEPRECATED: Use ledger-based record_spend() instead
/// This function uses mutable counters which break deterministic accounting
#[deprecated(since = "v22", note = "Use record_spend() from Ledger-Based Architecture")]
pub fn record_spend_atomic(
    _db: &Database,
    _key_id: &Uuid,
    _amount: u64,
) -> Result<(), KeyError> {
    unimplemented!("record_spend_atomic is deprecated - use ledger-based record_spend()")
}
```

```rust
/// DEPRECATED: Use ledger-based record_spend() instead
/// This function uses mutable counters which break deterministic accounting
#[deprecated(since = "v22", note = "Use record_spend() from Ledger-Based Architecture")]
pub fn record_spend_with_team_atomic(
    _db: &Database,
    _key_id: &Uuid,
    _team_id: Option<Uuid>,
    _amount: u64,
) -> Result<(), KeyError> {
    unimplemented!("record_spend_with_team_atomic is deprecated - use ledger-based record_spend()")
}
```

### Rate Limiting Algorithm

Token Bucket algorithm for RPM/TPM enforcement using **integer arithmetic** for deterministic behavior:

```rust
use std::time::{Duration, Instant};

/// Token bucket rate limiter for per-key rate limiting
/// Uses u64 integers for cross-platform deterministic behavior
/// Uses Instant for monotonic time source (immune to clock adjustments)
pub struct TokenBucket {
    capacity: u64,
    tokens: u64,
    /// Refill rate: tokens per minute (stored as-is, converted in calculations)
    refill_rate_per_minute: u64,
    last_refill: Instant,     // Monotonic time - immune to clock adjustments
    last_access: Instant,     // For cleanup_stale_buckets - track idle time
}

impl TokenBucket {
    pub fn new(capacity: u32, refill_per_minute: u32) -> Self {
        // Store rate as tokens per minute
        let refill_rate_per_minute = refill_per_minute as u64;
        let now = Instant::now();
        Self {
            capacity: capacity as u64,
            tokens: capacity as u64,
            refill_rate_per_minute,
            // Initialize to current monotonic time to avoid massive refill on first call
            last_refill: now,
            last_access: now,  // Track last access for cleanup
        }
    }

    /// Try to consume tokens, returns false if rate limited
    pub fn try_consume(&mut self, tokens_to_consume: u32) -> bool {
        self.refill();

        let tokens_needed = tokens_to_consume as u64;
        if self.tokens >= tokens_needed {
            self.tokens = self.tokens.saturating_sub(tokens_needed);
            self.last_access = Instant::now();  // Update last access time
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        // Use monotonic time - immune to system clock adjustments
        let now = Instant::now();
        let elapsed = self.last_refill.elapsed();
        let delta_secs = elapsed.as_secs() as u64;
        // Second-granularity refill: tokens per second = rate / 60
        // Use (delta_secs * rate) / 60 with proper integer math
        let new_tokens = delta_secs
            .saturating_mul(self.refill_rate_per_minute)
            .saturating_div(60);
        self.tokens = self.tokens.saturating_add(new_tokens).min(self.capacity);
        // Update last refill time to current monotonic time
        self.last_refill = now;
    }

    /// Returns retry-after seconds
    pub fn retry_after(&self) -> u64 {
        if self.tokens >= 1 {
            0
        } else {
            // Calculate seconds needed to get 1 token: 60 seconds / tokens_per_second
            // Using ceiling division: (60 + rate - 1) / rate
            let seconds_per_token = if self.refill_rate_per_minute > 0 {
                (60 + self.refill_rate_per_minute - 1) / self.refill_rate_per_minute
            } else {
                60 // 1 minute if no refill
            };
            seconds_per_token.max(1)
        }
    }
}
```

### Rate Limiter Storage

Rate limiters are stored per-key using DashMap for concurrent access:

```rust
use dashmap::DashMap;

pub struct RateLimiterStore {
    /// Per-key token buckets - DashMap for concurrent access
    buckets: DashMap<Uuid, (TokenBucket, TokenBucket)>, // (RPM, TPM)
}

impl RateLimiterStore {
    pub fn new() -> Self {
        Self {
            buckets: DashMap::new(),
        }
    }

    /// Check and consume tokens - mutates the bucket IN PLACE in DashMap
    pub fn check_rate_limit(&self, key: &ApiKey, tokens: u32) -> Result<(), KeyError> {
        // Get or create entry - must mutate in place
        let entry = self.buckets.entry(key.key_id).or_insert_with(|| {
            (
                TokenBucket::new(100, key.rpm_limit.unwrap_or(100)),
                TokenBucket::new(1000, key.tpm_limit.unwrap_or(1000)),
            )
        });

        // Mutate the entry directly (not a clone!)
        let buckets = entry.value_mut();

        // Check RPM
        if !buckets.0.try_consume(1) {
            return Err(KeyError::RateLimited {
                retry_after: buckets.0.retry_after(),
            });
        }

        // Check TPM
        if !buckets.1.try_consume(tokens) {
            return Err(KeyError::RateLimited {
                retry_after: buckets.1.retry_after(),
            });
        }

        Ok(())
    }

    /// Invalidate rate limiter for a key (call on key revocation)
    pub fn invalidate(&self, key_id: &Uuid) {
        self.buckets.remove(key_id);
    }

    /// Cleanup worker - removes stale buckets to prevent memory growth
    /// Must be called periodically (e.g., every 5 minutes)
    /// Also enforces max_size cap to prevent unbounded growth
    pub fn cleanup_stale_buckets(&self, max_idle_ms: u64, max_size: usize) {
        let now = Instant::now();
        let max_idle = Duration::from_millis(max_idle_ms);

        let stale_keys: Vec<Uuid> = self.buckets
            .iter()
            .filter(|(_, bucket)| now.duration_since(bucket.last_access) > max_idle)
            .map(|(key, _)| *key)
            .collect();

        for key in stale_keys {
            self.buckets.remove(&key);
        }

        // If still over max_size after cleanup, remove oldest entries
        if self.buckets.len() > max_size {
            let mut buckets: Vec<_> = self.buckets.iter()
                .map(|(k, v)| (*k, v.last_access))
                .collect();
            buckets.sort_by_key(|(_, access)| *access);

            let to_remove = self.buckets.len() - max_size;
            for (key, _) in buckets.into_iter().take(to_remove) {
                self.buckets.remove(&key);
            }
        }
    }
}
```

**Note:** L1 cache assumes single router instance. For multi-node deployments, use Redis-backed rate limiting.

### Key Rotation Protocol

Rotation with grace period for zero-downtime:

```rust
/// Rotation grace period in seconds (24 hours)
const ROTATION_GRACE_PERIOD_SECS: i64 = 86400;

/// Rotate key with grace period
/// Note: Budget is NOT carried over - new key starts fresh at 0 spend.
/// This is intentional: rotation provides a clean slate.
/// Note: Accepts cache to invalidate old key immediately (no TTL-based grace)
pub fn rotate_key(
    db: &Database,
    cache: &KeyCache,
    key_id: &Uuid,
) -> Result<GenerateKeyResponse, Error> {
    // Capture timestamp once for all time-related fields
    let now = Utc::now().timestamp();

    // 1. Get old key info and capture hash for cache invalidation
    let old_key = lookup_key(db, key_id)?;
    let old_key_hash = old_key.key_hash.clone(); // Capture for cache invalidation

    // 2. Generate new key and new key_id
    let new_key_id = Uuid::now_v7();
    let new_key = generate_key_string();
    let new_key_hash = hmac_sha256(server_secret, &new_key);
    let new_key_prefix = new_key.chars().take(8).collect::<String>();

    // 3. Insert new key with reference to old key (audit trail: new was rotated from old)
    db.execute(
        "INSERT INTO api_keys (
            key_id, key_hash, key_prefix, team_id,
            budget_limit, rpm_limit, tpm_limit,
            created_at, expires_at, key_type, allowed_routes,
            auto_rotate, rotation_interval_days, description, metadata,
            rotated_from
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        params![
            new_key_id.to_string(),
            new_key_hash,
            new_key_prefix,
            old_key.team_id.map(|t| t.to_string()),
            old_key.budget_limit,
            old_key.rpm_limit,
            old_key.tpm_limit,
            now,
            old_key.rotation_interval_days.map(|d| now + d as i64 * 86400),
            serialize_key_type(&old_key.key_type),
            serde_json::to_string(&old_key.allowed_routes).unwrap_or_default(),
            old_key.auto_rotate,
            old_key.rotation_interval_days,
            old_key.description,
            serde_json::to_string(&old_key.metadata).unwrap_or_default(),
            key_id.to_string(),  // rotated_from = old key ID
        ],
    )?;

    // 4. Invalidate old key from cache immediately (no TTL grace)
    cache.invalidate(&old_key_hash);

    Ok(GenerateKeyResponse {
        key: new_key,
        key_id: new_key_id, // Use the SAME new key_id
        expires: None,
        team_id: old_key.team_id,
        key_type: old_key.key_type,
        created_at: now,
    })
}
```

### Authorization Route Mapping

Define which routes each key type can access. Uses slash enforcement to prevent bypasses:

```rust
/// Route permission mapping with slash enforcement
pub fn check_route_permission(key: &ApiKey, route: &str) -> bool {
    // CRITICAL: Normalize path BEFORE checking to prevent bypass attacks
    // e.g., /v1/chat/../admin -> /v1/admin
    // SECURITY: Reject double-encoded paths (normalize_path returns Err on attack)
    let Ok(normalized) = normalize_path(route) else {
        return false; // Reject suspicious paths
    };

    // 1. Check explicit allowed_routes first (JSON array in database)
    // Format: ["\\/v1\\/chat","\\/v1\\/embeddings"]
    if !key.allowed_routes.is_empty() {
        return key.allowed_routes.iter().any(|r| {
            // Enforce trailing slash or exact match
            let with_slash = format!("{}/", r);
            normalized.starts_with(&with_slash) || normalized == r
        });
    }

    // 2. Fall back to key_type defaults
    match key.key_type {
        KeyType::LlmApi => {
            // Use exact prefix + slash to prevent /v1/chatX bypass
            normalized == "/v1/chat"
                || normalized.starts_with("/v1/chat/")
                || normalized == "/v1/completions"
                || normalized.starts_with("/v1/completions/")
                || normalized == "/v1/embeddings"
                || normalized.starts_with("/v1/embeddings/")
        }
        KeyType::Management => {
            normalized.starts_with("/key/")
                || normalized.starts_with("/team/")
                || normalized.starts_with("/user/")
        }
        KeyType::ReadOnly => {
            normalized.starts_with("/models/")
                || normalized.starts_with("/info")
        }
        KeyType::Default => true, // Allow all
    }
}
```

### L1 Cache for Fast Lookups

In-memory LRU cache for sub-millisecond key validation:

```rust
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use parking_lot::RwLock;
use std::time::{Duration, Instant};

/// L1 cache configuration
const CACHE_SIZE: usize = 10_000;
const CACHE_TTL_SECS: u64 = 30;

/// Cached key entry with TTL
/// Uses Arc<ApiKey> to avoid cloning on cache hits
struct CacheEntry {
    api_key: Arc<ApiKey>,
    cached_at: Instant,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > Duration::from_secs(CACHE_TTL_SECS)
    }
}

/// L1 key cache - in-memory LRU with TTL
/// Uses Vec<u8> for cache key to match binary key_hash storage
pub struct KeyCache {
    cache: Arc<RwLock<LruCache<Vec<u8>, CacheEntry>>>,
}

impl KeyCache {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruCache::new(NonZeroUsize::new(CACHE_SIZE).unwrap()))),
        }
    }

    /// Get key from cache (with TTL check)
    /// Uses Vec<u8> to avoid hex conversion overhead
    /// Returns Arc<ApiKey> to avoid cloning
    pub fn get(&self, key_hash: &[u8]) -> Option<Arc<ApiKey>> {
        let cache = self.cache.read();
        let entry = cache.get(key_hash)?;

        // Check TTL
        if entry.is_expired() {
            drop(cache);
            self.invalidate(key_hash);
            return None;
        }

        Some(Arc::clone(&entry.api_key))
    }

    /// Put key into cache
    /// Takes ownership of Vec<u8> to avoid copies
    /// Wraps ApiKey in Arc to avoid cloning on cache hits
    pub fn put(&self, key_hash: Vec<u8>, api_key: ApiKey) {
        let mut cache = self.cache.write();
        cache.put(key_hash, CacheEntry {
            api_key: Arc::new(api_key),
            cached_at: Instant::now(),
        });
    }

    /// Invalidate key in cache (on update/revoke)
    pub fn invalidate(&self, key_hash: &[u8]) {
        let mut cache = self.cache.write();
        cache.pop(key_hash);
    }

    /// Clear entire cache
    pub fn clear(&self) {
        let mut cache = self.cache.write();
        cache.clear();
    }
}

/// Validate key with L1 cache
/// Returns Arc<ApiKey> to avoid cloning on cache hits
pub fn validate_key_with_cache(
    db: &Database,
    cache: &KeyCache,
    key: &str,
) -> Result<Arc<ApiKey>, KeyError> {
    // 1. Compute hash
    let key_hash = hmac_sha256(server_secret, key);

    // 2. Check L1 cache first
    if let Some(cached_key) = cache.get(&key_hash) {
        // Validate cached key - dereference Arc to access fields
        if !cached_key.revoked {
            if let Some(expires) = cached_key.expires_at {
                if Utc::now().timestamp() > expires {
                    cache.invalidate(&key_hash);
                    return Err(KeyError::Expired);
                }
            }
            return Ok(cached_key);
        } else {
            // Key was revoked, invalidate
            cache.invalidate(&key_hash);
            return Err(KeyError::Revoked);
        }
    }

    // 3. Cache miss - lookup in database
    let api_key = lookup_key(db, &key_hash)?;

    // 4. Validate
    if let Some(expires) = api_key.expires_at {
        if Utc::now().timestamp() > expires {
            return Err(KeyError::Expired);
        }
    }
    if api_key.revoked {
        return Err(KeyError::Revoked);
    }

    // 5. Add to cache (put takes ApiKey, internally wraps in Arc)
    cache.put(key_hash, api_key.clone());

    Ok(Arc::new(api_key))
}
```

**Performance estimate:**

- Cache hit: ~0.1ms (L1 lookup)
- Cache miss: ~1-3ms (DB lookup + cache population)
- Target: <1ms average with 80%+ cache hit rate

## Implementation Notes

### Key Generation

```rust
use rand::RngCore;
use std::fmt::Write;

/// Generate a cryptographically secure API key (256-bit entropy)
/// Uses random 32 bytes encoded in hex for bias-free encoding
fn generate_key_string() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);

    // Hex encoding - bias-free, deterministic, URL-safe
    let mut hex_string = String::with_capacity(64);
    for byte in &bytes {
        write!(&mut hex_string, "{:02x}", byte).unwrap();
    }

    format!("sk-qr-{}", hex_string)
}

/// Generate a new API key
pub fn generate_key(db: &Database, req: GenerateKeyRequest) -> Result<GenerateKeyResponse, Error> {
    // Capture timestamp once for all time-related fields
    let now = Utc::now().timestamp();

    // Generate UUID for key_id (internal reference)
    let key_id = Uuid::now_v7();
    // Generate cryptographically secure key (256-bit entropy)
    let key = generate_key_string();
    let key_hash = hmac_sha256(server_secret, &key);
    let key_prefix = key.chars().take(8).collect::<String>();

    // Use rotation_interval_days directly from request (type-safe)
    let rotation_interval_days = req.rotation_interval_days;

    // Insert into database
    db.execute(
        "INSERT INTO api_keys (
            key_id, key_hash, key_prefix, team_id,
            budget_limit, rpm_limit, tpm_limit,
            created_at, expires_at, key_type, auto_rotate,
            rotation_interval_days, description, metadata
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        params![
            key_id.to_string(),
            key_hash,
            key_prefix,
            req.team_id.map(|t| t.to_string()),
            req.budget_limit,
            req.rpm_limit,
            req.tpm_limit,
            now,
            req.rotation_interval_days.map(|d| now + (d as i64 * 86400)),
            serialize_key_type(&req.key_type),
            req.auto_rotate.unwrap_or(false),
            rotation_interval_days,
            req.description,
            serde_json::to_string(&req.metadata).unwrap_or_default(),
        ],
    )?;

    // Compute expiration if rotation interval is set
    let expires = req.rotation_interval_days.map(|d| now + (d as i64 * 86400));

    Ok(GenerateKeyResponse {
        key,
        key_id,
        expires, // Present if auto_rotate enabled with rotation_interval_days
        team_id: req.team_id,
        key_type: req.key_type,
        created_at: now,
    })
}
```

### Key Validation (Performance Optimized)

```rust
/// Fast key lookup with caching
/// Uses binary key_hash (Vec<u8>) for efficient lookup
pub fn lookup_key(db: &Database, key_hash: &[u8]) -> Result<ApiKey, KeyError> {
    // Use prepared statement for performance
    let mut rows = db.query(
        "SELECT * FROM api_keys WHERE key_hash = $1 AND revoked = 0",
        params![key_hash],
    )?;

    if let Some(row) = rows.next()? {
        Ok(row_to_api_key(row)?)
    } else {
        Err(KeyError::InvalidKey)
    }
}
```

## Dependencies

Additional Rust dependencies required:

```toml
# Cargo.toml additions
[dependencies]
lru = "0.12"           # LRU cache for L1 key cache
hmac = "0.12"          # HMAC for key hashing
sha2 = "0.10"          # SHA-256 for HMAC
dashmap = "6.0"         # Concurrent HashMap for rate limiter storage
rand = "0.8"            # Cryptographic random bytes (already in workspace)
subtle = "2.5"          # Constant-time comparison for secure hash comparison
percent-encoding = "2.3" # URL percent decoding for path normalization
```

### Deterministic Serialization

All serialized structures MUST follow RFC-0126 Deterministic Serialization.

Key implications:

- Use `BTreeMap` instead of `HashMap` for metadata (ensures consistent key ordering)
- Store timestamps as epoch seconds (i64) for canonical representation, but note that timestamps are **operational metadata** and MUST NOT influence economic accounting decisions
- Store key_hash as BYTEA, not TEXT (binary for efficiency)
- Use JSON with canonical ordering for allowed_routes
- **metadata and allowed_routes MUST be serialized using RFC-0126 Deterministic Serialization canonical JSON rules** before database insertion to ensure multiple routers produce identical serialization
- **All cost calculations MUST use integer arithmetic. Floating point numbers MUST NOT be used in budget accounting.**

### Non-Deterministic Components

The following components are **non-deterministic** and MUST NOT be used for accounting logic:

| Component            | Type       | Purpose                | Note                                 |
| -------------------- | ---------- | ---------------------- | ------------------------------------ |
| `Utc::now()`         | Clock time | created_at, expires_at | Operational only, not for accounting |
| `Instant::now()`     | Monotonic  | Rate limiter refill    | Per-process, not replayable          |
| `rand::thread_rng()` | Entropy    | Key string generation  | Not reproducible                     |
| `Uuid::now_v7()`     | Random     | key_id generation      | Use UUIDv7 for time-ordered          |

**Important:** These are fine for operational use but MUST NOT be used in any code path that contributes to deterministic accounting state. Budget checks happen atomically via `record_spend()` (ledger-based) which computes from spend_ledger.

**Timestamps are operational metadata only:**

- Timestamps MUST NOT influence economic state transitions
- Budget enforcement must never depend on time
- Use timestamps for operational tracking (audit logs, expiry checks) but not for deterministic replay

**UUIDs are operational identifiers only:**

- `key_id` and other UUIDs are for routing and lookup
- Deterministic replay should use `event_hash` derived from content, not UUIDs

### Security Requirements

```
server_secret MUST be >= 256-bit cryptographic secret
stored outside database (environment variable or secrets manager)
loaded at startup, never logged or exposed
```

#### Constant-Time Comparison

When comparing key hashes, use constant-time comparison to prevent timing attacks:

```rust
use subtle::ConstantTimeEq;

/// Compare hashes in constant time
fn secure_compare(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).unwrap_u8() == 1
}
```

**Note:** Constant-time comparison is not strictly required for this implementation because key lookup uses database equality (`WHERE key_hash = $1`), which handles comparison server-side. This section is included for completeness in other comparison contexts.

### Spend Accounting Unit

This RFC is **currency-agnostic**. Budgets are stored as deterministic integer cost units.

For currency definitions (nano_octow, nano_usd), see **RFC-0904: Real-Time Cost Tracking**.

## Key Files to Create/Modify

| File                                           | Change                           |
| ---------------------------------------------- | -------------------------------- |
| `crates/quota-router-core/src/keys.rs`         | New - key generation, validation |
| `crates/quota-router-core/src/teams.rs`        | New - team management            |
| `crates/quota-router-core/src/auth.rs`         | New - auth middleware            |
| `crates/quota-router-core/src/cache.rs`        | New - L1 key cache               |
| `crates/quota-router-core/src/storage.rs`      | New - stoolap database layer     |
| `crates/quota-router-core/src/rate_limiter.rs` | New - token bucket rate limiter  |
| `crates/quota-router-cli/src/main.rs`          | Add key management routes        |

### Cache Invalidation

Cache must be invalidated on:

- Key revocation (`revoke_key()`)
- Key update (`update_key()`)
- Key rotation (`rotate_key()`)
- Key expiry check (TTL handles this)

```rust
/// Revoke key with cache invalidation
pub fn revoke_key(
    db: &Database,
    cache: &KeyCache,
    rate_limiter: &RateLimiterStore,
    key_id: &Uuid,
    revoked_by: &str,  // Caller identity for audit trail
    reason: &str,
) -> Result<(), KeyError> {
    let key_hash = lookup_key_hash(db, key_id)?;

    // CRITICAL: Update database FIRST (source of truth) before invalidating cache
    // This prevents a crash scenario where cache is cleared but DB still has active key
    db.execute(
        "UPDATE api_keys SET revoked = 1, revoked_at = $1, revoked_by = $2, revocation_reason = $3 WHERE key_id = $4",
        params![Utc::now().timestamp(), revoked_by, reason, key_id.to_string()],
    )?;

    // Invalidate cache AFTER DB update (safe now that source of truth is updated)
    cache.invalidate(&key_hash);

    // Invalidate rate limiter
    rate_limiter.invalidate(key_id);

    Ok(())
}
```

### Key Rotation Worker

Background worker for automatic rotation:

```rust
/// Rotation worker - runs every 5 minutes
pub async fn rotation_worker(db: &Database, cache: &KeyCache) {
    let interval = tokio::time::interval(Duration::from_secs(300));

    loop {
        interval.tick().await;

        // Find keys that need rotation
        let expired_keys = db.query(
            "SELECT key_id FROM api_keys WHERE auto_rotate = 1 AND expires_at < $1",
            params![Utc::now().timestamp()],
        )?;

        for row in expired_keys {
            let key_id: String = row.get("key_id")?;
            let key_uuid = Uuid::parse_str(&key_id).unwrap();

            // Rotate key
            if let Err(e) = rotate_key(db, cache, &key_uuid) {
                tracing::error!("Key rotation failed: {}", e);
            }
        }
    }
}
```

### Route Normalization

To prevent authorization bypasses like `/v1/chat/../management`, normalize paths:

```rust
use percent_encoding::percent_decode_str;

/// Decode percent-encoded path THEN normalize to prevent bypass attacks
/// e.g., /v1/chat/%2e%2e/admin -> /v1/chat/../admin -> /v1/admin
///
/// SECURITY: Reject double-encoded paths to prevent path traversal bypass
/// e.g., %252e%252e -> %2e%2e -> ..
/// Returns Err(()) on security violation, Ok(normalized_path) on success
fn normalize_path(path: &str) -> Result<String, ()> {
    // First check for double-encoded sequences - reject them
    let upper = path.to_uppercase();
    if upper.contains("%252E") || upper.contains("%252F") ||
       upper.contains("%25.") || upper.contains("%25/") {
        // Double encoding detected - reject the request
        return Err(());
    }

    // First decode percent encoding
    let decoded = percent_decode_str(path).decode_utf8_lossy();

    let mut segments = Vec::new();
    for segment in decoded.split('/') {
        match segment {
            "" | "." => continue,
            ".." => { segments.pop(); }
            _ => segments.push(segment),
        }
    }
    Ok(format!("/{}", segments.join("/")))
}
```

### Ledger-Based Spend Recording (Canonical Approach)

RFC-0903 uses the **spend_ledger** as the single source of truth for deterministic accounting. `current_spend` on ApiKey/Team is a DERIVED CACHE computed from the ledger.

```sql
-- Spend ledger - THE authoritative economic record (see Ledger-Based Architecture)
-- Token counts MUST originate from provider when available (see Canonical Token Accounting)
CREATE TABLE spend_ledger (
    event_id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    team_id TEXT,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_amount BIGINT NOT NULL,
    pricing_hash BYTEA(32) NOT NULL,  -- SHA256 = 32 bytes
    timestamp INTEGER NOT NULL,
    -- Token source for deterministic accounting
    token_source TEXT NOT NULL CHECK (token_source IN ('provider_usage', 'canonical_tokenizer')),
    tokenizer_version TEXT,
    provider_usage_json TEXT,  -- Raw provider usage for audit
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    -- Scoped uniqueness: request_id unique per key
    UNIQUE(key_id, request_id),
    -- Foreign keys for integrity
    FOREIGN KEY(key_id) REFERENCES api_keys(key_id) ON DELETE CASCADE,
    FOREIGN KEY(team_id) REFERENCES teams(team_id) ON DELETE SET NULL
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
-- Composite index for efficient quota queries
CREATE INDEX idx_spend_ledger_key_time ON spend_ledger(key_id, timestamp);
```

**Deterministic event_id generation:**

```rust
use sha2::{Sha256, Digest};

/// Generate deterministic event_id from request content
/// This enables deterministic replay and duplicate detection
/// CRITICAL: event_id includes token_source for deterministic hash across routers
fn compute_event_id(
    request_id: &str,
    key_id: &Uuid,
    provider: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    pricing_hash: &[u8; 32],
    token_source: TokenSource,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(request_id.as_bytes());
    hasher.update(key_id.to_string().as_bytes());
    hasher.update(provider.as_bytes());
    hasher.update(model.as_bytes());
    hasher.update(input_tokens.to_le_bytes());
    hasher.update(output_tokens.to_le_bytes());
    hasher.update(pricing_hash);
    // Include token_source so routers with different sources produce different hashes
    // Note: These strings ("provider", "tokenizer") are DIFFERENT from the DB storage
    // strings ("provider_usage", "canonical_tokenizer") used in record_spend().
    // This is intentional - hash strings are for deterministic identity, DB strings
    // are for audit/constraint validation. They serve different purposes.
    // Use methods to prevent accidental inconsistency
    hasher.update(token_source.to_hash_str().as_bytes());
    format!("{:x}", hasher.finalize())
}
```

**Pricing immutability rule:**

```
pricing_hash MUST reference an immutable pricing table snapshot (RFC-0910).
This ensures the same tokens produce the same cost across all routers.
```

**Fallback (before RFC-0910 exists):**

```
pricing_hash = SHA256(canonical pricing table JSON)
```

This ensures pricing determinism is defined even before RFC-0910 is implemented.

**Timestamp determinism rule:**

```
timestamp is METADATA ONLY - it does NOT participate in event_id hash.
event_id defines canonical identity.
timestamp allows temporal ordering for audit but is not required for deterministic replay.
```

**Ledger-based transaction ordering:**

The atomic pattern MUST use this order (as implemented in `record_spend()`):

```
1. SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE (lock key row)
2. SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1 (compute from ledger)
3. Verify budget not exceeded
4. INSERT INTO spend_ledger (event always inserted, idempotent via ON CONFLICT)
5. COMMIT
```

This prevents overspend by using the ledger as the single source of truth.

### Canonical Token Accounting

**Critical determinism rule:**

Two routers processing the same request MUST produce identical token counts, otherwise deterministic accounting fails.

**The token drift problem:**

Different routers may measure tokens differently due to:
- Tokenizer version differences
- Whitespace normalization differences
- Streaming chunk boundary differences
- Provider returning different usage metadata

This causes **deterministic accounting failure** where the same request produces different costs.

**Canonical token source rule:**

```
Priority 1: Provider-reported tokens (from response.usage)
Priority 2: Canonical tokenizer (RFC-0910 pinned implementation)
Priority 3: REJECT - cannot account without verifiable source
```

**CRITICAL invariant:**

```
For a given request_id, ALL routers MUST use the SAME token source.
token_source MUST be included in event_id hash.
```

Example divergence that must be prevented:

```
Router A: token_source = provider_usage
Router B: token_source = canonical_tokenizer
→ Different event_id = deterministic failure
```

```
Local tokenizer estimation MUST NOT be used for accounting.
```

**Token source recording:**

All spend events MUST record the token source:

```rust
/// Token source for determining which tokens were used for accounting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenSource {
    /// Token counts from provider response (preferred)
    ProviderUsage,
    /// Fallback: token counts from canonical tokenizer (when provider doesn't return usage)
    CanonicalTokenizer,
}

impl TokenSource {
    /// String representation for event_id hash computation
    /// DIFFERENT from to_db_str() - used in SHA256 hash
    pub fn to_hash_str(&self) -> &'static str {
        match self {
            TokenSource::ProviderUsage => "provider",
            TokenSource::CanonicalTokenizer => "tokenizer",
        }
    }

    /// String representation for database storage
    /// DIFFERENT from to_hash_str() - used in CHECK constraint
    pub fn to_db_str(&self) -> &'static str {
        match self {
            TokenSource::ProviderUsage => "provider_usage",
            TokenSource::CanonicalTokenizer => "canonical_tokenizer",
        }
    }
}

/// Complete spend event for deterministic accounting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendEvent {
    /// Deterministic event ID (SHA256 hash of request_id + key_id + provider + model + tokens + pricing_hash + token_source)
    pub event_id: String,
    /// Request identifier for idempotency (UNIQUE constraint)
    pub request_id: String,
    /// API key that made the request
    pub key_id: Uuid,
    /// Team (if key belongs to a team)
    pub team_id: Option<String>,
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,
    /// Model used (e.g., "gpt-4", "claude-3-opus")
    pub model: String,
    /// Number of input tokens
    pub input_tokens: u32,
    /// Number of output tokens
    pub output_tokens: u32,
    /// Total cost in deterministic micro-units
    pub cost_amount: u64,
    /// Hash of pricing table used (for audit trail)
    pub pricing_hash: [u8; 32],
    /// Token source for determining token count origin
    pub token_source: TokenSource,
    /// Version of canonical tokenizer used (if token_source is CanonicalTokenizer)
    pub tokenizer_version: Option<String>,
    /// Raw provider usage JSON for audit (optional)
    pub provider_usage_json: Option<String>,
    /// Event timestamp (epoch seconds)
    pub timestamp: i64,
}
```

**Replay safety invariant:**

```
For a given request_id, only ONE spend event may exist.
This is enforced by UNIQUE(key_id, request_id) constraint.
```

**Provider-truth anchoring (optional but recommended):**

Store raw provider usage for audit:

```sql
ALTER TABLE spend_ledger ADD COLUMN provider_usage_json TEXT;
```

```rust
// Instead of computing tokens locally, store provider truth:
let event = SpendEvent {
    provider_usage_json: Some(serde_json::to_string(&response.usage).unwrap()),
    // ... cost computed from stored provider usage
};
```

This eliminates tokenizer drift entirely.

**Counter as derived cache:**

```
current_spend is a DERIVED ACCELERATION CACHE, not authoritative state.

For deterministic replay:
    current_spend = SUM(spend_ledger.cost_amount)

The authoritative source is the spend_ledger table.
This enables ledger reconciliation and audit verification.
```

### Quota Consistency Model

**Critical consistency rule:**

Multiple routers processing requests simultaneously can cause **cross-router double-spend** if quota enforcement is not properly isolated.

**The double-spend problem:**

```
budget_limit = 1000
current_spend = 990

Router A reads:  current_spend = 990
Router B reads:  current_spend = 990

Both check: 990 + 20 ≤ 1000 ✓
Both commit: current_spend = 1030

Budget exceeded - double-spend occurred!
```

**Quota enforcement rules:**

```
1. Quota enforcement MUST occur against a strongly consistent
   primary database instance with row-level locking.

2. Routers MUST NOT enforce quotas using replica reads
   or eventually consistent storage.

3. All quota updates MUST occur via atomic SQL transactions.

4. The budget invariant MUST hold at all times:
   0 ≤ current_spend ≤ budget_limit
```

**Atomic spend pattern (ledger-only):**

```sql
-- Atomic spend enforcement using ledger as single source of truth
BEGIN;

-- 1. Lock the key row (acts as budget mutex - this is what FOR UPDATE locks)
SELECT budget_limit FROM api_keys WHERE key_id = $key_id FOR UPDATE;

-- 2. Compute current spend from ledger (authoritative)
SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $key_id;

-- 3. Verify budget not exceeded (application logic)
-- If current + amount > budget_limit, ROLLBACK

-- 4. Insert spend event (authoritative record)
INSERT INTO spend_ledger (event_id, key_id, request_id, ...)
VALUES ($1, $2, $3, ...)
ON CONFLICT(key_id, request_id) DO NOTHING;

COMMIT;
```

Note: `current_spend` in api_keys table is a DERIVED CACHE - do NOT update it in the transaction. It will be recomputed from the ledger.

**Idempotent request enforcement:**

```
request_id UNIQUE constraint ensures duplicate requests cannot double-charge.
```

This is already enforced by:
```sql
-- Scoped per key_id for multi-tenant safety
request_id TEXT NOT NULL,
UNIQUE(key_id, request_id)
```

**Single-writer principle:**

For deterministic accounting across multiple routers:

```
Router → Primary DB (strong consistency) → Spend Event Recorded
```

Routers should never write to replicas for quota operations.

**Atomic update pattern:**

```rust
/// DEPRECATED: Uses mutable counter - breaks deterministic accounting
/// Use record_spend() from Ledger-Based Architecture instead
#[deprecated(since = "v22", note = "Use ledger-based record_spend()")]
pub fn record_spend_with_event(
    db: &Database,
    key_id: &Uuid,
    event: &SpendEvent,
) -> Result<(), KeyError> {
    let tx = db.transaction()?;

    // 1. Atomic budget check + update FIRST (prevents orphan events)
    let rows = tx.execute(
        "UPDATE api_keys
         SET current_spend = current_spend + $1
         WHERE key_id = $2
         AND current_spend + $1 <= budget_limit",
        params![event.cost_amount as i64, key_id.to_string()],
    )?;

    // If budget exceeded, rollback immediately (no orphan events)
    if rows == 0 {
        tx.rollback()?;
        return Err(KeyError::BudgetExceeded {
            current: 0,
            limit: 0,
        });
    }

    // 2. Insert spend event ONLY if budget update succeeded
    // Use ON CONFLICT for idempotent retry handling
    // NOTE: This deprecated function omits team_id - team budget queries will miss these events.
    // Use record_spend_with_team() instead for team-attributed spend.
    tx.execute(
        "INSERT INTO spend_ledger (
            event_id, key_id, request_id, provider, model,
            input_tokens, output_tokens, cost_amount, pricing_hash, timestamp,
            token_source, tokenizer_version
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT(key_id, request_id) DO NOTHING",
        params![
            event.event_id.to_string(),
            event.key_id.to_string(),
            event.request_id,
            event.provider,
            event.model,
            event.input_tokens,
            event.output_tokens,
            event.cost_amount as i64,
            &event.pricing_hash,
            event.timestamp,
            match event.token_source {
                TokenSource::ProviderUsage => "provider_usage",
                TokenSource::CanonicalTokenizer => "canonical_tokenizer",
            },
            event.tokenizer_version,
        ],
    )?;

    tx.commit()?;
    Ok(())
}
```

**Benefits:**

- Full economic history for auditing
- Deterministic replay: `current_spend = SUM(events.cost_amount)`
- Fraud detection and dispute resolution
- Future: Merkle spend proofs for blockchain anchoring
- Multi-router determinism verification

This pattern is essential for RFC-0909 (Deterministic Quota Accounting).

### Economic Invariants

The following invariants MUST hold at all times:

```
1. spend_ledger are the authoritative economic record
2. current_spend = SUM(spend_ledger.cost_amount)
3. 0 ≤ current_spend ≤ budget_limit
4. request_id uniqueness prevents double charging
5. pricing_hash ensures deterministic cost calculation
6. token_source MUST be identical across routers for a given request_id
```

### Deterministic Replay Procedure

For audit and verification, deterministic replay MUST follow this procedure:

```
1. Load all spend_ledger for a key_id
2. Order by event_id (canonical identity)
3. Compute current_spend = SUM(events.cost_amount)
4. Verify equality: computed_spend == stored current_spend
5. If mismatch, trust spend_ledger as authoritative
```

This ensures economic audit can always reconcile the ledger.

### Rate Limiting Determinism

```
Rate limiting decisions MUST NOT influence spend recording.

If a provider request executed → spend MUST be recorded.
Even if rate limiter would have denied the request locally.
Rate limiting uses non-deterministic clocks (Instant) and is separate from accounting.
```

### Cache Revocation Rule

```
Cache MUST NOT return revoked keys, even if TTL not expired.
Revoke operation MUST propagate cache invalidation across all threads.

Multi-node deployments require distributed cache invalidation:
- Redis pub/sub for cache eviction
- Database NOTIFY for state changes
- Or enforce: always query DB for critical operations (revoke, budget check)

**v1 Workaround for Multi-Node Deployments:**
For deployments without Redis/DB pub/sub, use one of these approaches:
1. **Disable L1 cache:** Set L1 cache TTL to 0 in multi-node mode
2. **Short TTL:** Use 5-second TTL (balance between performance and consistency)
3. **Always query DB for critical operations:** Revoke and budget checks always hit primary DB
```

### Lock Ordering Invariant

```
ALL transactions that lock both `teams` and `api_keys` rows MUST acquire
the team lock BEFORE the key lock to prevent deadlocks:

1. SELECT ... FROM teams WHERE ... FOR UPDATE
2. SELECT ... FROM api_keys WHERE ... FOR UPDATE

This order must be followed consistently across ALL code paths.
Any code that violates this order risks deadlock under concurrent load.
```

### Team Budget Consistency

```
team_current_spend is a DERIVED CACHE (like key current_spend).

Invariant: team_current_spend >= SUM(child_key_current_spend)
For deterministic replay: team_spend = SUM(child_spend_ledger.cost_amount)
```

### Ledger-Based Architecture

RFC-0903 introduces a **ledger-based architecture** for quota accounting. This simplifies the system and makes it more deterministic.

**Core principle:**

```
spend_ledger is the authoritative economic record.
All balances MUST be derived from the ledger.
```

**Why this matters:**

Financial systems avoid complex counter synchronization by doing one thing:

```
append ledger entries
derive balances
```

This eliminates:
- Multiple counters to maintain
- Reconciliation complexity
- Possible drift between counters and events
- Complex transaction ordering

**Simplified data model:**

```
api_keys
  budget_limit        -- only this is stored
  -- current_spend REMOVED (derived from ledger)

spend_ledger          -- authoritative record
```

**Ledger schema:**

```sql
-- Spend ledger - THE authoritative economic record
-- All balances are derived from this table
CREATE TABLE spend_ledger (
    event_id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL,
    key_id TEXT NOT NULL,
    UNIQUE(key_id, request_id),
    team_id TEXT,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_amount BIGINT NOT NULL,
    pricing_hash BYTEA NOT NULL,
    token_source TEXT NOT NULL,
    tokenizer_version TEXT,
    provider_usage_json TEXT,  -- Raw provider usage for audit
    timestamp INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX idx_spend_ledger_key_id ON spend_ledger(key_id);
CREATE INDEX idx_spend_ledger_team_id ON spend_ledger(team_id);
CREATE INDEX idx_spend_ledger_timestamp ON spend_ledger(timestamp);
```

**Deterministic quota enforcement with row locking:**

CRITICAL: To prevent race conditions in multi-router deployments, quota enforcement MUST use `FOR UPDATE` row locking.

```rust
/// Check and record spend with atomic row locking
/// CRITICAL: Uses FOR UPDATE to prevent race conditions in multi-router deployments
pub fn record_spend(
    db: &Database,
    key_id: &Uuid,
    event: &SpendEvent,
) -> Result<(), KeyError> {
    let tx = db.transaction()?;

    // 1. Lock the key row to prevent concurrent budget modifications
    // FOR UPDATE ensures only one transaction can modify this key at a time
    let budget: i64 = tx.query_row(
        "SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE",
        params![key_id.to_string()],
        |row| row.get(0),
    )?;

    // 2. Compute current spend from ledger (not a counter)
    let current: i64 = tx.query_row(
        "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
        params![key_id.to_string()],
        |row| row.get(0),
    )?;

    // 3. Check budget with locked row
    if current + event.cost_amount as i64 > budget {
        return Err(KeyError::BudgetExceeded { current: current as u64, limit: budget as u64 });
    }

    // 4. Insert into ledger (idempotent with ON CONFLICT)
    tx.execute(
        "INSERT INTO spend_ledger (
            event_id, request_id, key_id, team_id, provider, model,
            input_tokens, output_tokens, cost_amount, pricing_hash,
            token_source, tokenizer_version, provider_usage_json, timestamp
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT(key_id, request_id) DO NOTHING",
        params![
            event.event_id.to_string(),
            event.request_id,
            event.key_id.to_string(),
            event.team_id,
            event.provider,
            event.model,
            event.input_tokens,
            event.output_tokens,
            event.cost_amount as i64,
            &event.pricing_hash,
            match event.token_source {
                TokenSource::ProviderUsage => "provider_usage",
                TokenSource::CanonicalTokenizer => "canonical_tokenizer",
            },
            event.tokenizer_version,
            event.provider_usage_json,
            event.timestamp,
        ],
    )?;

    tx.commit()?;
    Ok(())
}
```

**Team budget enforcement with row locking:**

```rust
/// Record spend with team budget enforcement
/// CRITICAL: Locks both key and team rows to prevent overspend
///
/// # Lock Ordering Invariant
/// ALL transactions that lock both `teams` and `api_keys` rows MUST acquire
/// the team lock BEFORE the key lock to prevent deadlocks:
///   1. SELECT ... FROM teams WHERE ... FOR UPDATE
///   2. SELECT ... FROM api_keys WHERE ... FOR UPDATE
///
/// This order must be followed consistently across all code paths.
pub fn record_spend_with_team(
    db: &Database,
    key_id: &Uuid,
    team_id: &str,
    event: &SpendEvent,
) -> Result<(), KeyError> {
    let tx = db.transaction()?;

    // 1. Lock team row FIRST (prevents team overspend)
    let team_budget: i64 = tx.query_row(
        "SELECT budget_limit FROM teams WHERE team_id = $1 FOR UPDATE",
        params![team_id],
        |row| row.get(0),
    )?;

    // 2. Lock key row
    let key_budget: i64 = tx.query_row(
        "SELECT budget_limit FROM api_keys WHERE key_id = $1 FOR UPDATE",
        params![key_id.to_string()],
        |row| row.get(0),
    )?;

    // 3. Compute current spends from ledger
    let key_current: i64 = tx.query_row(
        "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1",
        params![key_id.to_string()],
        |row| row.get(0),
    )?;

    let team_current: i64 = tx.query_row(
        "SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE team_id = $1",
        params![team_id],
        |row| row.get(0),
    )?;

    // 4. Check both budgets
    if key_current + event.cost_amount as i64 > key_budget {
        return Err(KeyError::BudgetExceeded { current: key_current as u64, limit: key_budget as u64 });
    }

    if team_current + event.cost_amount as i64 > team_budget {
        return Err(KeyError::TeamBudgetExceeded { current: team_current as u64, limit: team_budget as u64 });
    }

    // 5. Insert into ledger
    tx.execute(
        "INSERT INTO spend_ledger (
            event_id, request_id, key_id, team_id, provider, model,
            input_tokens, output_tokens, cost_amount, pricing_hash,
            token_source, tokenizer_version, provider_usage_json, timestamp
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT(key_id, request_id) DO NOTHING",
        params![
            event.event_id.to_string(),
            event.request_id,
            event.key_id.to_string(),
            event.team_id,
            event.provider,
            event.model,
            event.input_tokens,
            event.output_tokens,
            event.cost_amount as i64,
            &event.pricing_hash,
            match event.token_source {
                TokenSource::ProviderUsage => "provider_usage",
                TokenSource::CanonicalTokenizer => "canonical_tokenizer",
            },
            event.tokenizer_version,
            event.provider_usage_json,
            event.timestamp,
        ],
    )?;

    tx.commit()?;
    Ok(())
}
```

**Why FOR UPDATE is critical:**

Without row locking, two routers can race:

```
budget = 1000, current = 990, cost = 20

Router A: SELECT SUM = 990
Router B: SELECT SUM = 990

Both pass check
Both insert
Result: 1030 (overspend)
```

With `FOR UPDATE`:

```
Router A: SELECT ... FOR UPDATE (locks row)
Router B: SELECT ... FOR UPDATE (waits for lock)

Router A: inserts, commits
Router B: gets lock, SELECT SUM = 1010
Router B: fails check (1010 > 1000)

Result: correct (no overspend)
```

**Deterministic replay:**

```
1. SELECT * FROM spend_ledger ORDER BY created_at, event_id
2. Recompute balances
3. Verify equality with any cached balances
```

Note: Ordering by `created_at` (chronology) then `event_id` (tiebreaker) ensures deterministic replay. `event_id` alone is not chronological (SHA256 ordering is arbitrary).

This is extremely useful for:
- Audits
- Dispute resolution
- Fraud detection
- Blockchain anchoring later

**Derived balance views (optional optimization):**

For performance, materialized views can cache balances (marked as DERIVED CACHE):

```sql
-- DERIVED CACHE - MAY be rebuilt from spend_ledger
CREATE TABLE key_balances (
    key_id TEXT PRIMARY KEY,
    current_spend BIGINT NOT NULL DEFAULT 0,
    last_updated INTEGER NOT NULL
);

-- DERIVED CACHE - MAY be rebuilt from spend_ledger
CREATE TABLE team_balances (
    team_id TEXT PRIMARY KEY,
    current_spend BIGINT NOT NULL DEFAULT 0,
    last_updated INTEGER NOT NULL
);
```

But these are explicitly marked as:

```
DERIVED CACHE - MAY be rebuilt from ledger
```

**Benefits of ledger architecture:**

1. Single source of truth
2. Deterministic replay is trivial
3. No counter drift
4. Easy audit and verification
5. Enables cryptographic proofs later
6. Simpler transaction logic
7. Natural fit for blockchain anchoring

**Long-term enablement:**

Ledger architecture enables powerful features for CipherOcto:

```
- Merkle root of spend ledger
- Cryptographic spend proofs
- Economic verification
- Verifiable AI infrastructure
```

This is the foundation for RFC-0909 Deterministic Quota Accounting.

### Scalability Considerations

The `spend_ledger` SUM query runs inside every write transaction under FOR UPDATE lock:

```sql
SELECT COALESCE(SUM(cost_amount), 0) FROM spend_ledger WHERE key_id = $1
```

**At scale, this becomes a bottleneck** as the ledger grows to millions of rows per key.

**Deferred strategies for high-volume deployments:**

1. **Periodic reconciliation job:** Background worker that periodically computes `key_balances` and `team_balances` derived tables from the ledger. Read path uses cached balance, write path still uses FOR UPDATE + SUM until cache refresh.

2. **Incremental materialized views:** Database-native materialized view that auto-updates (PostgreSQL, CockroachDB). Read from materialized view, write to ledger.

3. **Sharding by key_id:** Partition the ledger by key_id hash. Each router instance owns a key partition.

**For v1/v2, the SUM approach is acceptable** for moderate traffic (< 100 req/s per key). At higher volumes, implement one of the above strategies.

## Future Work / Not Yet Implemented

The following features are documented but NOT yet implemented:

- **Grace period revocation:** The `rotation_worker` does not yet revoke keys after grace period. Currently only rotates on expiry.
- **Failed authentication lockout:** The `failed_attempts`, `last_failed_at`, and `locked` fields on `ApiKey` are defined but not used in `validate_key`.
- **Soft budget pre-check:** The optional pre-flight budget check is implemented but callers must explicitly invoke it.

## Future Work

- F1: OAuth2/JWT authentication
- F2: API key rotation automation
- F3: Key usage analytics dashboard
- F4: Team-based access control (RBAC)
- F5: Access group management (LiteLLM compatible)
- F6: Model-level budget controls

## Rationale

Virtual API keys are essential for:

1. **Multi-tenancy** - Multiple users on single router
2. **Budget control** - Prevent runaway spend
3. **Rate limiting** - Prevent abuse
4. **Enterprise ready** - Teams with shared budgets
5. **LiteLLM migration** - Match key management features
6. **Embedded persistence** - No external database dependency (stoolap)

### Router Statelessness Principle

```
Routers MUST treat API key validation and budget enforcement
as stateless operations.

All economic state transitions MUST occur via atomic database
transactions recorded as spend events.

This ensures routers are replaceable and enables:
- Horizontal scaling
- Deterministic replay
- Multi-router consensus (future)
```

## Operational Clarifications

### Cache Invalidation Rules

All mutations of `api_keys` table MUST invalidate the L1 cache entry:

- Key revocation (`revoke_key()`)
- Key rotation (`rotate_key()`)
- Key metadata update (`update_key()`)
- Key budget update
- TTL expiration (handled automatically by cache TTL)

### Prefix Usage

The `key_prefix` field is **informational only** and MUST NOT be used for key lookup.
All lookups MUST use the full `key_hash` (HMAC-SHA256) for security.

### Route Authorization

All route authorization checks MUST operate on a **normalized path**:

- Path must be canonicalized before authorization to prevent bypasses like `/v1/chat/../management`
- Use `normalize_path()` function before permission checks

### Token Bucket Behavior

Token bucket enforces **maximum burst capacity**:

- `tokens = min(capacity, tokens + refill_amount)`
- Bucket never exceeds `capacity` tokens
- Allows smooth burst handling within limits

**Rate Limiting Determinism Disclaimer:**

```
Rate limiting is NOT deterministic across router nodes.
Rate limiting MUST NOT influence accounting logic.

Two routers may disagree on whether to allow/deny a request,
but BOTH must record identical spend events if the request executes.
This ensures accounting determinism even when routing behavior diverges.
```

### Distributed Rate Limiting (Future)

v1 scope is single-node. For horizontal scaling, future versions will support:

- Redis-backed rate limiter
- Sharded token buckets
- Distributed atomic budget updates

### Key Format Specification

API keys MUST follow this format:

```
sk-qr-[64 hex characters]
```

- Prefix: `sk-qr-` (quota-router variant of LiteLLM's `sk-`)
- Body: 256-bit entropy encoded as 64 hex characters
- Total length: 70 characters

### Rate Limiter Memory Management

The DashMap-based rate limiter stores buckets per key. **MUST evict buckets idle > 10 minutes** to prevent memory growth in high-churn environments.

Implementation pattern:

```rust
pub struct TokenBucket {
    // ... existing fields ...
    last_access_ms: u64,
}

impl TokenBucket {
    fn is_stale(&self, now_ms: u64, max_idle_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_access_ms) > max_idle_ms
    }
}
```

- Keys that are revoked MUST call `rate_limiter.invalidate(key_id)`
- Implement periodic cleanup task to evict stale buckets

### Failed Authentication Tracking

To detect brute-force attacks, track failed attempts:

```rust
/// Enhanced key tracking for security
pub struct ApiKey {
    // ... existing fields ...
    pub failed_attempts: u32,      // Count of failed auth attempts
    pub last_failed_at: Option<i64>, // Timestamp of last failure
    pub locked: bool,               // Account lockout flag
}

const MAX_FAILED_ATTEMPTS: u32 = 5;
const LOCKOUT_DURATION_SECS: i64 = 300; // 5 minutes
```

### Maximum Key Limits

Prevent abuse by enforcing team key limits (application layer enforcement):

```rust
/// Maximum keys per team
const MAX_KEYS_PER_TEAM: u32 = 100;

/// Check key limit before insertion
pub fn check_team_key_limit(db: &Database, team_id: &Uuid) -> Result<(), KeyError> {
    let count: i64 = db.query(
        "SELECT COUNT(*) as cnt FROM api_keys WHERE team_id = $1",
        params![team_id.to_string()],
    )?.next()?.get("cnt")?;

    if count >= MAX_KEYS_PER_TEAM as i64 {
        return Err(KeyError::TeamKeyLimitExceeded {
            team_id: *team_id,
            current: count as u32,
            limit: MAX_KEYS_PER_TEAM,
        });
    }
    Ok(())
}
```

## References

- LiteLLM key management: `litellm/proxy/_types.py`, `litellm/proxy/management_endpoints/key_management_endpoints.py`
- stoolap embedded DB: `src/api/database.rs`

---

## Changelog

- **v28 (2026-03-13):** Hygiene fixes
  - Fixed version mismatch: header says v27, footer says v26 → now both say v28
  - Added DB CHECK constraint for MAX_KEYS_PER_TEAM (100) via trigger
  - Fixed i64 casts in check_budget_soft_limit: use try_into().unwrap_or() for explicit overflow handling
  - Clarified GenerateKeyResponse.expires: now returns expiration if rotation_interval_days is set

- **v27 (2026-03-13):** Ledger consistency fixes (continued)
  - Fixed to_db_str() implementation (was referencing undefined `event` variable)
  - Moved check_budget_soft_limit outside validate_key as standalone function with doc comments
  - Added estimated_max_cost guidance (use per-model ceiling or budget_limit)
  - Added dedicated Lock Ordering Invariant section (not just in function comment)
  - Added Scalability Considerations section (deferred SUM optimization strategies)

- **v26 (2026-03-13):** Ledger consistency fixes (continued)
  - Fixed rotated_from direction: new key now carries rotated_from=old_key_id (was backwards)
  - Added TokenSource::to_hash_str() and to_db_str() methods to prevent string inconsistency
  - Updated compute_event_id and all record_spend functions to use new TokenSource methods
  - Implemented check_budget_soft_limit in validate_key (optional pre-check for UX)
  - Added last_access field to TokenBucket for cleanup tracking
  - Implemented cleanup_stale_buckets with idle eviction and max_size cap
  - Updated rotate_key to set rotated_from on new key (not old)

- **v25 (2026-03-13):** Ledger consistency fixes (continued)
  - Added lock ordering invariant comment to record_spend_with_team (team before key)
  - Added provider_usage_json to both DDL blocks and INSERT statements
  - Added comment explaining TokenSource hash strings vs DB strings are intentionally different
  - Fixed revoke_key to update DB BEFORE invalidating cache (prevents crash edge case)
  - Added note about missing team_id in deprecated record_spend_with_event
  - Fixed normalize_path to return Result<String, ()> and reject double-encoding properly
  - Fixed check_route_permission to handle Err from normalize_path (reject suspicious paths)
  - Fixed rotate_key to accept cache and invalidate old key immediately (no TTL grace)
  - Updated rotation_worker to pass cache to rotate_key

- **v24 (2026-03-13):** Ledger consistency fixes (continued)
  - Completed record_spend_with_team INSERT placeholder with full params
  - Fixed SQL column name: amount -> cost_amount in deprecated INSERT
  - Added missing opening code fence to record_spend_with_team_atomic
  - Fixed generate_key to call Utc::now() once and reuse timestamp
  - Added provider_usage_json field to SpendEvent struct
  - Removed duplicate TokenSource enum definition
  - Fixed revoke_key to parameterize revoked_by instead of hardcoding 'admin'

- **v23 (2026-03-13):** Ledger consistency fixes (continued)
  - Removed dangling code fragment after record_spend_with_team (was claimed fixed in v22 but still present)
  - Fixed "Safer transaction ordering" block to use ledger-based approach
  - Updated "Spend Event Recording" section header and intro to reflect ledger as canonical
  - Fixed inconsistent UNIQUE constraints - aligned all to `UNIQUE(key_id, request_id)`
  - Fixed SpendEvent.event_id doc comment to match actual hash composition
  - Fixed event.amount -> event.cost_amount in deprecated record_spend_with_event
  - Fixed FOR UPDATE on spend_ledger - now correctly locks api_keys row
  - Added struct fields to TeamKeyLimitExceeded enum variant
  - Fixed rotate_key to call Utc::now() once and reuse timestamp

- **v22 (2026-03-13):** Ledger consistency fixes
  - Removed `current_spend` from DDL (lines 96-127) - derived from ledger
  - Added DERIVED CACHE comments to ApiKey and Team structs
  - Fixed KeyError enum: changed i64 to u64, added TeamKeyLimitExceeded variant
  - Removed dead code after `unimplemented!()` in cleanup_stale_buckets
  - Added UPDATE for old key in rotate_key (set rotated_from reference)
  - Deprecated counter-based functions: record_spend_atomic, record_spend_with_team_atomic, record_spend_with_event
  - Fixed contradictory atomic SQL pattern to use ledger-only approach
  - Removed dangling code fragment after record_spend_with_team
  - Fixed validate_key comment to reference correct function (record_spend vs record_spend_atomic)
  - Added complete SpendEvent struct definition with TokenSource enum
  - Added DERIVED CACHE comments to key_balances and team_balances tables

**Draft Date:** 2026-03-13
**Version:** v28
**Related Use Case:** Enhanced Quota Router Gateway
**Related Research:** LiteLLM Analysis and Quota Router Comparison
