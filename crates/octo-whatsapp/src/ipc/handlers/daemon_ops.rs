//! `reconnect.now` and `shutdown` — daemon-level lifecycle RPCs.
//!
//! - `reconnect.now` is a Phase 1 no-op (no adapter to reconnect to).
//! - `shutdown` cancels the daemon's `CancellationToken`. The supervisor
//!   loop observes the cancellation and exits; subsequent RPCs see
//!   `DaemonPhase::ShuttingDown` and should return `-32099` (handlers in
//!   later phases gate on this).

use serde_json::Value;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::{DaemonHandle, DaemonPhase};

#[derive(Debug)]
pub struct ReconnectNow;
#[derive(Debug)]
pub struct Shutdown;

#[async_trait::async_trait]
impl RpcHandler for ReconnectNow {
    fn name(&self) -> &'static str {
        "reconnect.now"
    }
    async fn call(&self, _h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "ok": true,
            "phase": "phase1_no_reconnect",
        }))
    }
}

#[async_trait::async_trait]
impl RpcHandler for Shutdown {
    fn name(&self) -> &'static str {
        "shutdown"
    }
    async fn call(&self, h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        h.cancel_token().cancel();
        h.set_phase(DaemonPhase::ShuttingDown).await;
        Ok(serde_json::json!({"ok": true}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        Daemon::new(cfg).handle()
    }

    #[tokio::test]
    async fn reconnect_now_noop_in_phase1() {
        let h = handle();
        let v = ReconnectNow.call(h.clone(), Value::Null).await.unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["phase"], "phase1_no_reconnect");
        assert!(!h.cancel_token().is_cancelled());
    }

    #[tokio::test]
    async fn shutdown_cancels_token() {
        // Each test gets a fresh handle: cancelling a CancellationToken is
        // permanent for the lifetime of the token.
        let h = handle();
        let v = Shutdown.call(h.clone(), Value::Null).await.unwrap();
        assert_eq!(v["ok"], true);
        assert!(h.cancel_token().is_cancelled());
        assert_eq!(h.phase(), DaemonPhase::ShuttingDown);
    }
}
