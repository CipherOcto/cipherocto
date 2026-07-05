//! `events.list` and `events.show` — Phase 1 in-memory read view.
//!
//! Phase 1 has no event tail (`/events.tail` arrives in Phase 2 with the
//! real adapter). `events.list` returns the empty buffer; `events.show`
//! returns a structured "unknown id" error so the method stays available.

use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Debug)]
pub struct EventsList;
#[derive(Debug)]
pub struct EventsShow;

#[async_trait::async_trait]
impl RpcHandler for EventsList {
    fn name(&self) -> &'static str {
        "events.list"
    }
    async fn call(&self, _h: DaemonHandle, _p: Value) -> Result<Value, RpcError> {
        Ok(serde_json::json!({
            "events": [],
            "phase": "phase1_no_tail",
        }))
    }
}

#[async_trait::async_trait]
impl RpcHandler for EventsShow {
    fn name(&self) -> &'static str {
        "events.show"
    }
    async fn call(&self, _h: DaemonHandle, p: Value) -> Result<Value, RpcError> {
        // Phase 1: no buffer — every id is unknown. Keep the method registered
        // (returns a structured error rather than MethodNotFound) so callers
        // can probe existence without falling back to the unknown-method
        // dispatcher path.
        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Err(RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("unknown event id {id:?}: phase1 has no event buffer"),
            data: Some(serde_json::json!({
                "id": id,
                "phase": "phase1_no_tail",
            })),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;

    #[tokio::test]
    async fn events_list_returns_empty_in_phase1() {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "x""#).unwrap();
        let h = Daemon::new(cfg).handle();
        let v = EventsList.call(h, Value::Null).await.unwrap();
        assert!(v["events"].as_array().unwrap().is_empty());
        assert_eq!(v["phase"], "phase1_no_tail");
    }
}
