pub mod errors;
pub mod models;

pub use errors::KeyError;
pub use models::{
    ApiKey, CreateTeamRequest, GenerateKeyRequest, GenerateKeyResponse, KeySpend, KeyType,
    KeyUpdates, PricingModel, RevokeKeyRequest, SpendEvent, Team, TokenSource, UpdateTeamRequest,
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
    // RFC 4122 hyphenated lowercase — MUST use to_string(), NOT as_bytes()
    hasher.update(key_id.to_string().as_bytes());
    hasher.update(provider.as_bytes());
    hasher.update(model.as_bytes());
    // Little-endian for cross-router determinism per RFC-0909
    hasher.update(input_tokens.to_le_bytes());
    hasher.update(output_tokens.to_le_bytes());
    hasher.update(pricing_hash);
    hasher.update(token_source.to_hash_str().as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Validate a request_id string.
///
/// Returns Ok(()) if 1 ≤ len ≤ 1024 bytes, Err(KeyError::InvalidFormat) otherwise.
/// Must be called in process_response before compute_event_id.
#[inline]
pub fn validate_request_id(request_id: &str) -> Result<(), KeyError> {
    let len = request_id.len();
    if (1..=1024).contains(&len) {
        Ok(())
    } else {
        Err(KeyError::InvalidFormat)
    }
}

/// Compute total cost in micro-units for a request using integer-only arithmetic.
///
/// Uses the formula: cost = (input_tokens * prompt_cost_per_1k / 1000)
///                        + (output_tokens * completion_cost_per_1k / 1000)
///
/// TOKEN_SCALE = 1000 (micro-units per token). Truncation error is bounded
/// at <2 micro-units per event (truncation occurs when cost < 0.5 micro-units
/// per step, which is effectively free).
///
/// This function is NOT a method — it is a standalone function per RFC-0909.
///
/// # Arguments
/// * `pricing` - PricingModel containing per-1k token pricing in micro-units
/// * `input_tokens` - Number of input tokens
/// * `output_tokens` - Number of output tokens
///
/// Returns total cost in micro-units.
#[inline]
pub fn compute_cost(pricing: &PricingModel, input_tokens: u32, output_tokens: u32) -> u64 {
    // Two-step integer computation matching RFC-0909 pseudocode structure.
    // Division is integer division — truncates toward zero.
    let prompt_cost = (input_tokens as u64)
        .saturating_mul(pricing.prompt_cost_per_1k)
        .saturating_div(1000);
    let completion_cost = (output_tokens as u64)
        .saturating_mul(pricing.completion_cost_per_1k)
        .saturating_div(1000);
    // saturating_add: single-request overflow is impossible (>1.8×10¹⁹ tokens required)
    // This differs from record_spend budget accumulation which uses checked arithmetic.
    prompt_cost.saturating_add(completion_cost)
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

#[cfg(test)]
mod compute_event_id_tests {
    use super::*;

    /// Decode a 64-char hex string to [u8; 32]
    fn hex_to_32_bytes(hex_str: &str) -> [u8; 32] {
        let bytes = hex::decode(hex_str).expect("valid 64-char hex");
        bytes.try_into().expect("must be 32 bytes")
    }

    #[test]
    fn test_compute_event_id_tv1() {
        // TV1: base inputs
        let request_id = "req-001";
        let key_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let provider = "openai";
        let model = "gpt-4";
        let input_tokens = 100u32;
        let output_tokens = 50u32;
        let pricing_hash = hex_to_32_bytes("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff");
        let token_source = TokenSource::ProviderUsage;

        let event_id = compute_event_id(
            request_id, &key_id, provider, model, input_tokens, output_tokens, &pricing_hash, token_source,
        );

        assert_eq!(
            event_id, "8d22792346a0417bb928da0c16f2af5330640678f365d16bc392d400c2aa4ab2",
            "TV1 failed: expected deterministic event_id for base inputs"
        );
    }

    #[test]
    fn test_compute_event_id_tv2() {
        // TV2: same as TV1 but request_id="req-002" and token_source=CanonicalTokenizer
        let request_id = "req-002";
        let key_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let provider = "openai";
        let model = "gpt-4";
        let input_tokens = 100u32;
        let output_tokens = 50u32;
        let pricing_hash = hex_to_32_bytes("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff");
        let token_source = TokenSource::CanonicalTokenizer;

        let event_id = compute_event_id(
            request_id, &key_id, provider, model, input_tokens, output_tokens, &pricing_hash, token_source,
        );

        assert_eq!(
            event_id, "0f26450e1734034b9bc6f999b61586c671dd8249002524dd740a94c51ded3f36",
            "TV2 failed: request_id and token_source change must produce different event_id"
        );
    }

    #[test]
    fn test_compute_event_id_tv3() {
        // TV3: same as TV1 but key_id changes to 660e8400-e29b-41d4-a716-446655440001
        let request_id = "req-001";
        let key_id = uuid::Uuid::parse_str("660e8400-e29b-41d4-a716-446655440001").unwrap();
        let provider = "openai";
        let model = "gpt-4";
        let input_tokens = 100u32;
        let output_tokens = 50u32;
        let pricing_hash = hex_to_32_bytes("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff");
        let token_source = TokenSource::ProviderUsage;

        let event_id = compute_event_id(
            request_id, &key_id, provider, model, input_tokens, output_tokens, &pricing_hash, token_source,
        );

        assert_eq!(
            event_id, "a3e31fbaa4b3bf6fe9d5c1eeb59055cfe4a3389358fc0e38c8820e2c2e6912ed",
            "TV3 failed: only key_id change must produce different event_id"
        );
    }

    #[test]
    fn test_compute_event_id_tv4() {
        // TV4: same as TV1 but pricing_hash changed to SHA256(b"pricing-table-v2")
        // hex of SHA256(b"pricing-table-v2"): 8b48fe37e84565f99285690a835a881fe2d580ec63775aa5f9465ba38a5a2f60
        let request_id = "req-001";
        let key_id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let provider = "openai";
        let model = "gpt-4";
        let input_tokens = 100u32;
        let output_tokens = 50u32;
        let pricing_hash = hex_to_32_bytes("8b48fe37e84565f99285690a835a881fe2d580ec63775aa5f9465ba38a5a2f60");
        let token_source = TokenSource::ProviderUsage;

        let event_id = compute_event_id(
            request_id, &key_id, provider, model, input_tokens, output_tokens, &pricing_hash, token_source,
        );

        assert_eq!(
            event_id, "06a6eb1c68f8a75287d0ac45b1ede9f00cd770f106c505685c299cf3b593726c",
            "TV4 failed: pricing_hash change must produce different event_id"
        );
    }

    #[test]
    fn test_validate_request_id_valid() {
        assert!(validate_request_id("req-001").is_ok());
        assert!(validate_request_id("a").is_ok());
        assert!(validate_request_id(&"x".repeat(1024)).is_ok());
    }

    #[test]
    fn test_validate_request_id_empty_rejected() {
        assert!(validate_request_id("").is_err());
    }

    #[test]
    fn test_validate_request_id_too_long_rejected() {
        assert!(validate_request_id(&"x".repeat(1025)).is_err());
    }

    #[test]
    fn test_validate_request_id_boundary_1024_ok() {
        assert!(validate_request_id(&"x".repeat(1024)).is_ok());
    }

    #[test]
    fn test_validate_request_id_boundary_1025_rejected() {
        assert!(validate_request_id(&"x".repeat(1025)).is_err());
    }
}

#[cfg(test)]
mod compute_cost_tests {
    use super::*;

    /// Test vector from RFC-0909 §compute_cost:
    /// prompt_cost_per_1k = 30_000, completion_cost_per_1k = 60_000
    /// input_tokens = 100, output_tokens = 50
    /// Expected: (100 * 30_000 / 1000) + (50 * 60_000 / 1000) = 3000 + 3000 = 6000
    #[test]
    fn test_compute_cost_tv1() {
        let pricing = PricingModel {
            model_name: "test".into(),
            prompt_cost_per_1k: 30_000,
            completion_cost_per_1k: 60_000,
        };
        assert_eq!(compute_cost(&pricing, 100, 50), 6000);
    }

    #[test]
    fn test_compute_cost_zero_tokens() {
        let pricing = PricingModel {
            model_name: "test".into(),
            prompt_cost_per_1k: 30_000,
            completion_cost_per_1k: 60_000,
        };
        assert_eq!(compute_cost(&pricing, 0, 0), 0);
    }

    #[test]
    fn test_compute_cost_input_only() {
        let pricing = PricingModel {
            model_name: "test".into(),
            prompt_cost_per_1k: 30_000,
            completion_cost_per_1k: 60_000,
        };
        // 1000 tokens * 30_000 / 1000 = 30_000
        assert_eq!(compute_cost(&pricing, 1000, 0), 30_000);
    }

    #[test]
    fn test_compute_cost_output_only() {
        let pricing = PricingModel {
            model_name: "test".into(),
            prompt_cost_per_1k: 30_000,
            completion_cost_per_1k: 60_000,
        };
        // 1000 tokens * 60_000 / 1000 = 60_000
        assert_eq!(compute_cost(&pricing, 0, 1000), 60_000);
    }

    #[test]
    fn test_compute_cost_large_tokens() {
        let pricing = PricingModel {
            model_name: "test".into(),
            prompt_cost_per_1k: 30_000,
            completion_cost_per_1k: 60_000,
        };
        // 1M input * 30_000 / 1000 = 30_000_000; 1M output * 60_000 / 1000 = 60_000_000
        // total = 90_000_000 micro-units
        assert_eq!(compute_cost(&pricing, 1_000_000, 1_000_000), 90_000_000);
    }
}
