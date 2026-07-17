//! `actions.escalate` RPC handler. Phase 4.
//!
//! Phase 4 stub: returns a synthetic `escalation_token` UUID and a
//! `dispatched` flag. Real implementation lands in Phase 5 once
//! `actions.escalate` has its own transport (PagerDuty / Slack /
//! custom). The handler exists today so the RPC surface is
//! complete and `daemon.methods.list` advertises the method.

use serde_json::Value;
use uuid::Uuid;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct ActionsEscalate;

#[async_trait::async_trait]
impl RpcHandler for ActionsEscalate {
    fn name(&self) -> &'static str {
        "actions.escalate"
    }
    async fn call(&self, _h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        let target = p
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing target".to_string()))?;
        let reason = p
            .get("reason")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::invalid_params("missing reason".to_string()))?;
        let token = Uuid::new_v4().to_string();
        Ok(serde_json::json!({
            "escalation_token": token,
            "target": target,
            "reason": reason,
            "dispatched": false,
            "phase": "phase4_stub",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let tmp = tempfile::tempdir().expect("tempdir");
        Daemon::new_for_tests(tmp.path()).1
    }

    #[tokio::test]
    async fn returns_token_for_valid_args() {
        let h = handle();
        let r = ActionsEscalate
            .call(
                h,
                serde_json::json!({"target": "oncall", "reason": "alert"}),
            )
            .await
            .unwrap();
        assert_eq!(r["target"], "oncall");
        assert_eq!(r["reason"], "alert");
        assert!(r["escalation_token"].as_str().unwrap().len() >= 32);
    }

    #[tokio::test]
    async fn rejects_missing_target() {
        let h = handle();
        let err = ActionsEscalate
            .call(h, serde_json::json!({"reason": "x"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }

    #[tokio::test]
    async fn rejects_missing_reason() {
        let h = handle();
        let err = ActionsEscalate
            .call(h, serde_json::json!({"target": "x"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, -32602);
    }
}
