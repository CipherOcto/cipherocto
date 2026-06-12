//! Key and credential validation utilities.

use crate::error::{OnboardError, Result};

/// Validate verifying_key is valid base64, exactly 44 chars, and decodes to 32 bytes (Ed25519).
pub fn validate_verifying_key(key: &str) -> Result<()> {
    use base64::Engine as _;
    if key.len() != 44 {
        return Err(OnboardError::BadConfig(format!(
            "verifying_key must be exactly 44 characters (standard base64 of 32 bytes), got {}",
            key.len()
        )));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(key.as_bytes())
        .map_err(|_| {
            OnboardError::BadConfig(
                "verifying_key is not valid standard base64 (URL-safe or unpadded not supported; \
                 use `base64` CLI or `openssl base64` to convert)"
                    .into(),
            )
        })?;
    if decoded.len() != 32 {
        return Err(OnboardError::BadConfig(format!(
            "verifying_key must decode to 32 bytes (Ed25519), got {}",
            decoded.len()
        )));
    }
    Ok(())
}
