//! `messages.forward` — forward a previously-sent message to a new peer.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    peer: String,
    original_msg_id: String,
}

#[derive(Debug)]
pub struct MessagesForward;

#[async_trait::async_trait]
impl RpcHandler for MessagesForward {
    fn name(&self) -> &'static str {
        "messages.forward"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let new_id = adapter
            .forward_message(&p.peer, &p.original_msg_id)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter forward_message failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "forwarded",
            "peer": p.peer,
            "original_msg_id": p.original_msg_id,
            "new_msg_id": new_id,
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
        let err = MessagesForward
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "original_msg_id": "ABCDEFG",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = MessagesForward
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "original_msg_id": "ABCDEFG",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "forwarded");
        assert_eq!(r["peer"], "1234567890@s.whatsapp.net");
        assert_eq!(r["original_msg_id"], "ABCDEFG");
        assert_eq!(r["new_msg_id"], "fake-fwd-msg-id");
    }
}
