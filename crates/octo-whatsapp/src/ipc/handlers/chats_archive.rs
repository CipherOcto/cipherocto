//! `chats.archive` — archive a chat.

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
pub struct ChatsArchive;

#[async_trait::async_trait]
impl RpcHandler for ChatsArchive {
    fn name(&self) -> &'static str {
        "chats.archive"
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
            .set_chat_archived(&p.jid, true)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter set_chat_archived failed: {e}"),
                data: Some(json!({"jid": p.jid, "archived": true})),
            })?;
        Ok(json!({
            "status": "archived",
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

    fn handle_with_mock() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        h.set_adapter_for_tests(Arc::new(MockAdapter::new()));
        h
    }

    #[tokio::test]
    async fn success_path_with_mock() {
        let r = ChatsArchive
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "jid": "1234567890@s.whatsapp.net",
                }),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "archived");
        assert_eq!(r["jid"], "1234567890@s.whatsapp.net");
    }
}
