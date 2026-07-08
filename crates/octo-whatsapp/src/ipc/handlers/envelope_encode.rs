//! `envelope.encode` — DOT/1/{base64url-no-pad} encode of wire bytes.
//!
//! RFC-0850 §8.6 defines the on-wire envelope format. The adapter's
//! static [`encode_envelope`] is the single source of truth; this
//! handler is a thin RPC wrapper so external tooling (CLI, MCP,
//! scripted clients) can drive the same encode path the daemon uses
//! when routing an outbound envelope to WhatsApp.
//!
//! **Phase 2 stub note:** the adapter is not bound during the unit
//! tests, and `encode_envelope` is a pure static function that does
//! not need a connected adapter. We call it directly via the type
//! path so the handler is testable end-to-end through the unix
//! socket without requiring `live-whatsapp`.
//!
//! [`encode_envelope`]: octo_adapter_whatsapp::WhatsAppWebAdapter::encode_envelope

use std::io::Read;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// Path to a file of wire bytes. If absent, the handler reads
    /// stdin to EOF (blocking — dispatched via `spawn_blocking`).
    #[serde(default)]
    file: Option<PathBuf>,
}

#[derive(Debug)]
pub struct EnvelopeEncode;

#[async_trait::async_trait]
impl RpcHandler for EnvelopeEncode {
    fn name(&self) -> &'static str {
        "envelope.encode"
    }

    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        let wire = if let Some(file) = p.file.as_ref() {
            tokio::fs::read(file).await.map_err(|e| RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!("cannot read {file:?}: {e}"),
                data: None,
            })?
        } else {
            // Read from stdin to EOF — spawn_blocking because
            // `std::io::stdin().read_to_end` is a blocking syscall and
            // must not run on the tokio runtime thread.
            tokio::task::spawn_blocking(|| -> std::io::Result<Vec<u8>> {
                let mut buf = Vec::new();
                std::io::stdin().read_to_end(&mut buf)?;
                Ok(buf)
            })
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!("stdin read join failed: {e}"),
                data: None,
            })?
            .map_err(|e| RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!("stdin read failed: {e}"),
                data: None,
            })?
        };

        let encoded = octo_adapter_whatsapp::WhatsAppWebAdapter::encode_envelope(&wire);

        Ok(json!({
            "encoded": encoded,
            "wire_bytes": wire.len(),
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
    fn name_is_envelope_encode() {
        assert_eq!(EnvelopeEncode.name(), "envelope.encode");
    }

    #[tokio::test]
    async fn invalid_params_returns_minus_32602() {
        // `file` must be a string path (not a number) — type mismatch fails
        // deserialization.
        let err = EnvelopeEncode
            .call(handle(), serde_json::json!({"file": 12345}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn missing_file_returns_minus_32602() {
        // No `file` provided AND no stdin piped — file-read branch fails.
        // We avoid stdin by passing a path that does not exist.
        let err = EnvelopeEncode
            .call(
                handle(),
                serde_json::json!({"file": "/nonexistent/path/envelope_encode_test.bin"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_returns_dot1_form() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("wire.bin");
        let wire = vec![0x01, 0x02, 0x03, 0x04];
        std::fs::write(&f, &wire).unwrap();
        let r = EnvelopeEncode
            .call(handle(), serde_json::json!({"file": f}))
            .await
            .unwrap();
        assert_eq!(r["wire_bytes"], 4);
        let encoded = r["encoded"].as_str().unwrap();
        assert!(encoded.starts_with("DOT/1/"));
        // Must round-trip via decode_envelope.
        let decoded = octo_adapter_whatsapp::WhatsAppWebAdapter::decode_envelope(encoded).unwrap();
        assert_eq!(decoded, wire);
    }
}
