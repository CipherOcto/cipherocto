//! DOT envelope pack/unpack (preserved from 0850f).
//! Mission AC line 97: "envelope.rs - DOT envelope pack/unpack (preserved from 0850f)"
//!
//! Wire format: 218-byte signing payload + 64-byte signature = 282 bytes total.
//! Encoding: base64 URL_SAFE_NO_PAD (preserved from 0850f lib.rs:228).

use crate::error::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

/// Encode an envelope as base64 with DOT prefix.
/// 0850f lib.rs:225 — `pub fn encode_envelope(envelope_bytes: &[u8]) -> String`
pub fn encode_envelope(envelope_bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(envelope_bytes)
}

/// Decode a base64-encoded envelope.
/// 0850f lib.rs:233 — `pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String>`
pub fn decode_envelope(text: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|e| crate::error::TelegramError::Envelope(format!("base64 decode error: {}", e)))
}
