use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    #[default]
    Default,
    LlmApi,
    Management,
    ReadOnly,
}

impl std::fmt::Display for KeyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyType::LlmApi => write!(f, "llm_api"),
            KeyType::Management => write!(f, "management"),
            KeyType::ReadOnly => write!(f, "read_only"),
            KeyType::Default => write!(f, "default"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub key_id: String,
    pub key_hash: Vec<u8>,
    pub key_prefix: String,
    pub team_id: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// String used in event_id hash input (different from DB storage strings)
    pub fn to_hash_str(&self) -> &'static str {
        match self {
            TokenSource::ProviderUsage => "provider_usage",
            TokenSource::CanonicalTokenizer => "canonical_tokenizer",
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
    pub team_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_amount: u64,
    pub pricing_hash: Vec<u8>, // 32 bytes — stored as BLOB in DB, Vec<u8> in code
    pub token_source: TokenSource,
    pub tokenizer_version: Option<String>,
    pub provider_usage_json: Option<String>,
    pub timestamp: i64,
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
    pub team_id: Option<String>,
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
    pub team_id: Option<String>,
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
