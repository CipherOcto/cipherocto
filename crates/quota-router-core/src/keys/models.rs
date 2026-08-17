use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors produced by `SpendEvent` boundary validation.
///
/// `CostOverflow` enforces the S4 Round 2 / S6c Round 1 invariant
/// that `cost_amount: u64` MUST be representable in the i64 column
/// used by `spend_ledger` and the budget-gate arithmetic. Failure
/// is closed — silently narrowing via `as i64` would let
/// `cost_amount > i64::MAX` wrap to negative, defeating the gate
/// (mission 0862-c7 adjacent-module wrap mitigation).
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SpendEventError {
    #[error("cost_amount overflow: cost={cost} exceeds i64::MAX ({max})")]
    CostOverflow { cost: u64, max: i64 },
}

/// Convert a `u64` cost to its `i64` representation, failing closed
/// when the value exceeds `i64::MAX`.
///
/// Used at every boundary where `cost_amount` is narrowed to `i64`
/// for `spend_ledger` column storage or budget-gate arithmetic.
/// Pattern: `let cost_i64 = cost_u64_to_i64(cost)?;` (mission
/// 0862-c7). Pair with `From<SpendEventError> for KeyError` to
/// propagate via `?`.
pub fn cost_u64_to_i64(cost: u64) -> Result<i64, SpendEventError> {
    if cost > i64::MAX as u64 {
        return Err(SpendEventError::CostOverflow {
            cost,
            max: i64::MAX,
        });
    }
    Ok(cost as i64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    #[default]
    Default,
    LlmApi,
    Management,
    ReadOnly,
    Sso,
}

impl std::fmt::Display for KeyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyType::LlmApi => write!(f, "llm_api"),
            KeyType::Management => write!(f, "management"),
            KeyType::ReadOnly => write!(f, "read_only"),
            KeyType::Sso => write!(f, "sso"),
            KeyType::Default => write!(f, "default"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub key_id: String,
    pub key_hash: Vec<u8>,
    pub key_prefix: String,
    pub team_id: Option<uuid::Uuid>,
    pub budget_limit: i64,
    pub rpm_limit: Option<i32>,
    pub tpm_limit: Option<i32>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub revoked: bool,
    pub revoked_at: Option<i64>,
    pub revoked_by: Option<String>,
    pub revocation_reason: Option<String>,
    pub key_type: KeyType,
    pub allowed_routes: Option<String>,
    pub auto_rotate: bool,
    pub rotation_interval_days: Option<i32>,
    pub description: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeyUpdates {
    pub budget_limit: Option<i64>,
    pub rpm_limit: Option<i32>,
    pub tpm_limit: Option<i32>,
    pub expires_at: Option<i64>,
    pub revoked: Option<bool>,
    pub revoked_by: Option<String>,
    pub revocation_reason: Option<String>,
    pub key_type: Option<KeyType>,
    pub description: Option<String>,
    pub metadata: Option<String>,
}

/// Team - group of API keys with shared budget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub team_id: String,
    pub name: String,
    pub budget_limit: i64,
    pub created_at: i64,
}

/// Tracks spending for a key within a time window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeySpend {
    pub key_id: String,
    pub total_spend: i64,  // in cents/millicents
    pub window_start: i64, // timestamp when window started
    pub last_updated: i64,
}

/// Token source for spend events — determines how tokens were counted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TokenSource {
    #[default]
    ProviderUsage,
    CanonicalTokenizer,
}

impl TokenSource {
    /// String used in event_id hash input (compact, for SHA256)
    /// DIFFERENT from to_db_str() - hash strings are compact for efficient hashing
    pub fn to_hash_str(&self) -> &'static str {
        match self {
            TokenSource::ProviderUsage => "provider",
            TokenSource::CanonicalTokenizer => "tokenizer",
        }
    }

    /// String used in database storage and CHECK constraint validation
    pub fn to_db_str(&self) -> &'static str {
        match self {
            TokenSource::ProviderUsage => "provider_usage",
            TokenSource::CanonicalTokenizer => "canonical_tokenizer",
        }
    }

    /// Parse from database string
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "provider_usage" => Some(TokenSource::ProviderUsage),
            "canonical_tokenizer" => Some(TokenSource::CanonicalTokenizer),
            _ => None,
        }
    }
}

/// A single spend event recorded in the ledger.
///
/// This is the canonical record of a billing event. event_id is deterministic
/// based on the inputs — the same request on any router produces the same event_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendEvent {
    pub event_id: String,
    pub request_id: String,
    pub key_id: uuid::Uuid,
    pub team_id: Option<uuid::Uuid>,
    pub provider: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_amount: u64,
    pub pricing_hash: [u8; 32], // 32 bytes — fixed-size array, stored as BLOB in DB
    pub token_source: TokenSource,
    pub tokenizer_version: Option<String>,
    pub provider_usage_json: Option<String>,
    pub timestamp: i64,
}

impl SpendEvent {
    /// Returns `cost_amount` as `i64`, failing closed when the value
    /// exceeds `i64::MAX`. Per mission 0862-c7 + S4 Round 2 / S6c
    /// Round 1 signed-underflow mitigation. Callers that previously
    /// used `event.cost_amount as i64` MUST switch to this getter
    /// (or the free function `cost_u64_to_i64`) to avoid silent
    /// wrap.
    pub fn cost_amount_i64(&self) -> Result<i64, SpendEventError> {
        cost_u64_to_i64(self.cost_amount)
    }
}

/// Key generation request (LiteLLM compatible) per RFC-0903 §GenerateKeyRequest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateKeyRequest {
    /// Optional existing key (for regeneration)
    pub key: Option<String>,
    /// Budget limit in deterministic cost units
    pub budget_limit: u64,
    /// Rate limits
    pub rpm_limit: Option<u32>,
    pub tpm_limit: Option<u32>,
    /// Key type (default: Default)
    #[serde(default)]
    pub key_type: KeyType,
    /// Auto-rotation
    pub auto_rotate: Option<bool>,
    /// Rotation interval in days
    pub rotation_interval_days: Option<u32>,
    /// Team ID
    pub team_id: Option<uuid::Uuid>,
    /// Metadata
    pub metadata: Option<serde_json::Value>,
    pub description: Option<String>,
}

/// Key generation response per RFC-0903 §GenerateKeyResponse
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateKeyResponse {
    /// The actual API key (sk-qr-...)
    pub key: String,
    /// Public key identifier
    pub key_id: String,
    /// Expiration timestamp (epoch seconds)
    pub expires: Option<i64>,
    /// Team ID if associated
    pub team_id: Option<uuid::Uuid>,
    /// Key type
    pub key_type: KeyType,
    /// Created timestamp (epoch seconds)
    pub created_at: i64,
}

/// Team creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTeamRequest {
    pub team_id: String,
    pub name: String,
    pub budget_limit: i64,
}

/// Team update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTeamRequest {
    pub name: Option<String>,
    pub budget_limit: Option<i64>,
}

/// Revoke key request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeKeyRequest {
    pub revoked_by: Option<String>,
    pub reason: Option<String>,
}

/// Pricing model for cost computation per RFC-0909 §PricingModel.
///
/// Contains per-token micro-unit pricing. TOKEN_SCALE = 1000 (micro-units per token).
/// Truncation error is bounded at <2 micro-units per event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingModel {
    pub model_name: String,
    /// Prompt cost per 1000 tokens, in micro-units.
    pub prompt_cost_per_1k: u64,
    /// Completion cost per 1000 tokens, in micro-units.
    pub completion_cost_per_1k: u64,
}

/// A node in a Merkle tree built from SpendEvents.
///
/// Leaf nodes contain event data. Internal nodes are hashes of their children.
/// Used for cryptographic proof generation per RFC-0909 §build_merkle_tree.
#[derive(Debug, Clone)]
pub struct MerkleNode {
    /// The SHA256 hash of this node's content.
    pub hash: [u8; 32],
    /// Left child (None for leaf nodes).
    pub left: Option<Box<MerkleNode>>,
    /// Right child (None for leaf nodes).
    pub right: Option<Box<MerkleNode>>,
}

/// One-hour grace window (in seconds) during which a predecessor API
/// key remains valid after a rotation event. Per mission 0957-b AC-7
/// + RFC-0903 §RotationEvents. Operators roll out the fresh key
///   while in-flight requests using the old key still complete; the
///   predecessor expires after `KEY_ROTATION_GRACE_SECS`.
pub const KEY_ROTATION_GRACE_SECS: i64 = 3_600;

/// A `KeyRotationEvent` records a single rotation of an API key,
/// capturing the predecessor key hash, the rotation epoch, and the
/// predecessor-expires-at timestamp. Mission 0957-b AC-7 closes when
/// the validator emits events of this shape on each rotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationEvent {
    /// Key id being rotated.
    pub key_id: String,
    /// Unix seconds at which the rotation occurred.
    pub rotated_at_unix: i64,
    /// Hash of the predecessor key (so the validator can match
    /// in-flight requests against the predecessor key during the
    /// grace window).
    pub predecessor_key_hash: Vec<u8>,
    /// Hash of the new key (the post-rotation active key).
    pub successor_key_hash: Vec<u8>,
    /// Unix seconds at which the predecessor key stops being
    /// honoured. Equal to `rotated_at_unix + KEY_ROTATION_GRACE_SECS`.
    pub predecessor_expires_at_unix: i64,
}
