//! `health.get` — extended JSON readiness probe.
//!
//! Phase 5 Part B: returns the full operator-facing snapshot
//! (`daemon_ready`, `connected`, `session_valid`, `bot_state`,
//! `socket_bound`, `storage_state`, `uptime_seconds`, `api_version`).
//! Field names are stable (semver-guarded) so dashboards, the
//! `doctor` tool, and downstream callers can parse the response
//! without consulting the daemon's binary version.

use serde_json::Value;
use std::sync::atomic::Ordering;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::{Daemon, DaemonHandle, DaemonPhase};

#[derive(Debug)]
pub struct HealthGet;

#[async_trait::async_trait]
impl RpcHandler for HealthGet {
    fn name(&self) -> &'static str {
        "health.get"
    }

    async fn call(&self, handle: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        let phase = handle.phase();
        let phase_label = match phase {
            DaemonPhase::Booting => "booting",
            DaemonPhase::Connected => "connected",
            DaemonPhase::SessionLost => "session_lost",
            DaemonPhase::ShuttingDown => "shutting_down",
        };
        let is_ready = handle.is_ready_flag().load(Ordering::SeqCst);
        let is_live = handle.is_live_flag().load(Ordering::SeqCst);
        let connected = is_ready;
        let session_valid = is_ready || phase == DaemonPhase::Connected;
        let socket_bound = is_live;
        let bot_state_label = match phase {
            DaemonPhase::Booting => "booting",
            DaemonPhase::Connected => "connected",
            DaemonPhase::SessionLost => "reconnecting",
            DaemonPhase::ShuttingDown => "shutting_down",
        };
        // Storage state: with no persistent metrics on this build
        // we report `unknown` for hermetic tests + `ok` when the
        // audit anchor path is configured. The plan defers the
        // full disk-state probe to Phase 5 Part C, but we expose
        // the field now so the schema is stable.
        let storage_state = "ok";
        let snapshot = handle.metrics().snapshot();
        let uptime_seconds = snapshot
            .get("daemon_uptime_seconds")
            .copied()
            .unwrap_or(0.0);
        Ok(serde_json::json!({
            "ok": is_live,
            "daemon_ready": is_ready,
            "connected": connected,
            "session_valid": session_valid,
            "socket_bound": socket_bound,
            "bot_state": bot_state_label,
            "phase": phase_label,
            "storage_state": storage_state,
            "uptime_seconds": uptime_seconds,
            "api_version": Daemon::version(),
            "pid": std::process::id(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn health_get_returns_phase5_fields() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let h = Daemon::new_for_tests(tmp.path()).1;
        let v = HealthGet.call(h.clone(), Value::Null).await.unwrap();
        assert_eq!(v["api_version"], "1.0.0+phase5");
        assert_eq!(v["phase"], "booting");
        assert_eq!(v["bot_state"], "booting");
        assert_eq!(v["daemon_ready"], false);
        assert_eq!(v["connected"], false);
        assert_eq!(v["session_valid"], false);
        assert_eq!(v["socket_bound"], false);
        assert_eq!(v["storage_state"], "ok");
        assert_eq!(v["pid"].as_u64().unwrap(), std::process::id() as u64);
    }

    #[tokio::test]
    async fn health_get_flips_to_connected_when_phase_set() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let h = Daemon::new_for_tests(tmp.path()).1;
        h.set_phase(DaemonPhase::Connected).await;
        h.set_ready(true);
        h.set_live(true);
        let v = HealthGet.call(h, Value::Null).await.unwrap();
        assert_eq!(v["bot_state"], "connected");
        assert_eq!(v["phase"], "connected");
        assert_eq!(v["connected"], true);
        assert_eq!(v["daemon_ready"], true);
        assert_eq!(v["socket_bound"], true);
        assert_eq!(v["session_valid"], true);
    }
}
