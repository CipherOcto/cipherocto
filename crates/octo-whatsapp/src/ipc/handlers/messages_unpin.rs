//! `messages.unpin` — unpin a previously pinned message.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    peer: String,
    msg_id: String,
}

#[derive(Debug)]
pub struct MessagesUnpin;

#[async_trait::async_trait]
impl RpcHandler for MessagesUnpin {
    fn name(&self) -> &'static str {
        "messages.unpin"
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
        adapter
            .unpin_message(&p.peer, &p.msg_id)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter unpin_message failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "unpinned",
            "peer": p.peer,
            "msg_id": p.msg_id,
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
        let err = MessagesUnpin
            .call(
                handle(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "msg_id": "ABCDEFG",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = MessagesUnpin
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "1234567890@s.whatsapp.net",
                    "msg_id": "ABCDEFG",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "unpinned");
        assert_eq!(r["peer"], "1234567890@s.whatsapp.net");
        assert_eq!(r["msg_id"], "ABCDEFG");
    }
}
