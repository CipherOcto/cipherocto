//! `envelope.send-native` — send raw wire bytes via the document path.
//!
//! This handler is the inverse of [`envelope_send`]: the input is
//! RAW wire bytes (NOT a `DOT/1/{base64}` envelope) and the daemon
//! uploads them via `MediaType::Document` + emits a `DOT/2/{token}`
//! reference per RFC-0850 §8.6 Native mode.
//!
//! **Pre-flight guard (design §923):** if the input file starts with
//! the bytes `DOT/`, the handler MUST refuse with `-32602` and a
//! message directing the caller to `envelope.send` (Text mode).
//! Catching the misuse at the daemon boundary prevents accidentally
//! double-encoding a payload that already carries an envelope
//! prefix.
//!
//! [`envelope_send`]: super::envelope_send

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

/// Maximum raw payload size (inclusive) per RFC-0850 §8.6 — matches
/// `octo-adapter-whatsapp::WhatsAppWebAdapter::MAX_UPLOAD_BYTES`
/// (100 MiB).
pub const MAX_NATIVE_BYTES: usize = 100 * 1024 * 1024;

#[derive(Deserialize)]
struct Params {
    /// E.164 phone number or `<digits>@s.whatsapp.net` / `@lid` /
    /// `<digits>@g.us` peer.
    peer: String,
    /// Path to a file of RAW wire bytes. Must NOT start with the
    /// bytes `DOT/` (see module docs).
    file: PathBuf,
}

#[derive(Debug)]
pub struct EnvelopeSendNative;

#[async_trait::async_trait]
impl RpcHandler for EnvelopeSendNative {
    fn name(&self) -> &'static str {
        "envelope.send-native"
    }

    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        // Validate the peer shape up front, before reading the file
        // off disk (cheaper failure path).
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

        // Pre-flight guard (design §923): reject double-encoded
        // payloads at the daemon boundary. The bytes `DOT/` are the
        // prefix of every encoded envelope (DOT/1/{b64} for Text,
        // DOT/2/{token} for Native, future DOT/N for further
        // versions).
        if wire.starts_with(b"DOT/") {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "input must be raw wire bytes, not DOT/*".to_string(),
                data: Some(json!({
                    "hint": "use envelope.send for already-encoded DOT/1/{b64} payloads",
                })),
            });
        }

        // Pre-flight size ceiling (matches the adapter's
        // MAX_UPLOAD_BYTES; the adapter enforces it again on
        // `upload_media`, this is a cheap early reject).
        if wire.len() > MAX_NATIVE_BYTES {
            return Err(RpcError {
                code: RpcErrorCode::PayloadTooLarge.as_i32(),
                message: format!(
                    "native payload is {} bytes; ceiling is {}; \
                     use a smaller payload or chunked transport",
                    wire.len(),
                    MAX_NATIVE_BYTES
                ),
                data: Some(json!({
                    "size_bytes": wire.len(),
                    "max_bytes": MAX_NATIVE_BYTES,
                })),
            });
        }

        Ok(json!({
            "status": "queued_for_phase2",
            "peer": p.peer,
            "wire_bytes": wire.len(),
            "mode": "native",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_envelope_send_native() {
        assert_eq!(EnvelopeSendNative.name(), "envelope.send-native");
    }
}
