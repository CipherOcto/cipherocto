//! `send.delete` — delete-for-everyone (subject to 3600s window).

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

/// WhatsApp's delete-for-everyone window (seconds). Per design §634.
pub const DELETE_WINDOW_SECONDS: i64 = 3600;

#[derive(Deserialize)]
struct Params {
    peer: String,
    msg_id: String,
    msg_timestamp: i64,
}

#[derive(Debug)]
pub struct SendDelete;

#[async_trait::async_trait]
impl RpcHandler for SendDelete {
    fn name(&self) -> &'static str {
        "send.delete"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if now - p.msg_timestamp > DELETE_WINDOW_SECONDS {
            return Err(RpcError {
                code: RpcErrorCode::DeleteWindowExpired.as_i32(),
                message: "delete-for-everyone window closed (typically 1 hour)".into(),
                data: Some(json!({
                    "msg_timestamp": p.msg_timestamp,
                    "now": now,
                    "window_seconds": DELETE_WINDOW_SECONDS,
                    "elapsed_seconds": now - p.msg_timestamp,
                })),
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        adapter
            .delete_message(&p.peer, &p.msg_id)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter delete_message failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "deleted",
            "msg_id": p.msg_id,
            "elapsed_seconds": now - p.msg_timestamp,
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
    async fn expired_window_returns_minus_32014() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let err = SendDelete
            .call(
                handle(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "msg_id": "ABCDEFG",
                    "msg_timestamp": now - 7200, // 2 hours ago
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::DeleteWindowExpired.as_i32());
        assert_eq!(err.code, -32014);
        let data = err.data.unwrap();
        assert_eq!(data["window_seconds"], DELETE_WINDOW_SECONDS);
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let r = SendDelete
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "msg_id": "ABCDEFG",
                    "msg_timestamp": now - 60, // 1 minute ago, well within window
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "deleted");
        assert_eq!(r["msg_id"], "ABCDEFG");
        // elapsed_seconds should be ~60 (between 0 and the 3600 window)
        let elapsed = r["elapsed_seconds"].as_i64().unwrap();
        assert!((0..=DELETE_WINDOW_SECONDS).contains(&elapsed));
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let err = SendDelete
            .call(
                handle(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "msg_id": "ABCDEFG",
                    "msg_timestamp": now - 60, // 1 minute ago — inside the window
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }
}
