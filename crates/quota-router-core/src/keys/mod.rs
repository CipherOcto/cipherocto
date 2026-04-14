pub mod errors;
pub mod models;

pub use errors::KeyError;
pub use models::{
    ApiKey, CreateTeamRequest, GenerateKeyRequest, GenerateKeyResponse, KeySpend, KeyType,
    KeyUpdates, RevokeKeyRequest, SpendEvent, Team, TokenSource, UpdateTeamRequest,
};

use hmac_sha256::HMAC;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Default server secret for key hashing (fallback)
const DEFAULT_SERVER_SECRET: &[u8] = b"quota-router-server-secret-change-in-production";

/// Environment variable name for server secret
const SERVER_SECRET_ENV: &str = "QUOTA_ROUTER_SECRET";

/// Cached server secret (initialized once)
static SERVER_SECRET: OnceLock<Vec<u8>> = OnceLock::new();

/// Get the server secret, using env var if set
fn get_server_secret() -> &'static [u8] {
    SERVER_SECRET.get_or_init(|| {
        std::env::var(SERVER_SECRET_ENV)
            .map(|s| s.into_bytes())
            .unwrap_or_else(|_| DEFAULT_SERVER_SECRET.to_vec())
    })
}

/// Compute HMAC-SHA256 hash of an API key
pub fn compute_key_hash(key: &str) -> [u8; 32] {
    HMAC::mac(key.as_bytes(), get_server_secret())
}

/// Generate a cryptographically secure API key string
/// Format: sk-qr-{64 hex characters}
pub fn generate_key_string() -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.random()).collect();

    let hex_string = bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    format!("sk-qr-{}", hex_string)
}

/// Compute deterministic event_id for a spend event.
#[allow(clippy::too_many_arguments)]
///
/// This function is deterministic: the same inputs always produce the same event_id.
/// This enables cross-router idempotency — the same request processed by different
/// routers produces the same event_id, so duplicate requests are safely ignored.
///
/// # Arguments
/// * `request_id` - Unique request identifier (from the API gateway)
/// * `key_id` - The API key used for this request
/// * `provider` - LLM provider name (e.g., "openai")
/// * `model` - Model name (e.g., "gpt-4o")
/// * `input_tokens` - Number of input tokens
/// * `output_tokens` - Number of output tokens
/// * `pricing_hash` - 32-byte pricing hash (from pricing table lookup)
/// * `token_source` - How tokens were counted
pub fn compute_event_id(
    request_id: &str,
    key_id: &uuid::Uuid,
    provider: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    pricing_hash: &[u8; 32],
    token_source: TokenSource,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(request_id.as_bytes());
    hasher.update(key_id.as_bytes());
    hasher.update(provider.as_bytes());
    hasher.update(model.as_bytes());
    hasher.update(input_tokens.to_be_bytes());
    hasher.update(output_tokens.to_be_bytes());
    hasher.update(pricing_hash);
    hasher.update(token_source.to_hash_str().as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Maximum keys per team (per RFC-0903 §Maximum Key Limits)
const MAX_KEYS_PER_TEAM: u32 = 100;

/// Check team key limit before creating a new key.
///
/// Returns Ok(()) if under the limit, Err(KeyError::TeamKeyLimitExceeded) otherwise.
pub fn check_team_key_limit(key_count: u32) -> Result<(), KeyError> {
    if key_count >= MAX_KEYS_PER_TEAM {
        return Err(KeyError::TeamKeyLimitExceeded {
            current: key_count,
            limit: MAX_KEYS_PER_TEAM,
        });
    }
    Ok(())
}

/// Generate a new key_id using UUIDv7-like format
/// Format: {timestamp_hex}-{random_hex}
pub fn generate_key_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let mut rng = rand::rng();
    let random_bytes: Vec<u8> = (0..8).map(|_| rng.random()).collect();

    format!(
        "{:016x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        now,
        random_bytes[0],
        random_bytes[1],
        random_bytes[2],
        random_bytes[3],
        random_bytes[4],
        random_bytes[5],
        random_bytes[6],
        random_bytes[7]
    )
}

/// Validate an API key (check expiry, revoked status)
pub fn validate_key(key: &ApiKey) -> Result<(), KeyError> {
    // Check if revoked
    if key.revoked {
        return Err(KeyError::Revoked(
            key.revocation_reason.clone().unwrap_or_default(),
        ));
    }

    // Check if expired
    if let Some(expires_at) = key.expires_at {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        if expires_at < now {
            return Err(KeyError::Expired(expires_at));
        }
    }

    Ok(())
}

/// Decode percent-encoded path THEN normalize to prevent bypass attacks.
///
/// e.g., /v1/chat/%2e%2e/admin -> /v1/chat/../admin -> /v1/admin
///
/// SECURITY: Reject double-encoded paths to prevent path traversal bypass.
/// e.g., %252e%252e -> %2e%2e -> ..
///
/// Returns Err(()) on security violation, Ok(normalized_path) on success.
#[allow(clippy::result_unit_err)]
pub fn normalize_path(path: &str) -> Result<String, ()> {
    use percent_encoding::percent_decode_str;

    // First check for double-encoded sequences - reject them
    // %252E = encoded '%' + '2E', %252F = encoded '%' + '2F'
    // Also reject %25. and %25/ which are partial double encodings
    let upper = path.to_uppercase();
    if upper.contains("%252E")
        || upper.contains("%252F")
        || upper.contains("%25.")
        || upper.contains("%25/")
    {
        // Double encoding detected - reject the request
        return Err(());
    }

    // Decode percent encoding
    let decoded = percent_decode_str(path).decode_utf8_lossy().into_owned();

    let mut segments: Vec<&str> = Vec::new();
    for segment in decoded.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                segments.pop();
            }
            _ => segments.push(segment),
        }
    }

    let normalized = format!("/{}", segments.join("/"));
    Ok(normalized)
}

/// Route permission mapping with slash enforcement per RFC-0903.
///
/// Checks if a key has permission to access a given route.
/// Normalizes the path BEFORE checking to prevent bypass attacks.
pub fn check_route_permission(key: &ApiKey, route: &str) -> bool {
    // CRITICAL: Normalize path BEFORE checking to prevent bypass attacks
    // SECURITY: Reject double-encoded paths (normalize_path returns Err on attack)
    let Ok(normalized) = normalize_path(route) else {
        return false; // Reject suspicious paths
    };

    // 1. Check explicit allowed_routes first (JSON array in database)
    // Format: ["\\/v1\\/chat","\\/v1\\/embeddings"]
    if let Some(ref allowed_routes_json) = key.allowed_routes {
        if let Ok(routes) = serde_json::from_str::<Vec<String>>(allowed_routes_json) {
            if !routes.is_empty() {
                return routes.iter().any(|r| {
                    // Enforce trailing slash or exact match
                    let with_slash = format!("{}/", r);
                    normalized.starts_with(&with_slash) || normalized == *r
                });
            }
        }
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
        KeyType::ReadOnly => normalized.starts_with("/models/") || normalized.starts_with("/info"),
        KeyType::Default => true, // Allow all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_key_string_length() {
        let key = generate_key_string();
        assert_eq!(key.len(), 70); // "sk-qr-" (6) + 64 hex chars
    }

    #[test]
    fn test_generate_key_string_prefix() {
        let key = generate_key_string();
        assert!(key.starts_with("sk-qr-"));
    }

    #[test]
    fn test_compute_key_hash() {
        let key = "sk-qr-1234567890abcdef";
        let hash = compute_key_hash(key);
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_generate_key_id() {
        let key_id = generate_key_id();
        // Should be in format: 16 hex chars - 16 hex chars
        assert!(key_id.contains('-'));
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    use crate::keys::models::ApiKey;

    #[test]
    fn test_validate_key_expired() {
        let expired_key = ApiKey {
            key_id: "test".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: Some(1), // Expired in past
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: crate::keys::KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        let result = validate_key(&expired_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_key_revoked() {
        let revoked_key = ApiKey {
            key_id: "test".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: None,
            revoked: true,
            revoked_at: None,
            revoked_by: Some("admin".to_string()),
            revocation_reason: Some("Policy violation".to_string()),
            key_type: crate::keys::KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        let result = validate_key(&revoked_key);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_key_valid() {
        let valid_key = ApiKey {
            key_id: "test".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: None, // Never expires
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: crate::keys::KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        let result = validate_key(&valid_key);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;

    // =============================================================================
    // normalize_path tests
    // =============================================================================

    #[test]
    fn test_normalize_path_simple() {
        assert_eq!(normalize_path("/v1/chat").unwrap(), "/v1/chat");
        assert_eq!(
            normalize_path("/v1/chat/completions").unwrap(),
            "/v1/chat/completions"
        );
    }

    #[test]
    fn test_normalize_path_current_dir_removed() {
        // Single dot is removed
        assert_eq!(normalize_path("/v1/./chat").unwrap(), "/v1/chat");
        assert_eq!(
            normalize_path("/v1/chat/./completions").unwrap(),
            "/v1/chat/completions"
        );
    }

    #[test]
    fn test_normalize_path_parent_dir_pop() {
        // Double dot pops parent segment
        assert_eq!(
            normalize_path("/v1/chat/../management").unwrap(),
            "/v1/management"
        );
        assert_eq!(normalize_path("/v1/../v2/chat").unwrap(), "/v2/chat");
    }

    #[test]
    fn test_normalize_path_root_handling() {
        assert_eq!(normalize_path("/v1///chat").unwrap(), "/v1/chat");
        assert_eq!(normalize_path("///v1/chat").unwrap(), "/v1/chat");
    }

    #[test]
    fn test_normalize_path_percent_decoding() {
        // Percent-encoded forward slash should be decoded
        assert_eq!(
            normalize_path("/v1/chat%2Fcompletions").unwrap(),
            "/v1/chat/completions"
        );
        // Percent-encoded dot should be decoded
        assert_eq!(
            normalize_path("/v1/.well-known").unwrap(),
            "/v1/.well-known"
        );
    }

    #[test]
    fn test_normalize_path_rejects_double_encoding() {
        // Double encoding - should be rejected
        assert!(normalize_path("/v1/chat/%252e%252e/management").is_err());
        assert!(normalize_path("/v1/chat/%252Fadmin").is_err());
    }

    #[test]
    fn test_normalize_path_rejects_partial_double_encoding() {
        // Partial double encoding (%25. or %25/)
        assert!(normalize_path("/v1/chat/%25./admin").is_err());
        assert!(normalize_path("/v1/chat/%25/admin").is_err());
    }

    #[test]
    fn test_normalize_path_bypass_attempt() {
        // Classic path traversal bypass
        assert!(normalize_path("/v1/chat/../management").is_ok()); // normalize_path doesn't reject, just normalizes
                                                                   // But after normalization, the path becomes /v1/management
        let result = normalize_path("/v1/chat/../management").unwrap();
        assert_eq!(result, "/v1/management");
    }

    // =============================================================================
    // check_route_permission tests
    // =============================================================================

    fn make_llm_api_key() -> ApiKey {
        ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::LlmApi,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        }
    }

    fn make_management_key() -> ApiKey {
        ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::Management,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        }
    }

    fn make_readonly_key() -> ApiKey {
        ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::ReadOnly,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        }
    }

    fn make_default_key() -> ApiKey {
        ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::Default,
            allowed_routes: None,
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        }
    }

    #[test]
    fn test_check_route_permission_llm_api_valid() {
        let key = make_llm_api_key();
        assert!(check_route_permission(&key, "/v1/chat"));
        assert!(check_route_permission(&key, "/v1/chat/completions"));
        assert!(check_route_permission(&key, "/v1/completions"));
        assert!(check_route_permission(&key, "/v1/embeddings"));
        assert!(check_route_permission(&key, "/v1/embeddings"));
    }

    #[test]
    fn test_check_route_permission_llm_api_rejects_management() {
        let key = make_llm_api_key();
        assert!(!check_route_permission(&key, "/key/list"));
        assert!(!check_route_permission(&key, "/team/list"));
    }

    #[test]
    fn test_check_route_permission_management_valid() {
        let key = make_management_key();
        assert!(check_route_permission(&key, "/key/list"));
        assert!(check_route_permission(&key, "/key/generate"));
        assert!(check_route_permission(&key, "/team/list"));
        assert!(check_route_permission(&key, "/team/create"));
        assert!(check_route_permission(&key, "/user/info"));
    }

    #[test]
    fn test_check_route_permission_management_rejects_llm() {
        let key = make_management_key();
        assert!(!check_route_permission(&key, "/v1/chat"));
        assert!(!check_route_permission(&key, "/v1/completions"));
    }

    #[test]
    fn test_check_route_permission_readonly_valid() {
        let key = make_readonly_key();
        assert!(check_route_permission(&key, "/models/list"));
        assert!(check_route_permission(&key, "/info"));
    }

    #[test]
    fn test_check_route_permission_default_allows_all() {
        let key = make_default_key();
        assert!(check_route_permission(&key, "/v1/chat"));
        assert!(check_route_permission(&key, "/key/list"));
        assert!(check_route_permission(&key, "/anything"));
    }

    #[test]
    fn test_check_route_permission_rejects_double_encoded_bypass() {
        let key = make_llm_api_key();
        // This should be rejected at normalization level
        assert!(!check_route_permission(
            &key,
            "/v1/chat/%252e%252e/management"
        ));
        assert!(!check_route_permission(&key, "/v1/chat/%252Fadmin"));
    }

    #[test]
    fn test_check_route_permission_with_explicit_allowed_routes() {
        let key = ApiKey {
            key_id: "test-key".to_string(),
            key_hash: vec![],
            key_prefix: "sk-qr-tes".to_string(),
            team_id: None,
            budget_limit: 1000,
            rpm_limit: None,
            tpm_limit: None,
            created_at: 0,
            expires_at: None,
            revoked: false,
            revoked_at: None,
            revoked_by: None,
            revocation_reason: None,
            key_type: KeyType::Default,
            allowed_routes: Some(r#"["\/v1\/chat","\/v1\/embeddings"]"#.to_string()),
            auto_rotate: false,
            rotation_interval_days: None,
            description: None,
            metadata: None,
        };

        assert!(check_route_permission(&key, "/v1/chat"));
        assert!(check_route_permission(&key, "/v1/chat/completions"));
        assert!(check_route_permission(&key, "/v1/embeddings"));
        assert!(!check_route_permission(&key, "/v1/completions"));
    }
}
