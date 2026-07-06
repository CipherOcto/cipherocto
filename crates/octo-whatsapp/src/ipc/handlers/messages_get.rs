//! `messages.get` — fetch a single message by id.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize)]
struct Params {
    msg_id: String,
}

#[derive(Debug)]
pub struct MessagesGet;

#[async_trait::async_trait]
impl RpcHandler for MessagesGet {
    fn name(&self) -> &'static str {
        "messages.get"
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
        // Use message_search as a probe; filter to exact id match.
        let hits = adapter
            .message_search(&p.msg_id, None)
            .await
            .map_err(|e| RpcError {
                code: RpcErrorCode::NotConnected.as_i32(),
                message: format!("adapter message_search failed: {e}"),
                data: None,
            })?;
        let exact: Vec<_> = hits.into_iter().filter(|h| h.msg_id == p.msg_id).collect();
        Ok(json!({
            "messages": exact,
            "msg_id": p.msg_id,
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
        let r = MessagesGet
            .call(
                handle_with_mock(),
                serde_json::json!({
                    "msg_id": "ABCDEFG",
                }),
            )
            .await
            .unwrap();
        assert!(r.is_object());
        assert_eq!(r["msg_id"], "ABCDEFG");
        assert!(r["messages"].is_array());
        // Default mock returns empty Vec, so the filtered list is also empty.
        assert_eq!(r["messages"].as_array().unwrap().len(), 0);
    }
}
