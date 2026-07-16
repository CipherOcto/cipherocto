//! `newsletter.revoke_message` — revoke (delete) a newsletter message.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jid: String,
    message_id: String,
}

#[derive(Debug)]
pub struct NewsletterRevokeMessage;

#[async_trait::async_trait]
impl RpcHandler for NewsletterRevokeMessage {
    fn name(&self) -> &'static str {
        "newsletter.revoke_message"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.jid.trim().is_empty() || p.message_id.trim().is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "jid and message_id must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .newsletter_revoke_message(&p.jid, &p.message_id)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter newsletter_revoke_message failed: {e}"),
                data: None,
            })?;
        Ok(json!({"status": "revoked", "message_id": p.message_id}))
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
        let err = NewsletterRevokeMessage
            .call(
                handle(),
                serde_json::json!({
                    "jid": "120363012345678901@newsletter",
                    "message_id": "MSG1",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = NewsletterRevokeMessage
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "jid": "120363012345678901@newsletter",
                    "message_id": "MSG1",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "revoked");
        assert_eq!(r["message_id"], "MSG1");
    }
}
