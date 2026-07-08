//! `messages.edit` — edit text of a previously-sent message (subject to 3600s window).

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::limits::MAX_TEXT_BYTES;

/// WhatsApp's edit-message window (seconds). Per design §633.
pub const EDIT_WINDOW_SECONDS: i64 = 3600;

#[derive(Deserialize)]
struct Params {
    peer: String,
    msg_id: String,
    msg_timestamp: i64,
    new_text: String,
}

#[derive(Debug)]
pub struct MessagesEdit;

#[async_trait::async_trait]
impl RpcHandler for MessagesEdit {
    fn name(&self) -> &'static str {
        "messages.edit"
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
        if now - p.msg_timestamp > EDIT_WINDOW_SECONDS {
            return Err(RpcError {
                code: RpcErrorCode::EditWindowExpired.as_i32(),
                message: "edit window closed (typically 1 hour)".into(),
                data: Some(json!({
                    "msg_timestamp": p.msg_timestamp,
                    "now": now,
                    "window_seconds": EDIT_WINDOW_SECONDS,
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
            .edit_message_checked(&p.peer, &p.msg_id, &p.new_text, MAX_TEXT_BYTES)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter edit_message failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "edited",
            "msg_id": p.msg_id,
            "elapsed_seconds": now - p.msg_timestamp,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;
    use crate::test_mock_adapter::MockAdapter;
    use std::sync::Arc;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        Daemon::new(cfg).handle()
    }

    fn handle_with_mock() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn expired_window_returns_minus_32013() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let err = MessagesEdit
            .call(
                handle(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "msg_id": "ABCDEFG",
                    "msg_timestamp": now - 7200, // 2 hours ago
                    "new_text": "replacement",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::EditWindowExpired.as_i32());
        assert_eq!(err.code, -32013);
        let data = err.data.unwrap();
        assert_eq!(data["window_seconds"], EDIT_WINDOW_SECONDS);
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let r = MessagesEdit
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "msg_id": "ABCDEFG",
                    "msg_timestamp": now - 60, // 1 minute ago — inside the window
                    "new_text": "replacement",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "edited");
        assert_eq!(r["msg_id"], "ABCDEFG");
        assert!(r["elapsed_seconds"].as_i64().unwrap() >= 60);
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let err = MessagesEdit
            .call(
                handle(),
                serde_json::json!({
                    "peer": "+15551234567",
                    "msg_id": "ABCDEFG",
                    "msg_timestamp": now - 60, // 1 minute ago — inside the window
                    "new_text": "replacement",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }
}
