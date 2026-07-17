//! `status.revoke` — revoke a previously-posted status update.
//!
//! `recipients` MUST match the list used at send time — the
//! revoke is individually encrypted to the same set of devices.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    message_id: String,
    #[serde(default = "default_privacy")]
    privacy: String,
    recipients: Vec<String>,
}

fn default_privacy() -> String {
    "contacts".to_string()
}

#[derive(Debug)]
pub struct StatusRevoke;

#[async_trait::async_trait]
impl RpcHandler for StatusRevoke {
    fn name(&self) -> &'static str {
        "status.revoke"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.message_id.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "message_id must be non-empty".into(),
                data: None,
            });
        }
        if p.recipients.is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "recipients must be non-empty (must match send-time list)".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let msg_id = adapter
            .revoke_status(&p.message_id, &p.privacy, &p.recipients)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter revoke_status failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "revoked",
            "message_id": p.message_id,
            "revoke_message_id": msg_id,
        }))
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
        let err = StatusRevoke
            .call(
                handle(),
                serde_json::json!({
                    "message_id": "STATUS_MSG_ID",
                    "recipients": ["15551234567@s.whatsapp.net"],
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_message_id_rejected() {
        let err = StatusRevoke
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "message_id": "  ",
                    "recipients": ["15551234567@s.whatsapp.net"],
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn empty_recipients_rejected() {
        let err = StatusRevoke
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "message_id": "STATUS_MSG_ID",
                    "recipients": [],
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = StatusRevoke
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "message_id": "STATUS_MSG_ID",
                    "recipients": ["15551234567@s.whatsapp.net"],
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "revoked");
        assert_eq!(r["message_id"], "STATUS_MSG_ID");
        assert_eq!(r["revoke_message_id"], "fake-status-revoke-msg-id");
    }
}
