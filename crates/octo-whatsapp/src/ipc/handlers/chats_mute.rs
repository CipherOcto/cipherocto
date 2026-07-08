//! `chats.mute` — mute a chat until a given epoch-second timestamp.
//!
//! Pass `until_epoch_secs = 0` to unmute.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jid: String,
    until_epoch_secs: i64,
}

#[derive(Debug)]
pub struct ChatsMute;

#[async_trait::async_trait]
impl RpcHandler for ChatsMute {
    fn name(&self) -> &'static str {
        "chats.mute"
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
            .set_chat_muted(&p.jid, p.until_epoch_secs)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter set_chat_muted failed: {e}"),
                data: Some(json!({"jid": p.jid, "until_epoch_secs": p.until_epoch_secs})),
            })?;
        let status = if p.until_epoch_secs == 0 {
            "unmuted"
        } else {
            "muted"
        };
        Ok(json!({
            "status": status,
            "jid": p.jid,
            "until_epoch_secs": p.until_epoch_secs,
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
        let err = ChatsMute
            .call(
                handle(),
                serde_json::json!({
                    "jid": "1234567890@s.whatsapp.net",
                    "until_epoch_secs": 1_700_000_000_i64,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = ChatsMute
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "jid": "1234567890@s.whatsapp.net",
                    "until_epoch_secs": 1_700_000_000_i64,
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "muted");
        assert_eq!(r["jid"], "1234567890@s.whatsapp.net");
        assert_eq!(r["until_epoch_secs"], 1_700_000_000_i64);
    }

    #[tokio::test]
    async fn invalid_params_returns_minus_32602() {
        let err = ChatsMute
            .call(handle(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn adapter_error_returns_minus_32012() {
        let h = handle();
        let mock = Arc::new(MockAdapter::new());
        mock.set_unit_err(
            "set_chat_muted",
            octo_network::dot::error::PlatformAdapterError::Unreachable {
                platform: "mock".into(),
                reason: "test".into(),
            },
        );
        h.bind_adapter(mock);
        let err = ChatsMute
            .call(
                h,
                serde_json::json!({
                    "jid": "1234567890@s.whatsapp.net",
                    "until_epoch_secs": 1_700_000_000_i64,
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
        let data = err.data.unwrap();
        assert_eq!(data["jid"], "1234567890@s.whatsapp.net");
        assert_eq!(data["until_epoch_secs"], 1_700_000_000_i64);
    }
}
