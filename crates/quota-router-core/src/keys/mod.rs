pub mod errors;
pub mod models;

pub use errors::KeyError;
pub use models::{ApiKey, KeyType, KeyUpdates};

use hmac_sha256::HMAC;
use rand::RngCore;

/// Server secret for key hashing
const SERVER_SECRET: &[u8] = b"quota-router-server-secret-change-in-production";

/// Compute HMAC-SHA256 hash of an API key
pub fn compute_key_hash(key: &str) -> [u8; 32] {
    HMAC::mac(key.as_bytes(), SERVER_SECRET)
}

/// Generate a cryptographically secure API key string
/// Format: sk-qr-{64 hex characters}
pub fn generate_key_string() -> String {
    let mut bytes = [0u8; 32]; // 256-bit entropy
    rand::thread_rng().fill_bytes(&mut bytes);

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

    let mut random_bytes = [0u8; 10];
    rand::thread_rng().fill_bytes(&mut random_bytes);

    format!(
        "{:016x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        now,
        random_bytes[0], random_bytes[1], random_bytes[2], random_bytes[3],
        random_bytes[4], random_bytes[5], random_bytes[6], random_bytes[7]
    )
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
