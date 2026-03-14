use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    LlmApi,
    Management,
    ReadOnly,
    Default,
}

impl Default for KeyType {
    fn default() -> Self {
        KeyType::Default
    }
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
