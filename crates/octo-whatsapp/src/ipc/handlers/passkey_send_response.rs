//! `passkey.send_response` — send the WebAuthn assertion for an
//! inbound `Event::PairPasskeyRequest` and open the handshake.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    /// Base64-encoded WebAuthn assertion JSON (`<webauthn_assertion>`
    /// payload).
    assertion_json_b64: String,
    /// Base64-encoded credential `rawId` bytes.
    credential_id_b64: String,
}

#[derive(Debug)]
pub struct PasskeySendResponse;

#[async_trait::async_trait]
impl RpcHandler for PasskeySendResponse {
    fn name(&self) -> &'static str {
        "passkey.send_response"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.assertion_json_b64.trim().is_empty() || p.credential_id_b64.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "assertion_json_b64 and credential_id_b64 must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .send_passkey_response(&p.assertion_json_b64, &p.credential_id_b64)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter send_passkey_response failed: {e}"),
                data: None,
            })?;
        Ok(json!({"status": "opened"}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    fn handle_with_mock() -> DaemonHandle {
        let h = handle();
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
    }

    // 16 bytes of zero, base64 = "AAAAAAAAAAAAAAAAAAAAAA=="
    const SAMPLE_B64: &str = "AAAAAAAAAAAAAAAAAAAAAA==";

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = PasskeySendResponse
            .call(
                handle(),
                serde_json::json!({
                    "assertion_json_b64": SAMPLE_B64,
                    "credential_id_b64": SAMPLE_B64,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_assertion_rejected() {
        let err = PasskeySendResponse
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "assertion_json_b64": "  ",
                    "credential_id_b64": SAMPLE_B64,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = PasskeySendResponse
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "assertion_json_b64": SAMPLE_B64,
                    "credential_id_b64": SAMPLE_B64,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "opened");
    }
}
