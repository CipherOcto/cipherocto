//! DOT Transport Mode Selection (RFC-0850 §8.6, §9)
//!
//! Determines how envelopes are encoded for platform transport.
//! Mode selection is deterministic: same payload + same capabilities → same mode.
//!
//! ## Wire Formats
//!
//! | Format | Mode | Description |
//! |--------|------|-------------|
//! | `DOT/1/{base64}` | Text | Base64url-encoded envelope bytes |
//! | `DOT/2/{msg_id}` | Native | Platform message ID referencing uploaded file |
//! | `DOT/F/{base64_fragment}` | Fragment | Base64-encoded fragment with header |
//! | `RAW/{binary}` | Raw | Native binary (QUIC, WebRTC, NativeP2P) |
//!
//! ## Mode Selection (deterministic)
//!
//! 1. If `capabilities.supports_raw_binary` → `Raw`
//! 2. If `payload.len() <= max_text_bytes` → `Text`
//! 3. If `capabilities.media_capabilities.supports_upload` → `Native`
//! 4. If `capabilities.supports_fragmentation` → `Fragment`
//! 5. Else → Error: payload too large

use crate::dot::adapters::CapabilityReport;

/// Transport mode for DOT envelope encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    /// DOT/1/{base64} — text mode (always works)
    Text,
    /// DOT/2/{msg_id} — native platform upload
    Native,
    /// DOT/F/{base64_fragment} — fragmented
    Fragment,
    /// RAW/{binary} — raw binary (QUIC, WebRTC, NativeP2P)
    Raw,
}

/// Error returned when no transport mode can accommodate the payload.
#[derive(Debug, Clone)]
pub struct PayloadTooLargeError {
    pub payload_len: usize,
    pub max_payload: usize,
}

impl std::fmt::Display for PayloadTooLargeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Payload ({} bytes) exceeds maximum ({} bytes) for all available transport modes",
            self.payload_len, self.max_payload
        )
    }
}

impl std::error::Error for PayloadTooLargeError {}

/// Default maximum text payload size (before switching to native/fragment).
///
/// Conservative default that works across all text-based platforms.
/// Individual adapters may have lower limits (IRC: 512B, LoRa: 256B).
pub const DEFAULT_MAX_TEXT_BYTES: usize = 4000;

/// Select the transport mode for a given payload and adapter capabilities.
///
/// Deterministic: same inputs → same mode. Mode is NOT part of envelope identity.
pub fn select_mode(
    payload_len: usize,
    capabilities: &CapabilityReport,
) -> Result<TransportMode, PayloadTooLargeError> {
    select_mode_with_max_text(payload_len, capabilities, DEFAULT_MAX_TEXT_BYTES)
}

/// Select transport mode with a custom max_text_bytes threshold.
///
/// This allows adapters to specify their own text size limits (e.g., IRC: 512).
pub fn select_mode_with_max_text(
    payload_len: usize,
    capabilities: &CapabilityReport,
    max_text_bytes: usize,
) -> Result<TransportMode, PayloadTooLargeError> {
    // 1. Raw binary transport (QUIC, WebRTC, NativeP2P)
    if capabilities.supports_raw_binary {
        return Ok(TransportMode::Raw);
    }

    // 2. Text mode (small enough for text platforms)
    if payload_len <= max_text_bytes {
        return Ok(TransportMode::Text);
    }

    // 3. Native upload (platform media API)
    if capabilities.media_capabilities.is_some() {
        return Ok(TransportMode::Native);
    }

    // 4. Fragment mode (split into platform-sized chunks)
    if capabilities.supports_fragmentation {
        return Ok(TransportMode::Fragment);
    }

    // 5. No viable mode
    Err(PayloadTooLargeError {
        payload_len,
        max_payload: capabilities.max_payload_bytes,
    })
}

/// Encode a DOT/2/{msg_id} wire format reference.
pub fn encode_native_ref(message_id: &str) -> String {
    format!("DOT/2/{}", message_id)
}

/// Decode a DOT/2/{msg_id} wire format reference.
/// Returns the platform message_id if the prefix matches.
pub fn decode_native_ref(text: &str) -> Option<&str> {
    text.trim().strip_prefix("DOT/2/")
}

/// Encode a DOT/F/{base64_fragment} wire format.
pub fn encode_fragment_ref(fragment_b64: &str) -> String {
    format!("DOT/F/{}", fragment_b64)
}

/// Decode a DOT/F/{base64_fragment} wire format.
/// Returns the base64 fragment data if the prefix matches.
pub fn decode_fragment_ref(text: &str) -> Option<&str> {
    text.trim().strip_prefix("DOT/F/")
}

/// Detect the transport mode from a wire format string.
///
/// Returns `None` if the prefix is unrecognized.
pub fn detect_mode(text: &str) -> Option<TransportMode> {
    let trimmed = text.trim();
    if trimmed.starts_with("DOT/1/") {
        Some(TransportMode::Text)
    } else if trimmed.starts_with("DOT/2/") {
        Some(TransportMode::Native)
    } else if trimmed.starts_with("DOT/F/") {
        Some(TransportMode::Fragment)
    } else {
        // Raw binary has no prefix — detected by binary content
        None
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dot::adapters::MediaCapabilities;

    fn caps_text_only() -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 4096,
            supports_fragmentation: false,
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: 100,
            media_capabilities: None,
        }
    }

    fn caps_with_upload() -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 65536,
            supports_fragmentation: true,
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: 100,
            media_capabilities: Some(MediaCapabilities {
                max_upload_bytes: 50_000_000,
                supported_mime_types: vec![],
            }),
        }
    }

    fn caps_fragment_only() -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 4096,
            supports_fragmentation: true,
            supports_encryption: false,
            supports_raw_binary: false,
            rate_limit_per_second: 100,
            media_capabilities: None,
        }
    }

    fn caps_raw_binary() -> CapabilityReport {
        CapabilityReport {
            max_payload_bytes: 1_048_576,
            supports_fragmentation: true,
            supports_encryption: true,
            supports_raw_binary: true,
            rate_limit_per_second: 10_000,
            media_capabilities: None,
        }
    }

    #[test]
    fn test_select_mode_small_payload_text() {
        let caps = caps_text_only();
        assert_eq!(select_mode(100, &caps).unwrap(), TransportMode::Text);
    }

    #[test]
    fn test_select_mode_large_payload_text_only_fails() {
        let caps = caps_text_only();
        assert!(select_mode(5000, &caps).is_err());
    }

    #[test]
    fn test_select_mode_large_payload_native_upload() {
        let caps = caps_with_upload();
        assert_eq!(select_mode(5000, &caps).unwrap(), TransportMode::Native);
    }

    #[test]
    fn test_select_mode_large_payload_fragment() {
        let caps = caps_fragment_only();
        assert_eq!(select_mode(5000, &caps).unwrap(), TransportMode::Fragment);
    }

    #[test]
    fn test_select_mode_raw_binary_wins() {
        let caps = caps_raw_binary();
        // Raw binary always wins, regardless of payload size
        assert_eq!(select_mode(100, &caps).unwrap(), TransportMode::Raw);
        assert_eq!(select_mode(100_000, &caps).unwrap(), TransportMode::Raw);
    }

    #[test]
    fn test_select_mode_deterministic() {
        let caps = caps_with_upload();
        let mode1 = select_mode(5000, &caps).unwrap();
        let mode2 = select_mode(5000, &caps).unwrap();
        assert_eq!(mode1, mode2);
    }

    #[test]
    fn test_select_mode_native_preferred_over_fragment() {
        // When both upload and fragmentation are available, native wins
        let caps = caps_with_upload();
        assert_eq!(select_mode(5000, &caps).unwrap(), TransportMode::Native);
    }

    #[test]
    fn test_encode_decode_native_ref() {
        let encoded = encode_native_ref("msg_abc123");
        assert_eq!(encoded, "DOT/2/msg_abc123");
        assert_eq!(decode_native_ref("DOT/2/msg_abc123"), Some("msg_abc123"));
        assert_eq!(decode_native_ref("DOT/1/base64data"), None);
    }

    #[test]
    fn test_encode_decode_fragment_ref() {
        let encoded = encode_fragment_ref("aGVsbG8=");
        assert_eq!(encoded, "DOT/F/aGVsbG8=");
        assert_eq!(decode_fragment_ref("DOT/F/aGVsbG8="), Some("aGVsbG8="));
        assert_eq!(decode_fragment_ref("DOT/1/base64data"), None);
    }

    #[test]
    fn test_detect_mode() {
        assert_eq!(detect_mode("DOT/1/base64data"), Some(TransportMode::Text));
        assert_eq!(detect_mode("DOT/2/msg_abc123"), Some(TransportMode::Native));
        assert_eq!(detect_mode("DOT/F/fragdata"), Some(TransportMode::Fragment));
        assert_eq!(detect_mode("random binary data"), None);
    }

    #[test]
    fn test_detect_mode_with_whitespace() {
        assert_eq!(
            detect_mode("  DOT/1/base64data  "),
            Some(TransportMode::Text)
        );
    }

    #[test]
    fn test_payload_too_large_error_display() {
        let err = PayloadTooLargeError {
            payload_len: 10_000,
            max_payload: 4096,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("10000"));
        assert!(msg.contains("4096"));
    }

    #[test]
    fn test_select_mode_custom_max_text() {
        let caps = caps_fragment_only();
        // With custom max_text of 512 (IRC), a 600-byte payload needs fragment
        assert_eq!(
            select_mode_with_max_text(600, &caps, 512).unwrap(),
            TransportMode::Fragment
        );
        // But 400 bytes fits in text mode
        assert_eq!(
            select_mode_with_max_text(400, &caps, 512).unwrap(),
            TransportMode::Text
        );
    }
}
