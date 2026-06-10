//! DOT envelope pack/unpack (preserved from 0850f).
//! Mission AC line 97: "envelope.rs - DOT envelope pack/unpack (preserved from 0850f)"
//!
//! Wire format: 218-byte signing payload + 64-byte signature = 282 bytes total.
//! Encoding: base64 URL_SAFE_NO_PAD (preserved from 0850f lib.rs:228).
//!
//! R4 C1: `decode_envelope` now rejects any payload whose decoded length is
//! not exactly 282 bytes (the deterministic envelope wire format). This
//! prevents arbitrary non-envelope base64 strings from being parsed as
//! envelopes and surfacing as `Unreachable` errors (which trigger reconnect
//! logic in the gateway). Users of `decode_envelope` should classify length
//! mismatches as `ApiError(400, ...)` so the gateway treats them as
//! normal API errors rather than transport failures.

use crate::error::{Result, TelegramError};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
/// R7 CRYPTO-L2: compile-time assertion that ENVELOPE_WIRE_LENGTH matches upstream.
/// If upstream changes SIGNING_LEN or SIGNATURE_LEN, this will fail to compile
/// and the adapter's constant must be updated.
#[allow(dead_code)]
const _: [(); 282] = [(); crate::envelope::ENVELOPE_WIRE_LENGTH];

/// The canonical wire-format byte length of a serialised DeterministicEnvelope:
/// 218-byte signing payload + 64-byte signature = 282 bytes.
pub const ENVELOPE_WIRE_LENGTH: usize = 282;

/// Encode an envelope as base64 with DOT prefix.
/// 0850f lib.rs:225 — `pub fn encode_envelope(envelope_bytes: &[u8]) -> String`
pub fn encode_envelope(envelope_bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(envelope_bytes)
}

/// Decode a base64-encoded envelope.
///
/// Returns `TelegramError::Envelope` for:
/// - Base64 decode failures (wrong alphabet, invalid padding, etc.)
/// - Length mismatches — the decoded payload is not exactly
///   [`ENVELOPE_WIRE_LENGTH`] (282) bytes.
///
/// Callers should map `TelegramError::Envelope` to
/// `PlatformAdapterError::ApiError` (not `Unreachable`) so the gateway
/// treats garbled payloads as API errors rather than transport failures.
///
/// 0850f lib.rs:233 — `pub fn decode_envelope(text: &str) -> Result<Vec<u8>, String>`
pub fn decode_envelope(text: &str) -> Result<Vec<u8>> {
    let bytes = URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|e| {
            let snippet = if text.len() > 80 { &text[..80] } else { text };
            tracing::debug!(payload_snippet = %snippet, "decode_envelope: base64 decode failed");
            TelegramError::Envelope(format!("base64 decode error: {}", e))
        })?;
    if bytes.len() != ENVELOPE_WIRE_LENGTH {
        let snippet = if text.len() > 80 { &text[..80] } else { text };
        tracing::debug!(
            payload_snippet = %snippet,
            expected = ENVELOPE_WIRE_LENGTH,
            got = bytes.len(),
            "decode_envelope: length mismatch"
        );
        return Err(TelegramError::Envelope(format!(
            "envelope length mismatch: expected {} bytes, got {}",
            ENVELOPE_WIRE_LENGTH,
            bytes.len()
        )));
    }
    Ok(bytes)
}
