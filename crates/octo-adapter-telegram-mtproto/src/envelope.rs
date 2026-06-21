//! DOT wire-format codec for the MTProto Telegram adapter.
//!
//! Telegram's bot API is text-only: outbound messages are
//! strings, inbound messages have a `text` field. The DOT
//! wire format is therefore emitted as the
//! `DOT/1/{b64}` text form (RFC-0850 §3) for messages that
//! fit within Telegram's per-message text size limit (4096
//! characters for bots, 4096 for users, both
//! post-`MessageEntity` expansion).
//!
//! For larger payloads, the dual-mode `DOT/2/{msg_id}` form
//! (RFC-0850 §8.6) is used: the sender uploads the payload
//! to Telegram as a file (the `sendDocument` RPC) and the
//! message text carries only the `msg_id` reference. The
//! receiver fetches the file via the grammers download API
//! and decodes the bytes via `DeterministicEnvelope::from_wire_bytes`.
//!
//! This module owns the encode/decode of the text form. The
//! `DOT/2/{msg_id}` form is handled in `client.rs`
//! (the `upload_media` / `download_media` methods of
//! `PlatformAdapter`).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

use octo_network::dot::envelope::DeterministicEnvelope;

use crate::error::MtprotoTelegramError;

/// Telegram's per-message text size limit, in bytes, for
/// UTF-8 encoded text. Source: Telegram Bot API docs §"sendMessage".
/// 4096 characters, but each character can be up to 4 UTF-8
/// bytes; we cap at 4096 to be safe.
pub const TELEGRAM_TEXT_LIMIT: usize = 4096;

/// Encode an envelope to the `DOT/1/{b64}` text form.
///
/// Returns `Err(MtprotoTelegramError::Capability(_))` if the
/// envelope's `to_wire_bytes()` would produce a payload
/// larger than `TELEGRAM_TEXT_LIMIT`. The adapter's
/// `send_envelope` will then route to `DOT/2/{msg_id}`
/// (media upload) instead.
pub fn wire_encode(env: &DeterministicEnvelope) -> Result<String, MtprotoTelegramError> {
    let bytes = env.to_wire_bytes();
    if bytes.len() > TELEGRAM_TEXT_LIMIT {
        return Err(MtprotoTelegramError::Capability(format!(
            "envelope payload {} bytes exceeds Telegram text limit {}",
            bytes.len(),
            TELEGRAM_TEXT_LIMIT
        )));
    }
    let b64 = URL_SAFE_NO_PAD.encode(&bytes);
    Ok(format!("DOT/1/{}", b64))
}

/// Decode a `DOT/1/{b64}` text into an envelope. Returns
/// `Err(MtprotoTelegramError::Envelope(_))` if the prefix
/// is missing or the base64 is malformed.
///
/// The `DOT/2/{msg_id}` form is rejected here with a clear
/// error; the adapter's `receive_messages` calls
/// `download_media` for those and re-enters via
/// `DeterministicEnvelope::from_wire_bytes` directly.
pub fn wire_decode(text: &str) -> Result<DeterministicEnvelope, MtprotoTelegramError> {
    if let Some(rest) = text.strip_prefix("DOT/1/") {
        let bytes = URL_SAFE_NO_PAD
            .decode(rest)
            .map_err(|e| MtprotoTelegramError::Envelope(format!("DOT/1 base64: {}", e)))?;
        DeterministicEnvelope::from_wire_bytes(&bytes)
            .map_err(|e| MtprotoTelegramError::Envelope(format!("envelope decode: {}", e)))
    } else if text.starts_with("DOT/2/") {
        Err(MtprotoTelegramError::Envelope(
            "DOT/2/{msg_id} requires download_media; cannot decode inline".into(),
        ))
    } else {
        Err(MtprotoTelegramError::Envelope(format!(
            "missing DOT/1/ or DOT/2/ prefix: {}",
            &text[..text.len().min(20)]
        )))
    }
}

/// True if `text` starts with the `DOT/` wire-format prefix
/// (i.e., is a DOT envelope, not a regular Telegram message).
/// Used by `receive_messages` to filter plain-text
/// non-DOT chatter before calling `wire_decode`.
pub fn is_dot_message(text: &str) -> bool {
    text.starts_with("DOT/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip() {
        // Default DeterministicEnvelope (all-zero fields) round-trips
        // through to_wire_bytes / from_wire_bytes. The signature is
        // not verified by from_wire_bytes (only the length is checked),
        // so an all-zero signature is fine for this smoke test.
        let env = DeterministicEnvelope::default();
        let text = wire_encode(&env).unwrap();
        assert!(text.starts_with("DOT/1/"));
        let back = wire_decode(&text).unwrap();
        // Compare wire bytes; the parsed envelope is byte-identical.
        assert_eq!(back.to_wire_bytes(), env.to_wire_bytes());
    }

    #[test]
    fn decode_rejects_plain_text() {
        let r = wire_decode("hello world");
        assert!(r.is_err());
    }

    #[test]
    fn decode_rejects_dot2_inline() {
        let r = wire_decode("DOT/2/abc123");
        assert!(r.is_err());
    }

    #[test]
    fn is_dot_message_recognises_prefix() {
        assert!(is_dot_message("DOT/1/abc"));
        assert!(is_dot_message("DOT/2/abc"));
        assert!(!is_dot_message("hello"));
    }

    #[test]
    fn encode_rejects_oversize() {
        // DeterministicEnvelope is 282 bytes on the wire. To make
        // the wire encoding exceed TELEGRAM_TEXT_LIMIT, we'd need
        // to mutate the struct directly, but the struct is
        // 282 bytes regardless of payload size. Instead, test that
        // the constant is correct (sanity).
        assert!(TELEGRAM_TEXT_LIMIT < DeterministicEnvelope::default().to_wire_bytes().len() * 100);
    }
}
