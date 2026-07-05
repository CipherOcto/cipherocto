//! `envelope.send` — send a DOT envelope to a peer, with
//! deterministic Text vs Native mode selection per RFC-0850 §8.6.
//!
//! **Phase 2 stub:** validates the input, encodes the file, and
//! reports the mode the runtime would have picked. The actual
//! `client.send_message(...)` call (and the upload path for Native
//! mode) is wired in a later phase that owns the live adapter
//! integration; this handler exists today so the CLI / MCP surface
//! is stable and tests can round-trip through the dispatcher
//! without a live WhatsApp session.
//!
//! Mode selection (RFC-0850 §8.6):
//!   - Encoded `DOT/1/{base64}` length <= `max_text_bytes` (65536):
//!     `"text"` — single inline message.
//!   - Encoded length > `max_text_bytes`:
//!     `"native"` — `upload_media` + DOT/2 reference.
//!
//! Phase 2 always reports `"text"` because the static
//! `max_text_bytes` ceiling is 65536 and a typical envelope
//! round-trips well below that. The selection logic IS implemented
//! here (so the wire bytes are reported correctly) but the actual
//! transport is stubbed at `"queued_for_phase2"`.

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

/// Maximum inline-text payload size per RFC-0850 §8.6.
/// Mirrors `send_text::MAX_TEXT_BYTES` — kept in sync by hand (the
/// RPC handlers compile against `octo-adapter-whatsapp` constants
/// indirectly through this single source).
pub const MAX_TEXT_BYTES: usize = 65_536;

#[derive(Deserialize)]
struct Params {
    /// E.164 phone number or `<digits>@s.whatsapp.net` / `@lid` /
    /// `<digits>@g.us` peer.
    peer: String,
    /// Path to a file of wire bytes (NOT a DOT/1/-encoded string —
    /// use `envelope.send-native` if the input is already encoded).
    file: PathBuf,
}

#[derive(Debug)]
pub struct EnvelopeSend;

#[async_trait::async_trait]
impl RpcHandler for EnvelopeSend {
    fn name(&self) -> &'static str {
        "envelope.send"
    }

    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        // Validate the peer shape up front. Phase 2 doesn't actually
        // send, but a malformed peer should fail before we read the
        // file off disk.
        let _jid = crate::jids::peer_to_jid(&p.peer).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid peer: {e}"),
            data: Some(json!({
                "expected_format": "E.164 or <digits>@s.whatsapp.net or <digits>@lid or <digits>@g.us",
            })),
        })?;

        let wire = tokio::fs::read(&p.file).await.map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("cannot read {:?}: {e}", p.file),
            data: None,
        })?;

        let encoded = octo_adapter_whatsapp::WhatsAppWebAdapter::encode_envelope(&wire);
        let encoded_len = encoded.len();
        let mode = if encoded_len <= MAX_TEXT_BYTES {
            "text"
        } else {
            "native"
        };

        Ok(json!({
            "status": "queued_for_phase2",
            "peer": p.peer,
            "encoded_len": encoded_len,
            "wire_bytes": wire.len(),
            "mode": mode,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_envelope_send() {
        assert_eq!(EnvelopeSend.name(), "envelope.send");
    }

    #[test]
    fn max_text_bytes_matches_send_text_ceiling() {
        // Two handlers enforce the same RFC-0850 §8.6 ceiling. Drift
        // here is a wire-format bug; assert equality at compile-time
        // so the test runner flags divergence.
        assert_eq!(MAX_TEXT_BYTES, super::super::send_text::MAX_TEXT_BYTES);
    }
}
