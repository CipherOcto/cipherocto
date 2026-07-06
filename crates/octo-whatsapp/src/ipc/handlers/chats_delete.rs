//! `chats.delete` — delete a chat entirely from this device.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    jid: String,
}

#[derive(Debug)]
pub struct ChatsDelete;

#[async_trait::async_trait]
impl RpcHandler for ChatsDelete {
    fn name(&self) -> &'static str {
        "chats.delete"
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
        adapter.delete_chat(&p.jid).await.map_err(|e| RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: format!("adapter delete_chat failed: {e}"),
            data: Some(json!({"jid": p.jid})),
        })?;
        Ok(json!({
            "status": "deleted",
            "jid": p.jid,
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
        let h = handle();
        h.set_adapter_for_tests(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = ChatsDelete
            .call(
                handle(),
                serde_json::json!({
                    "jid": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = ChatsDelete
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "jid": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "deleted");
        assert_eq!(r["jid"], "1234567890@s.whatsapp.net");
    }

    #[tokio::test]
    async fn invalid_params_returns_minus_32602() {
        let err = ChatsDelete
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
            "delete_chat",
            octo_network::dot::error::PlatformAdapterError::Unreachable {
                platform: "mock".into(),
                reason: "test".into(),
            },
        );
        h.set_adapter_for_tests(mock);
        let err = ChatsDelete
            .call(
                h,
                serde_json::json!({
                    "jid": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
        let data = err.data.unwrap();
        assert_eq!(data["jid"], "1234567890@s.whatsapp.net");
    }
}
