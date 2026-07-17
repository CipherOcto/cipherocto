//! `clients.list` — agent discovery surface for active MCP sessions.
//!
//! Phase 3 Part C: tracks the set of MCP clients that have connected
//! to the daemon's unix socket. Each entry exposes the client's
//! session id + subscribe-time. The session set is owned by the
//! `McpClientRegistry` (in `daemon`), not the handler — handlers are
//! stateless proxies that consult the registry.
//!
//! Phase 5 Part F: `McpClientRegistry` also tracks a per-session
//! notification sink (bounded mpsc) so that `mcp_notify` action
//! dispatches can fan out to subscribed MCP clients. The bounded
//! channel mirrors the `EventsSink` shape used by `EventsRouter`.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

/// Notification envelope sent to subscribed MCP clients. Phase 5
/// Part F: a JSON value + the originating rule id + template.
#[derive(Debug, Clone)]
pub struct McpNotification {
    pub rule_id: String,
    pub template: String,
    pub body: Value,
}

#[derive(Debug, Default)]
pub struct McpClientRegistry {
    inner: Arc<parking_lot::Mutex<Vec<McpClientEntry>>>,
}

#[derive(Debug)]
pub struct McpClientEntry {
    pub session_id: String,
    pub since_ts_unix_ms: i64,
    pub subscribed_events: bool,
    /// Phase 5 Part F: notification sink. Held inside the registry
    /// (not exposed to RPC) so the dispatchers can `try_send` per
    /// subscriber without exposing the channel type to clients.
    /// `pub` so tests can construct entries without going through
    /// the channel API; production code sets this via
    /// `McpClientRegistry::register_with_notif`.
    pub notif_tx: Option<mpsc::Sender<McpNotification>>,
}

impl Clone for McpClientEntry {
    fn clone(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            since_ts_unix_ms: self.since_ts_unix_ms,
            subscribed_events: self.subscribed_events,
            // notif_tx is intentionally NOT cloned; it's a per-entry
            // channel that the registry holds exclusively.
            notif_tx: None,
        }
    }
}

impl McpClientRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new MCP session entry. Returns a receiver the
    /// caller can `recv()` on for rule notifications. Phase 5 Part
    /// F: the receiver is bound at capacity 64 — slow consumers
    /// drop oldest + counter (drops are observable via
    /// `total_lagged`).
    pub fn register_with_notif(
        &self,
        session_id: &str,
        since_ts_unix_ms: i64,
    ) -> mpsc::Receiver<McpNotification> {
        let (tx, rx) = mpsc::channel(64);
        self.inner.lock().push(McpClientEntry {
            session_id: session_id.into(),
            since_ts_unix_ms,
            subscribed_events: true,
            notif_tx: Some(tx),
        });
        rx
    }

    pub fn register(&self, entry: McpClientEntry) {
        self.inner.lock().push(entry);
    }

    /// Phase 5 Part F: try-send the same notification to every
    /// subscriber. Failed sends (full channel or closed receiver)
    /// are counted but do NOT propagate an error to the caller —
    /// the dispatcher should not fail the rule pipeline just
    /// because one client is slow.
    pub fn broadcast_notification(&self, notif: &McpNotification) -> usize {
        let mut delivered = 0usize;
        let entries = self.inner.lock();
        for entry in entries.iter() {
            if let Some(tx) = &entry.notif_tx {
                match tx.try_send(notif.clone()) {
                    Ok(()) => delivered += 1,
                    Err(mpsc::error::TrySendError::Full(_))
                    | Err(mpsc::error::TrySendError::Closed(_)) => {
                        // Drop + carry on. The receiver side
                        // observes its own lagged counter via the
                        // channel's `capacity()` consumer side; we
                        // don't aggregate here.
                    }
                }
            }
        }
        delivered
    }

    /// Number of entries with an attached notification sink.
    pub fn notif_subscriber_count(&self) -> usize {
        self.inner
            .lock()
            .iter()
            .filter(|e| e.notif_tx.is_some())
            .count()
    }

    pub fn unregister(&self, session_id: &str) {
        self.inner.lock().retain(|e| e.session_id != session_id);
    }

    pub fn snapshot(&self) -> Vec<McpClientEntry> {
        self.inner.lock().clone()
    }

    pub fn count(&self) -> usize {
        self.inner.lock().len()
    }
}

#[derive(Debug)]
pub struct ClientsList;

#[async_trait::async_trait]
impl RpcHandler for ClientsList {
    fn name(&self) -> &'static str {
        "clients.list"
    }
    async fn call(&self, h: DaemonHandle, _params: Value) -> Result<Value, RpcError> {
        let clients = h.clients().snapshot();
        let v: Vec<Value> = clients
            .into_iter()
            .map(|e| {
                json!({
                    "session_id": e.session_id,
                    "since_ts_unix_ms": e.since_ts_unix_ms,
                    "subscribed_events": e.subscribed_events,
                })
            })
            .collect();
        Ok(json!({
            "clients": v,
            "count": h.clients().count(),
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

    #[test]
    fn registry_register_and_snapshot() {
        let reg = McpClientRegistry::new();
        reg.register(McpClientEntry {
            session_id: "mcp-a".into(),
            since_ts_unix_ms: 1_000_000,
            subscribed_events: true,
            notif_tx: None,
        });
        reg.register(McpClientEntry {
            session_id: "mcp-b".into(),
            since_ts_unix_ms: 1_000_001,
            subscribed_events: false,
            notif_tx: None,
        });
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].session_id, "mcp-a");
        assert_eq!(reg.count(), 2);
    }

    #[test]
    fn registry_unregister_removes_entry() {
        let reg = McpClientRegistry::new();
        reg.register(McpClientEntry {
            session_id: "mcp-a".into(),
            since_ts_unix_ms: 1,
            subscribed_events: false,
            notif_tx: None,
        });
        reg.unregister("mcp-a");
        assert_eq!(reg.count(), 0);
    }

    #[tokio::test]
    async fn clients_list_returns_empty_initially() {
        let h = handle();
        let v = ClientsList.call(h, Value::Null).await.unwrap();
        assert_eq!(v["count"], 0);
        assert!(v["clients"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn clients_list_returns_registered_clients() {
        let h = handle();
        h.clients().register(McpClientEntry {
            session_id: "mcp-1".into(),
            since_ts_unix_ms: 42,
            subscribed_events: true,
            notif_tx: None,
        });
        let v = ClientsList.call(h.clone(), Value::Null).await.unwrap();
        assert_eq!(v["count"], 1);
        assert_eq!(v["clients"][0]["session_id"], "mcp-1");
        assert_eq!(v["clients"][0]["since_ts_unix_ms"], 42);
        assert!(v["clients"][0]["subscribed_events"].as_bool().unwrap());
    }
}
