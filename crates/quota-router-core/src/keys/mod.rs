pub mod errors;
pub mod models;

pub use errors::KeyError;
pub use models::{ApiKey, KeyType, KeyUpdates, KeySpend, Team};

use hmac_sha256::HMAC;
use rand::Rng;
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
        random_bytes[0], random_bytes[1], random_bytes[2], random_bytes[3],
        random_bytes[4], random_bytes[5], random_bytes[6], random_bytes[7]
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
