//! `chats.list` — list chats, optionally filtered by kind.
//!
//! Phase 2 stub: no StoolapStore query path wired yet (Phase 3 owns the
//! event router persistence). Returns an empty array so callers can
//! consume the wire shape.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize, Default)]
#[allow(dead_code)] // `kind` / `limit` reserved for Phase 3 event-router filtering.
struct Params {
    #[serde(default)]
    kind: Option<String>, // "dm" | "group"
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug)]
pub struct ChatsList;

#[async_trait::async_trait]
impl RpcHandler for ChatsList {
    fn name(&self) -> &'static str {
        "chats.list"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let _adapter = h.adapter().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "no adapter bound to daemon".into(),
            data: None,
        })?;
        // Phase 2 stub: no StoolapStore query path wired yet (Phase 3 owns
        // event router persistence). Return empty array.
        let limit = p.limit.unwrap_or(100).min(1000);
        let _ = p.kind;
        Ok(json!({
            "chats": [],
            "count": 0,
            "limit": limit,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn chats_list_returns_not_connected_in_phase2() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let err = ChatsList
            .call(h, serde_json::json!({"kind": "dm"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }
}
