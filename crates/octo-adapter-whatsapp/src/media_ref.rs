// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Mission 0850 (RFC-0850 §8.6/§9.4): wire-format helper for the WhatsApp
// adapter's `DOT/2/{msg_id}` native upload mode.
//
// `MediaRef` carries every field the receiver needs to reconstruct a
// `waproto::whatsapp::DocumentMessage` and call `Client::download`. The
// wire format is base64url-encoded JSON, matching the `DOT/1/{base64url}`
// convention used by `decode_envelope` at `adapter.rs:348-365`.
//
// SECURITY: `MediaRef` contains the AES-256 `media_key` that decrypts the
// CDN blob. Anyone with `media_key` + `direct_path` can fetch and decrypt
// the payload from WhatsApp's CDN. See the `Notes` section of
// `missions/open/0850-whatsapp-media-transport.md` for the full
// confidentiality contract.

use serde::{Deserialize, Serialize};

use octo_network::dot::transport::{b64url_decode, b64url_encode};
use whatsapp_rust::upload::UploadResponse;
use waproto::whatsapp as wa;

// ── MediaRef ───────────────────────────────────────────────────────

/// Wire-format representation of an uploaded WhatsApp media blob.
///
/// Mirrors [`UploadResponse`] field-for-field (so a future wacore upgrade
/// that adds fields doesn't break older receivers — `serde_json` ignores
/// unknown fields on deserialize by default), plus a `filename` for
/// operator-visible logging.
///
/// R1-C3 fix: a standalone struct, NOT a newtype around `UploadResponse`
/// (which does not derive `Serialize` in the pinned wacore rev).
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct MediaRef {
    /// CDN URL (`https://mmg.whatsapp.net/v/t62.7117-24/...`).
    pub(crate) url: String,
    /// CDN host-relative path; used as the primary locator when the URL
    /// is unavailable (e.g., CDN host rotation).
    pub(crate) direct_path: String,
    /// AES-256 media-encryption key. **Sensitive — never log.**
    pub(crate) media_key: [u8; 32],
    /// SHA-256 of the *encrypted* payload; verified by `Client::download`
    /// before decryption.
    pub(crate) file_enc_sha256: [u8; 32],
    /// SHA-256 of the *plaintext* payload; verified by the gateway's
    /// `DeterministicEnvelope::verify_payload_hash` after canonicalize.
    pub(crate) file_sha256: [u8; 32],
    /// Plaintext byte length.
    pub(crate) file_length: u64,
    /// Unix timestamp (seconds) when the media key was generated; used
    /// by the CDN to select the correct key bundle.
    pub(crate) media_key_timestamp: i64,
    /// Operator-supplied filename (metadata only; not used by
    /// `to_document_message`).
    pub(crate) filename: String,
}

// Custom Debug impl — the default would print `media_key` in plaintext.
// All `tracing::debug!(?media_ref)`-style invocations must use the
// redacted formatter.
impl std::fmt::Debug for MediaRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaRef")
            .field("url", &"<redacted>")
            .field("direct_path", &"<redacted>")
            .field("media_key", &"<redacted 32 bytes>")
            .field("file_enc_sha256", &"<redacted 32 bytes>")
            .field("file_sha256", &"<redacted 32 bytes>")
            .field("file_length", &self.file_length)
            .field("media_key_timestamp", &self.media_key_timestamp)
            .field("filename", &self.filename)
            .finish()
    }
}

impl MediaRef {
    /// Build a `MediaRef` from a successful `Client::upload` response.
    pub(crate) fn from_upload_response(response: &UploadResponse, filename: &str) -> Self {
        Self {
            url: response.url.clone(),
            direct_path: response.direct_path.clone(),
            media_key: response.media_key,
            file_enc_sha256: response.file_enc_sha256,
            file_sha256: response.file_sha256,
            file_length: response.file_length,
            media_key_timestamp: response.media_key_timestamp,
            filename: filename.to_string(),
        }
    }

    /// Reconstruct the `DocumentMessage` that `Client::download` accepts.
    ///
    /// `..Default::default()` covers the fields WhatsApp's CDN ignores on
    /// re-download (`mimetype`, `file_name`, `title`, `page_count`, …).
    /// Only the cryptographic locator fields are populated.
    pub(crate) fn to_document_message(&self) -> wa::message::DocumentMessage {
        wa::message::DocumentMessage {
            media_key: Some(self.media_key.to_vec()),
            direct_path: Some(self.direct_path.clone()),
            file_enc_sha256: Some(self.file_enc_sha256.to_vec()),
            file_sha256: Some(self.file_sha256.to_vec()),
            file_length: Some(self.file_length),
            ..Default::default()
        }
    }
}

// ── encode / decode ────────────────────────────────────────────────

/// Encode a `MediaRef` as the base64url-JSON token used inside
/// `DOT/2/{token}`.
pub(crate) fn encode_base64url(media_ref: &MediaRef) -> String {
    // SAFETY: `MediaRef` contains `media_key` in plaintext in the JSON.
    // Callers MUST NOT log the result except inside the `DOT/2/{token}`
    // wire envelope itself.
    let json = serde_json::to_vec(media_ref).expect("MediaRef is always serializable");
    b64url_encode(&json)
}

/// Decode a `DOT/2/{token}` payload back into a `MediaRef`.
///
/// R1-H4 fix: MUST NOT panic on any input. All error paths return
/// `Err(MediaRefError::..)` with a redacted message that does not
/// include the input bytes (which contain `media_key`).
pub(crate) fn decode_base64url(s: &str) -> Result<MediaRef, MediaRefError> {
    // Empty input is a malformed `DOT/2/` token — reject as `Base64` so
    // the contract is consistent: `decode_base64url` only ever produces
    // `Ok(_)` for valid base64url JSON of a `MediaRef`. The empty case
    // would otherwise fall through to `serde_json` and produce a `Json`
    // error, leaking the distinction between "empty token" and "token
    // with bad base64" — both should be `Base64` from the caller's
    // perspective.
    if s.is_empty() {
        return Err(MediaRefError::Base64);
    }
    let bytes = b64url_decode(s).map_err(|_| MediaRefError::Base64)?;
    serde_json::from_slice(&bytes).map_err(MediaRefError::Json)
}

/// Errors from `decode_base64url`. The inner strings are redacted —
/// they do NOT contain the input bytes or any decoded field.
#[derive(Debug)]
pub(crate) enum MediaRefError {
    /// Base64url decode failed. The original input is NOT preserved
    /// (would leak `media_key`).
    Base64,
    /// JSON parse failed (missing fields, type mismatch, or trailing
    /// garbage). The original bytes are NOT preserved.
    Json(serde_json::Error),
}

impl MediaRefError {
    /// R8-M1 fix: short identifier for the variant, used in
    /// `tracing::debug!` calls to distinguish `Base64` from `Json`
    /// failures without leaking the original input bytes. The
    /// `Display` impl returns the same redacted string for both
    /// variants, so the `variant_name` is the only way for an
    /// operator to know which decode stage failed.
    pub(crate) fn variant_name(&self) -> &'static str {
        match self {
            MediaRefError::Base64 => "Base64",
            MediaRefError::Json(_) => "Json",
        }
    }
}

impl std::fmt::Display for MediaRefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Generic string — never echo the input. The `Display` is
            // suitable for `PlatformAdapterError::ApiError { message }`.
            MediaRefError::Base64 => f.write_str("invalid media ref format"),
            MediaRefError::Json(_) => f.write_str("invalid media ref format"),
        }
    }
}

impl std::error::Error for MediaRefError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MediaRefError::Base64 => None,
            MediaRefError::Json(e) => Some(e),
        }
    }
}

// ── tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic `UploadResponse` with every field populated to a
    /// distinct sentinel value. Round-trip tests use this to detect field
    /// drops (the diff between an old and a new `UploadResponse` shape).
    fn synthetic_upload_response() -> UploadResponse {
        UploadResponse {
            url: "https://mmg.whatsapp.net/v/t62.7117-24/synthetic".to_string(),
            direct_path: "/v/t62.7117-24/synthetic".to_string(),
            media_key: [0xA1u8; 32],
            file_enc_sha256: [0xB2u8; 32],
            file_sha256: [0xC3u8; 32],
            file_length: 12345,
            media_key_timestamp: 1_700_000_000,
        }
    }

    #[test]
    fn media_ref_roundtrip() {
        let upload = synthetic_upload_response();
        let media_ref = MediaRef::from_upload_response(&upload, "envelope.bin");
        let token = encode_base64url(&media_ref);
        let decoded = decode_base64url(&token).expect("decode must succeed");

        assert_eq!(decoded.url, upload.url);
        assert_eq!(decoded.direct_path, upload.direct_path);
        assert_eq!(decoded.media_key, upload.media_key);
        assert_eq!(decoded.file_enc_sha256, upload.file_enc_sha256);
        assert_eq!(decoded.file_sha256, upload.file_sha256);
        assert_eq!(decoded.file_length, upload.file_length);
        assert_eq!(decoded.media_key_timestamp, upload.media_key_timestamp);
        assert_eq!(decoded.filename, "envelope.bin");
    }

    #[test]
    fn media_ref_to_document_message() {
        let upload = synthetic_upload_response();
        let media_ref = MediaRef::from_upload_response(&upload, "test.bin");
        let doc = media_ref.to_document_message();

        // Populated fields:
        assert_eq!(doc.media_key.as_deref(), Some(upload.media_key.as_slice()));
        assert_eq!(doc.direct_path.as_deref(), Some(upload.direct_path.as_str()));
        assert_eq!(
            doc.file_enc_sha256.as_deref(),
            Some(upload.file_enc_sha256.as_slice())
        );
        assert_eq!(
            doc.file_sha256.as_deref(),
            Some(upload.file_sha256.as_slice())
        );
        assert_eq!(doc.file_length, Some(upload.file_length));

        // Unpopulated fields (WhatsApp's CDN ignores these on re-download):
        assert!(doc.url.is_none());
        assert!(doc.mimetype.is_none());
        assert!(doc.file_name.is_none());
        assert!(doc.title.is_none());
        assert!(doc.page_count.is_none());
    }

    #[test]
    fn encode_base64url_no_special_chars() {
        let upload = synthetic_upload_response();
        let media_ref = MediaRef::from_upload_response(&upload, "envelope.bin");
        let token = encode_base64url(&media_ref);

        // Standard base64 alphabet is `[A-Za-z0-9+/=]`. Base64url is
        // `[A-Za-z0-9_-]` (no padding). `+` and `/` would break the
        // `DOT/2/{token}` parser inside a text-message body.
        for c in token.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '-' || c == '_',
                "non-base64url char {c:?} in token {token:?}"
            );
        }
    }

    #[test]
    fn decode_base64url_invalid_base64() {
        // `!` is not a base64url char.
        let result = decode_base64url("!!!");
        assert!(matches!(result, Err(MediaRefError::Base64)));
    }

    #[test]
    fn decode_base64url_invalid_json() {
        // Valid base64 that decodes to non-JSON bytes.
        let token = b64url_encode(b"not json");
        let result = decode_base64url(&token);
        assert!(matches!(result, Err(MediaRefError::Json(_))));
    }

    #[test]
    fn decode_base64url_empty_string() {
        let result = decode_base64url("");
        assert!(matches!(result, Err(MediaRefError::Base64)));
    }

    #[test]
    fn decode_base64url_does_not_panic_on_arbitrary_input() {
        // 1 MiB of random bytes — not valid base64url. Must not panic.
        let garbage = "Z".repeat(1024 * 1024);
        let result = decode_base64url(&garbage);
        assert!(result.is_err());
    }

    #[test]
    fn decode_base64url_does_not_leak_input_in_error() {
        // Synthetic input that looks like a MediaRef but fails JSON parse.
        // The error Display string MUST NOT include any field value.
        let upload = synthetic_upload_response();
        let media_ref = MediaRef::from_upload_response(&upload, "test.bin");
        let valid_token = encode_base64url(&media_ref);
        // Truncate the token mid-byte to break JSON parse (the last
        // base64url char loses significance, so the decoded bytes will
        // have a truncated JSON suffix → `Json` error).
        let truncated = &valid_token[..valid_token.len() - 2];
        let result = decode_base64url(truncated);
        let err = result.expect_err("truncated token must fail to decode");
        let display = format!("{err}");
        assert_eq!(display, "invalid media ref format");
        // Defensive: the source error chain (visible via `Error::source`)
        // does not propagate the JSON bytes either, but the `Display` is
        // what reaches the gateway's `PlatformAdapterError::ApiError`
        // message field.
    }

    #[test]
    fn media_ref_field_count_matches_upload_response() {
        // Drift guard: serialized `MediaRef` JSON MUST have exactly the
        // UploadResponse field count (7) + 1 (`filename`). If a future
        // wacore version adds fields to UploadResponse and we forget to
        // mirror them here, this test fails loudly.
        let upload = synthetic_upload_response();
        let media_ref = MediaRef::from_upload_response(&upload, "drift-guard.bin");
        let value = serde_json::to_value(&media_ref).expect("serialize");
        let obj = value.as_object().expect("object");
        assert_eq!(
            obj.len(),
            8,
            "MediaRef must have 7 UploadResponse fields + filename = 8 (got {:?})",
            obj.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn debug_redacts_media_key() {
        let upload = synthetic_upload_response();
        let media_ref = MediaRef::from_upload_response(&upload, "test.bin");
        let formatted = format!("{media_ref:?}");
        // The synthetic upload has `media_key = [0xA1; 32]` — a
        // hex-decimal of `a1` repeated 64 times would be a leak.
        assert!(
            !formatted.contains("a1a1a1a1"),
            "Debug output leaked media_key: {formatted}"
        );
        // `redacted` is the explicit marker in the custom Debug impl.
        assert!(
            formatted.contains("redacted"),
            "Debug output missing redaction marker: {formatted}"
        );
    }
}
