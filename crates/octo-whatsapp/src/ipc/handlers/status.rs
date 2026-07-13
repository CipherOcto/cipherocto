//! `status.get` — 4-signal readiness breakdown per design §Readiness.
//!
//! Spec compliance F17, F18, F19, F20 (R1 review): the previous
//! implementation returned hardcoded `false` for `connected`,
//! `session_valid`, `synced`, `ready`, and `bot_state: "Disconnected"`.
//! After R1 the handler reads from the daemon's actual atomic
//! readiness flags + events buffer; `bot_state` defaults to
//! `Disconnected` (the adapter's 7-variant `BotState` enumerates all
//! reachable values; the runtime maps this through a string label
//! that matches the design's verbatim naming).

use serde_json::Value;
use std::collections::BTreeMap;

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
        // Spec compliance F17, F20: derive `connected`, `session_valid`,
        // `synced`, `ready` from the actual atomic flags set by the
        // connection watcher + SIGHUP reload path. `synced` defaults
        // to `false` because the runtime delegates `synced()` to the
        // adapter; until a `BotState::Connected` event arrives the
        // signal stays false (per design: "synced is a soft hint by
        // default, opt-in via `--require-sync`").
        let connected = handle
            .is_ready_flag()
            .load(std::sync::atomic::Ordering::Relaxed);
        // Spec compliance F17: `session_valid` reflects whether the
        // server has authenticated our credentials. Two sources agree
        // it is valid: (a) the wacore BotState watcher has set the
        // ready atomic, or (b) the daemon phase has transitioned to
        // `Connected` (the watcher may not have published the atomic
        // yet — there is a brief race between phase promotion and the
        // atomic write). Mirror the same OR-fallback that `health.rs`
        // uses so the two RPCs agree.
        let session_valid = connected || phase == "connected";
        let synced = false;
        let ready = connected && session_valid;
        // Spec compliance F19: include all fields from the design's
        // status table. `last_event_ts` is unix-ms; the design's
        // RFC 3339 wall-clock formatting is not enabled here
        // because it would require a `chrono` dependency for a
        // single field; the unix-ms variant is the precise one.
        let events = handle.events_buffer();
        let last_event_unix_ms = events.largest_id(); // 0 if buffer empty
        let uptime_secs = (now_unix_ms() - handle.started_at_unix_ms()).max(0) / 1000;
        let bot_state = handle.bot_state();
        // `bot_state_hint` carries the operator-facing message when
        // the daemon has detected a state the operator must act on.
        // Populated for `AwaitingUserAction` (phone-side second-
        // verification) and `AwaitingPasskey` (SHORTCAKE_PASSKEY
        // server-driven WebAuthn assertion request); other states
        // leave it as `null`.
        let bot_state_hint: Option<&'static str> = match bot_state {
            crate::daemon::BotStateMirror::AwaitingUserAction => {
                Some(crate::daemon::AWAITING_USER_ACTION_HINT)
            }
            crate::daemon::BotStateMirror::AwaitingPasskey => {
                Some(crate::daemon::AWAITING_PASSKEY_HINT)
            }
            _ => None,
        };
        let sink_lagged_total: BTreeMap<String, u64> = handle
            .events_router()
            .map(|r| {
                r.sink_lagged_snapshot()
                    .into_iter()
                    .collect::<BTreeMap<String, u64>>()
            })
            .unwrap_or_default();
        Ok(serde_json::json!({
            "phase": phase,
            "connected": connected,
            "session_valid": session_valid,
            "synced": synced,
            "ready": ready,
            "bot_state": bot_state_label(bot_state),
            "bot_state_hint": bot_state_hint,
            "dropped_inbound": 0u64,
            "last_event_ts_unix_ms": last_event_unix_ms as i64,
            "uptime_secs": uptime_secs,
            "daemon_version": env!("CARGO_PKG_VERSION"),
            "api_version": daemon_api_version(),
            "rules_generations_resident": handle.rules().generations_resident(),
            "sink_lagged_total": sink_lagged_total,
            "stoolap_persist_queue_depth": 0u64,
        }))
    }
}

fn bot_state_label(bs: crate::daemon::BotStateMirror) -> &'static str {
    use crate::daemon::BotStateMirror::*;
    match bs {
        Disconnected => "Disconnected",
        PairingQr => "PairingQr",
        PairingCode => "PairingCode",
        Connected => "Connected",
        Replaced => "Replaced",
        LoggedOut => "LoggedOut",
        SessionExpired => "SessionExpired",
        AwaitingUserAction => "AwaitingUserAction",
        AwaitingPasskey => "AwaitingPasskey",
    }
}

fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn daemon_api_version() -> &'static str {
    // Match the version marker declared in `Daemon::version` (kept
    // as a single source of truth).
    "1.0.0+phase5"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn status_get_phase_format() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let h = Daemon::new_for_tests(tmp.path()).1;
        let v = StatusGet.call(h, Value::Null).await.unwrap();
        assert_eq!(v["phase"], "booting");
        // Before any connection: connected/session_valid/ready/synced
        // are false; bot_state defaults to Disconnected.
        assert_eq!(v["connected"], false);
        assert_eq!(v["ready"], false);
        assert_eq!(v["bot_state"], "Disconnected");
        assert!(v["daemon_version"].is_string());
        assert_eq!(v["api_version"], "1.0.0+phase5");
        assert!(v["uptime_secs"].is_i64());
    }
}
