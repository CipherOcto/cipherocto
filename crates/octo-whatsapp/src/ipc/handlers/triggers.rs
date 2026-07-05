//! `triggers.list` and `triggers.get` — Phase 1 read-only mirrors of `rules`.

use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::triggers::TriggersView;

#[derive(Debug)]
pub struct TriggersList;
#[derive(Debug)]
pub struct TriggersGet;

#[async_trait::async_trait]
impl RpcHandler for TriggersList {
    fn name(&self) -> &'static str {
        "triggers.list"
    }
    async fn call(&self, _h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "triggers": TriggersView::empty().list(),
            "phase": "phase1_readonly",
        }))
    }
}

#[async_trait::async_trait]
impl RpcHandler for TriggersGet {
    fn name(&self) -> &'static str {
        "triggers.get"
    }
    async fn call(&self, _h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
        Ok(serde_json::json!({
            "id": id,
            "found": TriggersView::empty().get(id).is_some(),
            "phase": "phase1_readonly",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn triggers_list_returns_empty() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = TriggersList.call(h, Value::Null).await.unwrap();
        assert!(v["triggers"].as_array().unwrap().is_empty());
        assert_eq!(v["phase"], "phase1_readonly");
    }
}
