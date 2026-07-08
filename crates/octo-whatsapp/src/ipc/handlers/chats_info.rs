//! `chats.info` — fetch metadata for a single chat.

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
pub struct ChatsInfo;

#[async_trait::async_trait]
impl RpcHandler for ChatsInfo {
    fn name(&self) -> &'static str {
        "chats.info"
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
        let info = adapter.chat_info(&p.jid).await.map_err(|e| RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: format!("adapter chat_info failed: {e}"),
            data: Some(json!({"jid": p.jid})),
        })?;
        Ok(json!({
            "chat": info,
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
        h.bind_adapter(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn not_connected_returns_minus_32012() {
        let err = ChatsInfo
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
        // Default MockAdapter returns Ok(None) for chat_info.
        let r = ChatsInfo
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "jid": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap();
        assert!(r.is_object());
        assert_eq!(r["jid"], "1234567890@s.whatsapp.net");
        assert!(r["chat"].is_null());
    }
}
