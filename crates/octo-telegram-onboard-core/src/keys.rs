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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;

    #[test]
    fn validate_verifying_key_accepts_44_char_32_byte() {
        let key = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        assert_eq!(key.len(), 44);
        assert!(validate_verifying_key(&key).is_ok());
    }

    #[test]
    fn validate_verifying_key_rejects_short() {
        assert!(validate_verifying_key("short").is_err());
    }

    #[test]
    fn validate_verifying_key_rejects_43_char() {
        assert!(validate_verifying_key(&"A".repeat(43)).is_err());
    }

    #[test]
    fn validate_verifying_key_rejects_45_char() {
        assert!(validate_verifying_key(&"A".repeat(45)).is_err());
    }

    #[test]
    fn validate_verifying_key_rejects_url_safe() {
        let mut key = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        key = key.replace('+', "-").replace('/', "_");
        let key_no_pad = key.trim_end_matches('=');
        assert!(validate_verifying_key(key_no_pad).is_err());
    }

    #[test]
    fn validate_verifying_key_rejects_non_32_byte_decoded() {
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        assert!(validate_verifying_key(&short).is_err());
    }
}
