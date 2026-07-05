//! `messages.list` — Phase 2 stub. Returns empty list because Phase 3 owns
//! the event-router persistence layer. Wire shape mirrors the Phase 1 stub
//! so callers built against the Phase 1 contract keep working.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize, Default)]
#[allow(dead_code)] // `peer` / `since` / `limit` reserved for Phase 3 event-router filtering.
struct Params {
    #[serde(default)]
    peer: Option<String>,
    #[serde(default)]
    since: Option<i64>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug)]
pub struct MessagesList;

#[async_trait::async_trait]
impl RpcHandler for MessagesList {
    fn name(&self) -> &'static str {
        "messages.list"
    }

    async fn call(&self, _h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        // TODO(phase3): replace with adapter-backed query once the event router
        // owns a persisted message index. Until then we return an empty Vec
        // so callers can already consume the wire shape.
        Ok(json!({
            "messages": [],
            "limit": p.limit.unwrap_or(50),
            "phase": "phase2",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn messages_list_returns_empty_in_phase2() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = MessagesList
            .call(h, serde_json::json!({"limit": 10}))
            .await
            .unwrap();
        assert!(v["messages"].as_array().unwrap().is_empty());
        assert_eq!(v["limit"], 10);
        assert_eq!(v["phase"], "phase2");
    }
}
