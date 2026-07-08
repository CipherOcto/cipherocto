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
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    #[test]
    fn name_is_envelope_send_native() {
        assert_eq!(EnvelopeSendNative.name(), "envelope.send-native");
    }

    #[tokio::test]
    async fn invalid_params_returns_minus_32602() {
        // Missing `file`.
        let err = EnvelopeSendNative
            .call(handle(), serde_json::json!({"peer": "+15551234567"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn invalid_peer_returns_minus_32602_with_data() {
        let err = EnvelopeSendNative
            .call(
                handle(),
                serde_json::json!({
                    "peer": "abc",  // contains '@' but not a valid suffix
                    "file": "/tmp/whatever.bin",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
        assert!(err.data.is_some());
    }

    #[tokio::test]
    async fn dot_prefixed_input_rejected_at_boundary() {
        // Pre-flight guard: refuse bytes that already start with DOT/.
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("encoded.txt");
        std::fs::write(&f, b"DOT/1/AAAAalready_encoded_payload").unwrap();
        let err = EnvelopeSendNative
            .call(
                handle(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "file": f,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
        assert!(err.message.contains("raw wire bytes"));
        assert!(err.data.is_some());
        assert_eq!(
            err.data.unwrap()["hint"],
            "use envelope.send for already-encoded DOT/1/{b64} payloads"
        );
    }

    #[tokio::test]
    async fn oversize_payload_returns_minus_32004() {
        // Payload over MAX_NATIVE_BYTES triggers PayloadTooLarge (-32004).
        // We don't actually write 100 MiB to disk — use a sparse file or
        // skip if file doesn't exist after we create the path. Instead,
        // assert with a missing file path that produces the InvalidParams
        // error first, then test size enforcement via a fresh file.
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("huge.bin");
        // Write a 1 MiB payload and assert it succeeds (well under ceiling).
        // Then for the oversize branch, write a small file but assert via
        // a smaller mock ceiling: instead, validate the early-exit branch
        // by feeding wire bytes that DO start with "DOT/" (covered above).
        // The oversize branch requires a real 100 MiB allocation; cover it
        // by writing a stub that exceeds the ceiling through a test helper
        // — see `dot_prefixed_input_rejected_at_boundary` for the prefix
        // path.
        //
        // We exercise the oversize branch by writing a real 100 MiB + 1
        // byte file. This is slow but covers the only remaining branch.
        let huge = vec![0u8; MAX_NATIVE_BYTES + 1];
        std::fs::write(&f, &huge).unwrap();
        let err = EnvelopeSendNative
            .call(
                handle(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "file": f,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::PayloadTooLarge.as_i32());
        assert!(err.data.is_some());
        assert_eq!(
            err.data.as_ref().unwrap()["size_bytes"].as_u64().unwrap(),
            (MAX_NATIVE_BYTES + 1) as u64
        );
        assert_eq!(
            err.data.unwrap()["max_bytes"].as_u64().unwrap(),
            MAX_NATIVE_BYTES as u64
        );
    }

    #[tokio::test]
    async fn success_path_reports_native_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("raw.bin");
        let wire = vec![0xCC; 1024];
        std::fs::write(&f, &wire).unwrap();
        let r = EnvelopeSendNative
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "file": f,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "queued_for_phase2");
        assert_eq!(r["mode"], "native");
        assert_eq!(r["wire_bytes"], wire.len());
        assert_eq!(r["peer"], "1234567890@s.whatsapp.net");
    }
}
