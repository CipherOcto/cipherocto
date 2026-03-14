pub mod errors;
pub mod models;

pub use errors::KeyError;
pub use models::{ApiKey, KeyType, KeyUpdates};

use hmac_sha256::HMAC;

/// Server secret for key hashing
const SERVER_SECRET: &[u8] = b"quota-router-server-secret-change-in-production";

/// Compute HMAC-SHA256 hash of an API key
pub fn compute_key_hash(key: &str) -> [u8; 32] {
    HMAC::mac(key.as_bytes(), SERVER_SECRET)
}
