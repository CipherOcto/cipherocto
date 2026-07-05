//! `rules.list` and `rules.get` — read-only views. Phase 1 stub returns
//! empty list / not-found from `RulesView::empty()`. Phase 4 will switch to
//! the live `arc_swap::ArcSwap<Ruleset>` view.

use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::rules::RulesView;

#[derive(Debug)]
pub struct RulesList;
#[derive(Debug)]
pub struct RulesGet;

#[async_trait::async_trait]
impl RpcHandler for RulesList {
    fn name(&self) -> &'static str {
        "rules.list"
    }
    async fn call(&self, _h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "rules": RulesView::empty().list(),
            "phase": "phase1_readonly",
        }))
    }
}

#[async_trait::async_trait]
impl RpcHandler for RulesGet {
    fn name(&self) -> &'static str {
        "rules.get"
    }
    async fn call(&self, _h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
        Ok(serde_json::json!({
            "id": id,
            "found": RulesView::empty().get(id).is_some(),
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
    async fn rules_list_returns_empty() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = RulesList.call(h, Value::Null).await.unwrap();
        assert!(v["rules"].as_array().unwrap().is_empty());
        assert_eq!(v["phase"], "phase1_readonly");
    }

    #[tokio::test]
    async fn rules_get_returns_not_found() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = RulesGet
            .call(h, serde_json::json!({"id": "no-such-rule"}))
            .await
            .unwrap();
        assert_eq!(v["found"], false);
        assert_eq!(v["id"], "no-such-rule");
    }
}
