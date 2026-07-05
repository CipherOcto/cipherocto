//! `status.get` — 4-signal readiness breakdown per design §Readiness.
//! Phase 1: all adapter-derived signals are `false` / `0`.

use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::{DaemonHandle, DaemonPhase};

#[derive(Debug)]
pub struct StatusGet;

#[async_trait::async_trait]
impl RpcHandler for StatusGet {
    fn name(&self) -> &'static str {
        "status.get"
    }

    async fn call(&self, handle: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let phase = match handle.phase() {
            DaemonPhase::Booting => "booting",
            DaemonPhase::Connected => "connected",
            DaemonPhase::SessionLost => "session_lost",
            DaemonPhase::ShuttingDown => "shutting_down",
        };
        Ok(serde_json::json!({
            "phase": phase,
            "connected": false,
            "session_valid": false,
            "synced": false,
            "ready": false,
            "bot_state": "Disconnected",
            "dropped_inbound": 0u64,
            "last_event_ts_unix_ms": 0i64,
            "sink_lagged_total": {"mcp": 0u64, "cli": 0u64, "rules": 0u64},
            "stoolap_persist_queue_depth": 0u64,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn status_get_phase_format() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = StatusGet.call(h, Value::Null).await.unwrap();
        assert_eq!(v["phase"], "booting");
        assert_eq!(v["connected"], false);
        assert_eq!(v["ready"], false);
        assert_eq!(v["bot_state"], "Disconnected");
        assert_eq!(v["dropped_inbound"], 0u64);
    }
}
