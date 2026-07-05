//! `health.get` — liveness probe. Returns `{ok: true, phase, pid}`.
//! Always returns `ok: true` in Phase 1 (the process is up; deeper readiness
//! lives in `status.get`).

use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::{DaemonHandle, DaemonPhase};

#[derive(Debug)]
pub struct HealthGet;

#[async_trait::async_trait]
impl RpcHandler for HealthGet {
    fn name(&self) -> &'static str {
        "health.get"
    }

    async fn call(&self, handle: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        let phase = match handle.phase() {
            DaemonPhase::Booting => "booting",
            DaemonPhase::Connected => "connected",
            DaemonPhase::SessionLost => "session_lost",
            DaemonPhase::ShuttingDown => "shutting_down",
        };
        Ok(serde_json::json!({
            "ok": true,
            "phase": phase,
            "pid": std::process::id(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn health_get_returns_ok() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = HealthGet.call(h, Value::Null).await.unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["phase"], "booting");
        assert_eq!(v["pid"].as_u64().unwrap(), std::process::id() as u64);
    }
}
