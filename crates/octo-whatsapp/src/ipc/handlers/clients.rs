//! `clients.list` — agent discovery surface for active MCP sessions.
//!
//! Phase 3 Part C: tracks the set of MCP clients that have connected
//! to the daemon's unix socket. Each entry exposes the client's
//! session id + subscribe-time. The session set is owned by the
//! `McpClientRegistry` (in `daemon`), not the handler — handlers are
//! stateless proxies that consult the registry.

use std::sync::Arc;

use serde_json::{json, Value};

use super::super::protocol::RpcError;
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug, Default, Clone)]
pub struct McpClientRegistry {
    inner: Arc<parking_lot::Mutex<Vec<McpClientEntry>>>,
}

#[derive(Debug, Clone)]
pub struct McpClientEntry {
    pub session_id: String,
    pub since_ts_unix_ms: i64,
    pub subscribed_events: bool,
}

impl McpClientRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, entry: McpClientEntry) {
        self.inner.lock().push(entry);
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
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "cl""#).unwrap();
        Daemon::new(cfg).handle()
    }

    #[test]
    fn registry_register_and_snapshot() {
        let reg = McpClientRegistry::new();
        reg.register(McpClientEntry {
            session_id: "mcp-a".into(),
            since_ts_unix_ms: 1_000_000,
            subscribed_events: true,
        });
        reg.register(McpClientEntry {
            session_id: "mcp-b".into(),
            since_ts_unix_ms: 1_000_001,
            subscribed_events: false,
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
        });
        let v = ClientsList.call(h.clone(), Value::Null).await.unwrap();
        assert_eq!(v["count"], 1);
        assert_eq!(v["clients"][0]["session_id"], "mcp-1");
        assert_eq!(v["clients"][0]["since_ts_unix_ms"], 42);
        assert!(v["clients"][0]["subscribed_events"].as_bool().unwrap());
    }
}
