//! `passkey.send_confirmation` — confirm a passkey link after
//! the operator has verified the
//! `Event::PairPasskeyConfirmation` code.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
#[allow(dead_code)] // no params today; struct reserved for future extension.
struct Params {}

#[derive(Debug)]
pub struct PasskeySendConfirmation;

#[async_trait::async_trait]
impl RpcHandler for PasskeySendConfirmation {
    fn name(&self) -> &'static str {
        "passkey.send_confirmation"
    }

    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .send_passkey_confirmation()
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter send_passkey_confirmation failed: {e}"),
                data: None,
            })?;
        Ok(json!({"status": "confirmed"}))
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

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = PasskeySendConfirmation
            .call(handle(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = PasskeySendConfirmation
            .call(handle_with_mock(), serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(r["status"], "confirmed");
    }
}
