//! `status.send_text` — post a text status update.
//!
//! Privacy: `"contacts"` (default) / `"allowlist"` / `"denylist"`.
//! Font: `"SYSTEM"` (default) / `"SYSTEM_TEXT"` / `"FB_SCRIPT"` /
//! `"SYSTEM_BOLD"` / `"MORNINGBREEZE_REGULAR"` /
//! `"CALISTOGA_REGULAR"` / `"EXO2_EXTRABOLD"` /
//! `"COURIERPRIME_BOLD"`. Recipients are the JIDs the status is
//! encrypted to — typically your full contact list.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    text: String,
    /// 0xAARRGGBB background colour. Default 0xFF1E6E4F (matches
    /// WA Web's default green).
    #[serde(default = "default_background")]
    background_argb: u32,
    #[serde(default = "default_font")]
    font: String,
    #[serde(default = "default_privacy")]
    privacy: String,
    recipients: Vec<String>,
}

fn default_background() -> u32 {
    0xFF1E6E4F
}

fn default_font() -> String {
    "SYSTEM".to_string()
}

fn default_privacy() -> String {
    "contacts".to_string()
}

#[derive(Debug)]
pub struct StatusSendText;

#[async_trait::async_trait]
impl RpcHandler for StatusSendText {
    fn name(&self) -> &'static str {
        "status.send_text"
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
            .send_status_text(
                &p.text,
                p.background_argb,
                &p.font,
                &p.privacy,
                &p.recipients,
            )
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("adapter send_status_text failed: {e}"),
                data: None,
            })?;
        Ok(json!({
            "status": "posted",
            "message_id": msg_id,
            "kind": "status_text",
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
        let err = StatusSendText
            .call(
                handle(),
                serde_json::json!({
                    "text": "Hello world",
                    "recipients": ["15551234567@s.whatsapp.net"],
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_recipients_rejected() {
        let err = StatusSendText
            .call(
                handle_with_mock(),
                serde_json::json!({"text": "Hello", "recipients": []}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = StatusSendText
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "text": "Hello world",
                    "recipients": ["15551234567@s.whatsapp.net"],
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "posted");
        assert_eq!(r["message_id"], "fake-status-text-msg-id");
        assert_eq!(r["kind"], "status_text");
    }

    #[tokio::test]
    async fn explicit_font_and_privacy() {
        let r = StatusSendText
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "text": "Hello",
                    "background_argb": 0xFFFF0000_u32,
                    "font": "FB_SCRIPT",
                    "privacy": "allowlist",
                    "recipients": ["15551234567@s.whatsapp.net"],
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "posted");
    }
}
