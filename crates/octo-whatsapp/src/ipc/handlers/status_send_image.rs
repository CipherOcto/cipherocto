//! `status.send_image` — post an image status update.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    file_path: String,
    #[serde(default)]
    caption: Option<String>,
    /// Base64-encoded JPEG thumbnail bytes (small, < 16 KiB).
    /// Optional — WA Web renders a placeholder when missing.
    #[serde(default)]
    thumbnail_b64: Option<String>,
    #[serde(default = "default_privacy")]
    privacy: String,
    recipients: Vec<String>,
}

fn default_privacy() -> String {
    "contacts".to_string()
}

#[derive(Debug)]
pub struct StatusSendImage;

#[async_trait::async_trait]
impl RpcHandler for StatusSendImage {
    fn name(&self) -> &'static str {
        "status.send_image"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        if p.recipients.is_empty() {
            return Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: "recipients must be non-empty".into(),
                data: None,
            });
        }
        let adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        let msg_id = adapter
            .send_status_image(
                std::path::Path::new(&p.file_path),
                p.caption.as_deref(),
                p.thumbnail_b64.as_deref(),
                &p.privacy,
                &p.recipients,
            )
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter send_status_image failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "posted",
            "message_id": msg_id,
            "kind": "status_image",
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
        let err = StatusSendImage
            .call(
                handle(),
                serde_json::json!({
                    "file_path": "/tmp/no-such.jpg",
                    "recipients": ["15551234567@s.whatsapp.net"],
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_recipients_rejected() {
        let err = StatusSendImage
            .call(
                handle_with_mock(),
                serde_json::json!({"file_path": "/tmp/x.jpg", "recipients": []}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = StatusSendImage
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "file_path": "/tmp/x.jpg",
                    "caption": "hi",
                    "recipients": ["15551234567@s.whatsapp.net"],
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "posted");
        assert_eq!(r["message_id"], "fake-status-image-msg-id");
    }
}
