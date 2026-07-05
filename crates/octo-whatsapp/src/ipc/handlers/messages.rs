//! `messages.list` — Phase 1 stub returning empty list with limit echoed.

use serde::Deserialize;
use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize, Default)]
#[allow(dead_code)] // `peer` is reserved for Phase 2 filtering; accepting it now keeps the wire stable.
struct Params {
    #[serde(default)]
    peer: Option<String>,
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
        let p: Params = serde_json::from_value(params).unwrap_or_default();
        Ok(serde_json::json!({
            "messages": [],
            "limit": p.limit.unwrap_or(50),
            "phase": "phase1",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn messages_list_returns_empty_in_phase1() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = MessagesList
            .call(h, serde_json::json!({"limit": 10}))
            .await
            .unwrap();
        assert!(v["messages"].as_array().unwrap().is_empty());
        assert_eq!(v["limit"], 10);
        assert_eq!(v["phase"], "phase1");
    }
}
