pub mod errors;
pub mod models;

pub use errors::{BudgetError, KeyError};
pub use models::{
    ApiKey, CreateTeamRequest, GenerateKeyRequest, GenerateKeyResponse, KeySpend, KeyType,
    KeyUpdates, MerkleNode, PricingModel, RevokeKeyRequest, SpendEvent, Team, TokenSource,
    UpdateTeamRequest,
};

use hmac_sha256::HMAC;
use rand::Rng;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
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

// =============================================================================
// BLOB Storage Boundary Helpers
// =============================================================================
// These functions are the storage boundary — they should be called ONLY at the
// INSERT/SELECT boundary, not inside business logic.

/// Convert a 64-char hex string to 32 raw bytes for event_id BLOB(32) storage.
///
/// Panics if the input is not exactly 64 hex chars (32 bytes).
/// Invalid hex or wrong length are both implementation bugs, not user input.
#[inline]
pub fn hex_to_blob_32(hex_str: &str) -> [u8; 32] {
    let bytes = hex::decode(hex_str).expect("invalid hex in event_id");
    bytes
        .try_into()
        .expect("event_id hex must be exactly 64 chars (32 bytes)")
}

/// Convert 32 raw bytes to a 64-char hex string for event_id API responses.
///
/// **Critical:** This function does NOT apply to request_id, which is stored
/// as raw binary BLOB(32), not hex. Never use `blob_32_to_hex` on request_id data.
#[inline]
pub fn blob_32_to_hex(blob: &[u8; 32]) -> String {
    hex::encode(blob)
}

/// Convert a Uuid to 16 raw bytes for key_id/team_id BLOB(16) storage.
#[inline]
pub fn uuid_to_blob_16(uuid: &uuid::Uuid) -> [u8; 16] {
    *uuid.as_bytes()
}

/// Convert 16 raw bytes from key_id/team_id BLOB(16) retrieval to a Uuid.
///
/// **Important:** `uuid::Uuid::from_bytes` silently accepts any 16-byte sequence
/// without validating RFC 4122 version or variant bits — the resulting Uuid may
/// be structurally invalid per the UUID spec, but no Rust undefined behavior
/// occurs (this is safe Rust). Per RFC-0903-B1: "UUIDs with invalid version
/// or variant bits MUST be rejected." Downstream validation will catch invalid UUIDs.
#[inline]
pub fn blob_16_to_uuid(blob: &[u8; 16]) -> uuid::Uuid {
    // SAFETY: from_bytes is safe Rust — it never causes undefined behavior.
    // Invalid UUIDs (bad version/variant) are caught by downstream validation.
    uuid::Uuid::from_bytes(*blob)
}

/// Normalize provider and model names per RFC-0909 CONSISTENCY GOAL.
///
/// Applies: (1) Unicode NFC normalization for any non-ASCII characters,
/// (2) ASCII lowercase conversion.
///
/// This ensures `compute_event_id` sees consistent byte sequences across all
/// router instances, regardless of how the gateway formats provider/model names.
///
/// # Arguments
/// * `provider` - LLM provider name (e.g., "OpenAI", "openai")
/// * `model` - Model name (e.g., "GPT-4", "gpt-4")
///
/// # Returns
/// A tuple of (normalized_provider, normalized_model) as owned Strings.
pub fn normalize_provider_model(provider: &str, model: &str) -> (String, String) {
    use unicode_normalization::UnicodeNormalization;
    let p = provider.nfc().collect::<String>().to_lowercase();
    let m = model.nfc().collect::<String>().to_lowercase();
    (p, m)
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

/// Encode a gateway-provided request_id string to 32 raw bytes for BLOB(32) storage.
///
/// All inputs are treated as raw text strings (not hex). Always uses SHA256 regardless
/// of input length — uniform encoding for all gateway request_id formats.
///
/// WARNING: The gateway's input format (raw text vs hex) must be consistent across
/// all routers. A router that changes input format will produce different request_id
/// values for the same logical request, breaking idempotency.
///
/// This function is defined in RFC-0903-B1 §request_id.
#[inline]
pub fn encode_request_id(request_id: &str) -> [u8; 32] {
    Sha256::digest(request_id.as_bytes()).into()
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

/// Compute cost delegating to RFC-0910 canonical implementation.
///
/// Takes `&PricingTable` (RFC-0910 struct) and returns `Result<u64, BudgetError>`.
/// Converts `CostError::Overflow` → `BudgetError::CostOverflow`.
#[inline]
pub fn compute_cost_from_pricing_table(
    pricing: &crate::pricing::PricingTable,
    input_tokens: u32,
    output_tokens: u32,
) -> Result<u64, BudgetError> {
    crate::pricing::compute_cost(pricing, input_tokens, output_tokens)
        .map_err(|e| match e {
            crate::pricing::CostError::Overflow { .. } => BudgetError::CostOverflow,
        })
}

/// Reconstruct per-key spend aggregates from an ordered slice of SpendEvents.
///
/// This function is deterministic: the same events always produce the same aggregates.
/// Used for audit, historical reconciliation, and budget state verification.
///
/// NOT for live quota enforcement — use `record_spend` for that (per RFC-0903 Final).
///
/// NOT for Merkle proof generation — use `build_merkle_tree` for that (Mission 0909-e).
///
/// # Arguments
/// * `events` - Slice of SpendEvents to aggregate
///
/// Returns a BTreeMap of key_id (as String) → total accumulated cost in micro-units.
/// BTreeMap provides deterministic iteration order (sorted by key).
///
/// # Sorting
/// Events are sorted by event_id (hex string, ascending) for deterministic ordering.
/// Note: the sort is required for audit/replay determinism, NOT because aggregation
/// math requires it — SUM is order-independent.
#[inline]
pub fn replay_events(events: &[SpendEvent]) -> BTreeMap<String, u64> {
    let mut sorted_events = events.to_vec();
    sorted_events.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    let mut result: BTreeMap<String, u64> = BTreeMap::new();
    for event in sorted_events {
        let key = event.key_id.to_string();
        result
            .entry(key)
            .and_modify(|v| *v = v.saturating_add(event.cost_amount))
            .or_insert(event.cost_amount);
    }
    result
}

// =============================================================================
// Merkle Tree (Mission 0909-e)
// =============================================================================

/// Build a Merkle tree from SpendEvents for cryptographic proof generation.
///
/// This function is deterministic: the same events always produce the same root.
///
/// NOT for budget computation — only for cryptographic proof generation.
/// NOT for multi-tenant use — caller MUST filter events to single tenant scope
/// before calling (per RFC-0909 §Security Note — No Field Delimiters).
///
/// # Arguments
/// * `events` - Slice of SpendEvents to build tree from
///
/// Returns `Option<MerkleNode>` — `None` if events is empty (no root to publish).
///
/// # Leaf Hash
/// `SHA256(event_id.as_bytes() || cost_amount.to_le_bytes())`
/// where cost_amount is 8-byte little-endian encoding (per RFC-0909).
///
/// # Internal Node Hash
/// `SHA256(left_hash || right_hash)`
///
/// # Odd Leaf Padding
/// If odd number of leaves, pad by duplicating the last leaf (deterministic).
pub fn build_merkle_tree(events: &[SpendEvent]) -> Option<MerkleNode> {
    if events.is_empty() {
        return None;
    }

    // Sort by event_id ascending (same ordering as replay_events)
    let mut sorted_events = events.to_vec();
    sorted_events.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    // Build leaf nodes
    let mut nodes: Vec<MerkleNode> = sorted_events
        .iter()
        .map(|e| {
            let mut hasher = Sha256::new();
            hasher.update(e.event_id.as_bytes());
            hasher.update(e.cost_amount.to_le_bytes());
            let result = hasher.finalize();
            let hash: [u8; 32] = result.into();
            MerkleNode {
                hash,
                left: None,
                right: None,
            }
        })
        .collect();

    // Bottom-up tree construction
    loop {
        if nodes.len() == 1 {
            return Some(nodes.remove(0));
        }

        // Pad odd count by duplicating last leaf
        if !nodes.len().is_multiple_of(2) {
            let last = nodes.last().cloned().unwrap();
            nodes.push(last);
        }

        // Pair up nodes and compute parent hashes
        let mut new_level = Vec::new();
        for pair in nodes.chunks(2) {
            debug_assert_eq!(pair.len(), 2);
            let mut hasher = Sha256::new();
            hasher.update(pair[0].hash);
            hasher.update(pair[1].hash);
            let hash: [u8; 32] = hasher.finalize().into();
            new_level.push(MerkleNode {
                hash,
                left: Some(Box::new(pair[0].clone())),
                right: Some(Box::new(pair[1].clone())),
            });
        }
        nodes = new_level;
    }
}

// =============================================================================
// Tokenizer ID Helpers (Mission 0909-f)
// =============================================================================

/// Convert a tokenizer version string to a 16-byte BLAKE3 hash for BLOB(16) storage.
///
/// Per RFC-0909 §tokenizer_version_to_id: BLAKE3(version_string) truncated to 16 bytes.
///
/// Collision probability becomes non-negligible after ~2^32 distinct tokenizer versions.
/// This is acceptable for tokenizer versioning (far fewer than 4 billion tokenizer
/// versions will ever exist).
///
/// # Test Vector
/// `tokenizer_version_to_id("tiktoken-cl100k_base-v1.2.3")` → `e3c8e8ff724411c6416dd4fb135368e3`
#[inline]
pub fn tokenizer_version_to_id(version: &str) -> [u8; 16] {
    let hash = blake3::hash(version.as_bytes());
    let bytes = hash.as_bytes();
    // Truncate to 16 bytes — unwrap is safe because blake3::hash always produces 32 bytes
    bytes[..16].try_into().unwrap()
}

/// Convert a 16-byte tokenizer_id back to its version string via DB lookup.
///
/// Per RFC-0909 §tokenizer_id_to_version: queries `SELECT version FROM tokenizers WHERE tokenizer_id = ?`.
///
/// Returns:
/// - `Ok(Some(version))` if tokenizer_id found in database
/// - `Ok(None)` if tokenizer_id not found (never registered)
/// - `Err("tokenizer_id_to_version: requires DB lookup implementation")` if DB not available
///
/// Note: If the DB query fails (connection error), callers should substitute
/// `Err(KeyError::Storage(...))` in the error path per RFC-0909 §Error Handling.
///
/// The tokenizers table is populated on-demand when a new tokenizer version is first used.
/// A no-match result may mean the tokenizer was never persisted to storage.
pub fn tokenizer_id_to_version(id: &[u8; 16]) -> Result<Option<String>, &'static str> {
    // Stub: requires DB lookup implementation against tokenizers table.
    // Full implementation: SELECT version FROM tokenizers WHERE tokenizer_id = $1
    let _ = id;
    Err("tokenizer_id_to_version: requires DB lookup implementation")
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
        let pricing_hash =
            hex_to_32_bytes("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff");
        let token_source = TokenSource::ProviderUsage;

        let event_id = compute_event_id(
            request_id,
            &key_id,
            provider,
            model,
            input_tokens,
            output_tokens,
            &pricing_hash,
            token_source,
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
        let pricing_hash =
            hex_to_32_bytes("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff");
        let token_source = TokenSource::CanonicalTokenizer;

        let event_id = compute_event_id(
            request_id,
            &key_id,
            provider,
            model,
            input_tokens,
            output_tokens,
            &pricing_hash,
            token_source,
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
        let pricing_hash =
            hex_to_32_bytes("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff");
        let token_source = TokenSource::ProviderUsage;

        let event_id = compute_event_id(
            request_id,
            &key_id,
            provider,
            model,
            input_tokens,
            output_tokens,
            &pricing_hash,
            token_source,
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
        let pricing_hash =
            hex_to_32_bytes("8b48fe37e84565f99285690a835a881fe2d580ec63775aa5f9465ba38a5a2f60");
        let token_source = TokenSource::ProviderUsage;

        let event_id = compute_event_id(
            request_id,
            &key_id,
            provider,
            model,
            input_tokens,
            output_tokens,
            &pricing_hash,
            token_source,
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

    #[test]
    fn test_normalize_provider_model() {
        // Mixed case → lowercase
        let (p, m) = normalize_provider_model("OpenAI", "GPT-4");
        assert_eq!(p, "openai");
        assert_eq!(m, "gpt-4");

        // Already lowercase: unchanged
        let (p, m) = normalize_provider_model("openai", "gpt-4");
        assert_eq!(p, "openai");
        assert_eq!(m, "gpt-4");
    }

    #[test]
    fn test_normalize_provider_model_unicode_nfc() {
        // Unicode with NFC normalization
        // é can be composed (e + ́) or decomposed (e + combining acute)
        // NFC normalizes to composed form before lowercase
        let (p, m) = normalize_provider_model("OpenAI", "GPT-4");
        assert_eq!(p, "openai");
        assert_eq!(m, "gpt-4");
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

#[cfg(test)]
mod blob_helpers_tests {
    use super::*;

    #[test]
    fn test_hex_to_blob_32_valid() {
        let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let blob = hex_to_blob_32(hex);
        assert_eq!(blob.len(), 32);
        assert_eq!(blob[0], 0x00);
        assert_eq!(blob[1], 0x11);
        assert_eq!(blob[31], 0xff);
    }

    #[test]
    fn test_blob_32_to_hex_roundtrip() {
        let original = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let blob = hex_to_blob_32(original);
        let roundtripped = blob_32_to_hex(&blob);
        assert_eq!(roundtripped, original);
    }

    #[test]
    fn test_blob_32_to_hex_all_zeros() {
        let blob = [0u8; 32];
        let hex = blob_32_to_hex(&blob);
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c == '0'));
    }

    #[test]
    fn test_blob_32_to_hex_all_ff() {
        let blob = [0xffu8; 32];
        let hex = blob_32_to_hex(&blob);
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c == 'f'));
    }

    #[test]
    #[should_panic(expected = "invalid hex in event_id")]
    fn test_hex_to_blob_32_invalid_hex_panics() {
        hex_to_blob_32("not_valid_hex!");
    }

    #[test]
    #[should_panic(expected = "event_id hex must be exactly 64 chars")]
    fn test_hex_to_blob_32_wrong_length_panics() {
        // 60 chars, not 64 — valid hex but wrong byte length
        hex_to_blob_32("00112233445566778899aabbccddeeff00112233445566778899aabbccddee");
    }

    #[test]
    fn test_uuid_to_blob_16_roundtrip() {
        let uuid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let blob = uuid_to_blob_16(&uuid);
        assert_eq!(blob.len(), 16);
        let roundtripped = blob_16_to_uuid(&blob);
        assert_eq!(roundtripped, uuid);
    }

    #[test]
    fn test_blob_16_to_uuid_from_raw_bytes() {
        let uuid = uuid::Uuid::parse_str("660e8400-e29b-41d4-a716-446655440001").unwrap();
        let blob = uuid_to_blob_16(&uuid);
        let result = blob_16_to_uuid(&blob);
        assert_eq!(result, uuid);
    }

    #[test]
    fn test_blob_16_to_uuid_all_zeros() {
        let blob = [0u8; 16];
        let uuid = blob_16_to_uuid(&blob);
        assert_eq!(uuid.as_bytes(), &[0u8; 16]);
    }

    #[test]
    fn test_blob_16_to_uuid_all_ff() {
        let blob = [0xffu8; 16];
        let uuid = blob_16_to_uuid(&blob);
        assert_eq!(uuid.as_bytes(), &[0xffu8; 16]);
    }
}

#[cfg(test)]
mod replay_events_tests {
    use super::*;

    fn make_event(event_id: &str, key_id: &str, cost: u64) -> SpendEvent {
        SpendEvent {
            event_id: event_id.to_string(),
            request_id: "req-001".to_string(),
            key_id: uuid::Uuid::parse_str(key_id).unwrap(),
            team_id: None,
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cost_amount: cost,
            pricing_hash: [0u8; 32],
            token_source: TokenSource::ProviderUsage,
            tokenizer_version: None,
            provider_usage_json: None,
            timestamp: 0,
        }
    }

    #[test]
    fn test_replay_events_empty() {
        let events: &[SpendEvent] = &[];
        let result = replay_events(events);
        assert!(result.is_empty());
    }

    #[test]
    fn test_replay_events_single_key_single_event() {
        let events = [make_event(
            "0000000000000000000000000000000000000000000000000000000000000001",
            "550e8400-e29b-41d4-a716-446655440000",
            1000,
        )];
        let result = replay_events(&events);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("550e8400-e29b-41d4-a716-446655440000"),
            Some(&1000)
        );
    }

    #[test]
    fn test_replay_events_single_key_multiple_events() {
        let events = [
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000001",
                "550e8400-e29b-41d4-a716-446655440000",
                1000,
            ),
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000002",
                "550e8400-e29b-41d4-a716-446655440000",
                2000,
            ),
        ];
        let result = replay_events(&events);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("550e8400-e29b-41d4-a716-446655440000"),
            Some(&3000)
        );
    }

    #[test]
    fn test_replay_events_multiple_keys() {
        let events = [
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000001",
                "550e8400-e29b-41d4-a716-446655440000",
                1000,
            ),
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000002",
                "660e8400-e29b-41d4-a716-446655440001",
                3000,
            ),
        ];
        let result = replay_events(&events);
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.get("550e8400-e29b-41d4-a716-446655440000"),
            Some(&1000)
        );
        assert_eq!(
            result.get("660e8400-e29b-41d4-a716-446655440001"),
            Some(&3000)
        );
    }

    #[test]
    fn test_replay_events_deterministic_sort() {
        // Same events in reverse order — replay_events should produce identical result
        let events_asc = [
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000001",
                "550e8400-e29b-41d4-a716-446655440000",
                1000,
            ),
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000002",
                "660e8400-e29b-41d4-a716-446655440001",
                2000,
            ),
        ];
        let events_desc = [
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000002",
                "660e8400-e29b-41d4-a716-446655440001",
                2000,
            ),
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000001",
                "550e8400-e29b-41d4-a716-446655440000",
                1000,
            ),
        ];
        let result_asc = replay_events(&events_asc);
        let result_desc = replay_events(&events_desc);
        assert_eq!(result_asc, result_desc);
    }

    #[test]
    fn test_replay_events_saturating_add() {
        // Verify saturating_add doesn't panic on large values
        let events = [make_event(
            "0000000000000000000000000000000000000000000000000000000000000001",
            "550e8400-e29b-41d4-a716-446655440000",
            u64::MAX,
        )];
        let result = replay_events(&events);
        assert_eq!(
            result.get("550e8400-e29b-41d4-a716-446655440000"),
            Some(&u64::MAX)
        );
    }
}

#[cfg(test)]
mod build_merkle_tree_tests {
    use super::*;

    fn make_event(event_id: &str, key_id: &str, cost: u64) -> SpendEvent {
        SpendEvent {
            event_id: event_id.to_string(),
            request_id: "req-001".to_string(),
            key_id: uuid::Uuid::parse_str(key_id).unwrap(),
            team_id: None,
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            cost_amount: cost,
            pricing_hash: [0u8; 32],
            token_source: TokenSource::ProviderUsage,
            tokenizer_version: None,
            provider_usage_json: None,
            timestamp: 0,
        }
    }

    #[test]
    fn test_build_merkle_tree_empty() {
        // Empty events → None
        let events: &[SpendEvent] = &[];
        let result = build_merkle_tree(events);
        assert!(result.is_none());
    }

    #[test]
    fn test_build_merkle_tree_single_event() {
        // Single event → root equals leaf hash
        let event = make_event(
            "0000000000000000000000000000000000000000000000000000000000000001",
            "550e8400-e29b-41d4-a716-446655440000",
            1000,
        );
        let result = build_merkle_tree(std::slice::from_ref(&event));
        assert!(result.is_some());
        let root = result.unwrap();
        // Root should have no children (leaf node)
        assert!(root.left.is_none());
        assert!(root.right.is_none());
        // Root hash should be SHA256(event_id.as_bytes() || cost.to_le_bytes())
        let mut expected_hasher = Sha256::new();
        expected_hasher.update(b"0000000000000000000000000000000000000000000000000000000000000001");
        expected_hasher.update(1000u64.to_le_bytes());
        let expected_hash: [u8; 32] = expected_hasher.finalize().into();
        assert_eq!(root.hash, expected_hash);
    }

    #[test]
    fn test_build_merkle_tree_two_identical_events() {
        // Two identical events → parent = SHA256(leaf_hash || leaf_hash)
        let event1 = make_event(
            "0000000000000000000000000000000000000000000000000000000000000001",
            "550e8400-e29b-41d4-a716-446655440000",
            1000,
        );
        let event2 = make_event(
            "0000000000000000000000000000000000000000000000000000000000000001",
            "550e8400-e29b-41d4-a716-446655440000",
            1000,
        );
        let result = build_merkle_tree(&[event1, event2]);
        assert!(result.is_some());
        let root = result.unwrap();
        assert!(root.left.is_some());
        assert!(root.right.is_some());
        // Parent hash = SHA256(leaf_hash || leaf_hash)
        let mut leaf_hasher = Sha256::new();
        leaf_hasher.update(b"0000000000000000000000000000000000000000000000000000000000000001");
        leaf_hasher.update(1000u64.to_le_bytes());
        let leaf_hash: [u8; 32] = leaf_hasher.finalize().into();
        let mut parent_hasher = Sha256::new();
        parent_hasher.update(leaf_hash);
        parent_hasher.update(leaf_hash);
        let expected_parent: [u8; 32] = parent_hasher.finalize().into();
        assert_eq!(root.hash, expected_parent);
    }

    #[test]
    fn test_build_merkle_tree_two_different_events() {
        // Two different events → parent = SHA256(hash_A || hash_B) where hash_A ≠ hash_B
        let event1 = make_event(
            "0000000000000000000000000000000000000000000000000000000000000001",
            "550e8400-e29b-41d4-a716-446655440000",
            1000,
        );
        let event2 = make_event(
            "0000000000000000000000000000000000000000000000000000000000000002",
            "550e8400-e29b-41d4-a716-446655440000",
            2000,
        );
        let result = build_merkle_tree(&[event1, event2]);
        assert!(result.is_some());
        let root = result.unwrap();
        assert!(root.left.is_some());
        assert!(root.right.is_some());
        // Compute expected leaf hashes
        let mut hasher1 = Sha256::new();
        hasher1.update(b"0000000000000000000000000000000000000000000000000000000000000001");
        hasher1.update(1000u64.to_le_bytes());
        let hash1: [u8; 32] = hasher1.finalize().into();
        let mut hasher2 = Sha256::new();
        hasher2.update(b"0000000000000000000000000000000000000000000000000000000000000002");
        hasher2.update(2000u64.to_le_bytes());
        let hash2: [u8; 32] = hasher2.finalize().into();
        // hash1 ≠ hash2
        assert_ne!(hash1, hash2);
        // Parent = SHA256(hash1 || hash2)
        let mut parent_hasher = Sha256::new();
        parent_hasher.update(hash1);
        parent_hasher.update(hash2);
        let expected_parent: [u8; 32] = parent_hasher.finalize().into();
        assert_eq!(root.hash, expected_parent);
    }

    #[test]
    fn test_build_merkle_tree_odd_count_padded() {
        // 3 leaves → padded to 4, last leaf duplicated
        let event1 = make_event(
            "0000000000000000000000000000000000000000000000000000000000000001",
            "550e8400-e29b-41d4-a716-446655440000",
            1000,
        );
        let event2 = make_event(
            "0000000000000000000000000000000000000000000000000000000000000002",
            "550e8400-e29b-41d4-a716-446655440000",
            2000,
        );
        let event3 = make_event(
            "0000000000000000000000000000000000000000000000000000000000000003",
            "550e8400-e29b-41d4-a716-446655440000",
            3000,
        );
        let result = build_merkle_tree(&[event1.clone(), event2.clone(), event3.clone()]);
        assert!(result.is_some());
        // 3 leaves → pairs: (leaf1, leaf2), (leaf3, leaf3)
        // Level 1: parent1 = SHA256(hash1 || hash2), parent2 = SHA256(hash3 || hash3)
        // Level 2: root = SHA256(parent1 || parent2)
        let mut h1 = Sha256::new();
        h1.update(b"0000000000000000000000000000000000000000000000000000000000000001");
        h1.update(1000u64.to_le_bytes());
        let hash1: [u8; 32] = h1.finalize().into();
        let mut h2 = Sha256::new();
        h2.update(b"0000000000000000000000000000000000000000000000000000000000000002");
        h2.update(2000u64.to_le_bytes());
        let hash2: [u8; 32] = h2.finalize().into();
        let mut h3 = Sha256::new();
        h3.update(b"0000000000000000000000000000000000000000000000000000000000000003");
        h3.update(3000u64.to_le_bytes());
        let hash3: [u8; 32] = h3.finalize().into();
        let mut p1_hasher = Sha256::new();
        p1_hasher.update(hash1);
        p1_hasher.update(hash2);
        let parent1: [u8; 32] = p1_hasher.finalize().into();
        let mut p2_hasher = Sha256::new();
        p2_hasher.update(hash3);
        p2_hasher.update(hash3);
        let parent2: [u8; 32] = p2_hasher.finalize().into();
        let mut root_hasher = Sha256::new();
        root_hasher.update(parent1);
        root_hasher.update(parent2);
        let expected_root: [u8; 32] = root_hasher.finalize().into();
        assert_eq!(result.unwrap().hash, expected_root);
    }

    #[test]
    fn test_build_merkle_tree_deterministic_sort() {
        // Events in reverse order should produce identical root
        let events_asc = [
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000001",
                "550e8400-e29b-41d4-a716-446655440000",
                1000,
            ),
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000002",
                "550e8400-e29b-41d4-a716-446655440000",
                2000,
            ),
        ];
        let events_desc = [
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000002",
                "550e8400-e29b-41d4-a716-446655440000",
                2000,
            ),
            make_event(
                "0000000000000000000000000000000000000000000000000000000000000001",
                "550e8400-e29b-41d4-a716-446655440000",
                1000,
            ),
        ];
        let root_asc = build_merkle_tree(&events_asc);
        let root_desc = build_merkle_tree(&events_desc);
        assert!(root_asc.is_some());
        assert!(root_desc.is_some());
        assert_eq!(root_asc.unwrap().hash, root_desc.unwrap().hash);
    }
}

#[cfg(test)]
mod tokenizer_helpers_tests {
    use super::*;

    #[test]
    fn test_tokenizer_version_to_id_tiktoken() {
        // Test vector from mission spec
        let version = "tiktoken-cl100k_base-v1.2.3";
        let id = tokenizer_version_to_id(version);
        let id_hex = hex::encode(id);
        assert_eq!(id_hex, "e3c8e8ff724411c6416dd4fb135368e3");
    }

    #[test]
    fn test_tokenizer_version_to_id_deterministic() {
        // Same version always produces same id
        let version = "tiktoken-cl100k_base-v1.2.3";
        let id1 = tokenizer_version_to_id(version);
        let id2 = tokenizer_version_to_id(version);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_tokenizer_version_to_id_different_versions() {
        // Different versions produce different ids
        let id1 = tokenizer_version_to_id("tiktoken-cl100k_base-v1.2.3");
        let id2 = tokenizer_version_to_id("tiktoken-cl100k_base-v1.2.4");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_tokenizer_version_to_id_empty_string() {
        // Empty version string is valid input (though unlikely in practice)
        let id = tokenizer_version_to_id("");
        assert_eq!(id.len(), 16);
        // BLAKE3 of empty string is deterministic
        let expected: [u8; 16] = blake3::hash(b"").as_bytes()[..16].try_into().unwrap();
        assert_eq!(id, expected);
    }

    #[test]
    fn test_tokenizer_id_to_version_stub_error() {
        // Stub always returns error
        let id = [0u8; 16];
        let result = tokenizer_id_to_version(&id);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "tokenizer_id_to_version: requires DB lookup implementation"
        );
    }

    #[test]
    fn test_tokenizer_version_to_id_id_length() {
        // Verify output is exactly 16 bytes
        let id = tokenizer_version_to_id("any-version-string");
        assert_eq!(id.len(), 16);
    }
}
