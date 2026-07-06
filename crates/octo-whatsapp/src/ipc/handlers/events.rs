//! `events.list` / `events.show` / `events.replay` / `events.tail`
//!
//! Phase 3: the in-memory `EventsBuffer` is the source of truth for
//! `list`/`show`/`replay`. `tail` returns the same shape but with
//! `lagged=0` (the broadcast bus is consumed by the event router; MCP
//! subscribers use a separate per-sink mpsc — see `events_router.rs`).
//!
//! Read endpoints (list/show/replay) never block; they take the
//! buffer's `parking_lot::Mutex` only for the duration of a single
//! snapshot, never across `.await`.

use serde_json::Value;

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 10_000;

#[derive(Debug)]
pub struct EventsList;

#[async_trait::async_trait]
impl RpcHandler for EventsList {
    fn name(&self) -> &'static str {
        "events.list"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let limit = parse_limit(&params)?;
        let events = h.events_buffer().list_recent(limit);
        Ok(serde_json::json!({
            "events": events,
            "limit": limit,
            "buffer_len": h.events_buffer().len(),
            "total_evicted": h.events_buffer().total_evicted(),
        }))
    }
}

#[derive(Debug)]
pub struct EventsShow;

#[async_trait::async_trait]
impl RpcHandler for EventsShow {
    fn name(&self) -> &'static str {
        "events.show"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let id = parse_id(&params)?;
        match h.events_buffer().get(id) {
            Some(ev) => Ok(serde_json::json!({
                "id": id,
                "event": ev,
            })),
            None => Err(RpcError {
                code: RpcErrorCode::InvalidParams.as_i32(),
                message: format!("unknown event id {id}"),
                data: Some(serde_json::json!({ "id": id })),
            }),
        }
    }
}

#[derive(Debug)]
pub struct EventsReplay;

#[async_trait::async_trait]
impl RpcHandler for EventsReplay {
    fn name(&self) -> &'static str {
        "events.replay"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let since_id = params.get("since_id").and_then(|v| v.as_u64()).unwrap_or(0);
        let limit = parse_limit(&params)?;
        let events = h.events_buffer().list(Some(since_id), limit);
        Ok(serde_json::json!({
            "events": events,
            "since_id": since_id,
            "limit": limit,
            "buffer_len": h.events_buffer().len(),
        }))
    }
}

#[derive(Debug)]
pub struct EventsTail;

#[async_trait::async_trait]
impl RpcHandler for EventsTail {
    fn name(&self) -> &'static str {
        "events.tail"
    }
    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        // Phase 3 Part A: `tail` returns the most-recent buffer snapshot
        // (same data as `list` with `lagged=0`). The streaming + Lagged
        // counter surface arrives in Part B (events_router + per-sink
        // mpsc). For now, this gives consumers a non-blocking probe.
        let limit = parse_limit(&params)?;
        let events = h.events_buffer().list_recent(limit);
        Ok(serde_json::json!({
            "events": events,
            "lagged": 0_u64,
            "limit": limit,
            "buffer_len": h.events_buffer().len(),
        }))
    }
}

fn parse_limit(params: &Value) -> Result<usize, RpcError> {
    let n = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_LIMIT as u64);
    if n == 0 || n > MAX_LIMIT as u64 {
        return Err(RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("limit must be 1..={MAX_LIMIT} (got {n})"),
            data: Some(serde_json::json!({ "limit": n })),
        });
    }
    Ok(n as usize)
}

fn parse_id(params: &Value) -> Result<u64, RpcError> {
    let id = params
        .get("id")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: "missing or non-integer `id`".to_string(),
            data: None,
        })?;
    if id == 0 {
        return Err(RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: "id must be >= 1".to_string(),
            data: None,
        });
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WhatsAppRuntimeConfig;
    use crate::daemon::Daemon;
    use crate::events::{EventEnvelope, InboundEvent};
    use serde_json::json;

    fn handle() -> DaemonHandle {
        let cfg = WhatsAppRuntimeConfig::from_toml(br#"name = "evt""#).unwrap();
        Daemon::new(cfg).handle()
    }

    #[tokio::test]
    async fn events_list_returns_empty_when_no_events() {
        let h = handle();
        let v = EventsList.call(h, Value::Null).await.unwrap();
        assert!(v["events"].as_array().unwrap().is_empty());
        assert_eq!(v["buffer_len"], 0);
    }

    #[tokio::test]
    async fn events_list_returns_buffered_events() {
        let h = handle();
        h.events_buffer().push(InboundEvent::parse(EventEnvelope {
            raw: "Message(id: \"X\", peer: \"P\", sender: \"S\", text: \"hi\", kind: Text, is_group: false)".to_string(),
            ts_unix_ms: 1000,
            ts_mono_ns: 1,
        }));
        let v = EventsList.call(h.clone(), Value::Null).await.unwrap();
        assert_eq!(v["events"].as_array().unwrap().len(), 1);
        assert_eq!(v["buffer_len"], 1);
    }

    #[tokio::test]
    async fn events_list_respects_limit() {
        let h = handle();
        for i in 0..5 {
            h.events_buffer().push(InboundEvent::Unknown {
                raw: format!("m{i}"),
                ts_unix_ms: i,
                ts_mono_ns: 0,
                untrusted: false,
            });
        }
        let v = EventsList
            .call(h.clone(), json!({ "limit": 3 }))
            .await
            .unwrap();
        assert_eq!(v["events"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn events_list_rejects_zero_limit() {
        let h = handle();
        let e = EventsList.call(h, json!({ "limit": 0 })).await.unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn events_list_rejects_oversize_limit() {
        let h = handle();
        let e = EventsList
            .call(h, json!({ "limit": 100_000 }))
            .await
            .unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn events_show_returns_event_by_id() {
        let h = handle();
        let id = h.events_buffer().push(InboundEvent::Unknown {
            raw: "m1".into(),
            ts_unix_ms: 1,
            ts_mono_ns: 0,
            untrusted: false,
        });
        let v = EventsShow
            .call(h.clone(), json!({ "id": id }))
            .await
            .unwrap();
        assert_eq!(v["id"], id);
        assert!(v["event"].is_object());
    }

    #[tokio::test]
    async fn events_show_unknown_id_returns_minus_32602() {
        let h = handle();
        let e = EventsShow.call(h, json!({ "id": 9999 })).await.unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn events_show_missing_id_returns_minus_32602() {
        let h = handle();
        let e = EventsShow.call(h, Value::Null).await.unwrap_err();
        assert_eq!(e.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn events_replay_since_id_zero_returns_all() {
        let h = handle();
        for i in 0..3 {
            h.events_buffer().push(InboundEvent::Unknown {
                raw: format!("m{i}"),
                ts_unix_ms: i,
                ts_mono_ns: 0,
                untrusted: false,
            });
        }
        let v = EventsReplay
            .call(h.clone(), json!({ "since_id": 0, "limit": 10 }))
            .await
            .unwrap();
        assert_eq!(v["events"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn events_tail_returns_recent_with_lagged_zero() {
        let h = handle();
        h.events_buffer().push(InboundEvent::Unknown {
            raw: "t1".into(),
            ts_unix_ms: 1,
            ts_mono_ns: 0,
            untrusted: false,
        });
        let v = EventsTail.call(h.clone(), Value::Null).await.unwrap();
        assert_eq!(v["lagged"], 0);
        assert_eq!(v["events"].as_array().unwrap().len(), 1);
    }
}
