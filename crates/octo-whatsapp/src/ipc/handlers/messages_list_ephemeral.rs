//! `messages.list_ephemeral` — return rows from the `messages`
//! table whose `ephemeral_expires_at_seconds IS NOT NULL` (i.e.
//! the message has a disappearing-message / `EphemeralSettings`
//! timer). Columns surfaced: event_id, peer, sender, ts_unix_ms,
//! ephemeral_expires_at_seconds, kind. Mirrors the persistence
//! contract added in Phase 7.K v2.
//!
//! **Scoping:** requires the daemon's `QuerySubsystem` (gated behind
//! the `query` cargo feature).

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize, Default)]
struct Params {
    /// Optional peer JID filter.
    #[serde(default)]
    peer: Option<String>,
    /// Optional kind filter (`text` | `image` | ...).
    #[serde(default)]
    kind: Option<String>,
    /// Maximum rows to return (default 100, hard cap 500).
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug)]
pub struct MessagesListEphemeral;

#[async_trait::async_trait]
impl RpcHandler for MessagesListEphemeral {
    fn name(&self) -> &'static str {
        "messages.list_ephemeral"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        let subsystem = h.query_subsystem().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "messages.list_ephemeral requires the query subsystem (rebuild with `--features query`)".into(),
            data: None,
        })?;
        let db = subsystem.ingester().db();

        let mut where_clauses: Vec<String> =
            vec!["ephemeral_expires_at_seconds IS NOT NULL".to_string()];
        if let Some(peer) = &p.peer {
            where_clauses.push(format!("peer = '{}'", peer.replace('\'', "''")));
        }
        if let Some(k) = &p.kind {
            where_clauses.push(format!("kind = '{}'", k.replace('\'', "''")));
        }
        let where_sql = format!(" WHERE {}", where_clauses.join(" AND "));
        let limit = p.limit.unwrap_or(100).min(500);
        let sql = format!(
            "SELECT event_id, peer, sender, ts_unix_ms, kind, ephemeral_expires_at_seconds \
             FROM messages{} ORDER BY ts_unix_ms DESC LIMIT {}",
            where_sql, limit
        );

        let mut rows = db.query(&sql, ()).map_err(|e| RpcError {
            code: RpcErrorCode::Internal.as_i32(),
            message: format!("messages.list_ephemeral: select failed: {e}"),
            data: None,
        })?;
        let mut items: Vec<Value> = Vec::new();
        for row_res in rows.by_ref() {
            let row = row_res.map_err(|e| RpcError {
                code: RpcErrorCode::Internal.as_i32(),
                message: format!("messages.list_ephemeral: row error: {e}"),
                data: None,
            })?;
            items.push(json!({
                "event_id": row.get::<i64>(0).unwrap_or(0),
                "peer": row.get::<String>(1).unwrap_or_default(),
                "sender": row.get::<String>(2).unwrap_or_default(),
                "ts_unix_ms": row.get::<i64>(3).unwrap_or(0),
                "kind": row.get::<String>(4).unwrap_or_default(),
                "ephemeral_expires_at_seconds": row.get::<i64>(5).ok(),
            }));
        }
        Ok(json!({
            "rows": items,
            "count": items.len(),
            "limit": limit,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::events::{EventEnvelope, InboundEvent};
    use crate::query::embedder::MockEmbedder;
    use std::sync::Arc;
    use stoolap::Database;

    fn handle_with_query() -> (tempfile::TempDir, DaemonHandle) {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let h = Daemon::new_for_tests(tmp.path()).1;
        let base = tempfile::tempdir().expect("subdir");
        let _db = Database::open("memory://messages-list-ephemeral-test").expect("db");
        let embedder: Arc<dyn crate::query::embedder::Embedder> =
            Arc::new(MockEmbedder::ok("test", 384));
        let s =
            crate::query::open_subsystem(base.path(), embedder, crate::query::JobConfig::default())
                .expect("open subsystem");
        h.install_query_subsystem(Arc::new(s));
        let _ = (PathBuf::from("memory://messages-list-ephemeral-test"), base);
        (tmp, h)
    }

    fn ingest_ephemeral(h: &DaemonHandle, id: u64, ttl_seconds: u32) {
        let raw = format!(
            r#"Message(id: "E{id}", peer: "120363411021224818@g.us", sender: "u1", text: "vanishing", kind: Text, ephemeral_expires_at_seconds: {ttl_seconds}, is_group: true)"#
        );
        let ev = InboundEvent::parse(EventEnvelope {
            raw: raw.into(),
            ts_unix_ms: 1000,
            ts_mono_ns: 0,
        });
        h.query_subsystem()
            .unwrap()
            .ingester()
            .ingest(id, (1000, 0), &ev)
            .expect("ingest");
    }

    fn ingest_plain(h: &DaemonHandle, id: u64) {
        let raw = format!(
            r#"Message(id: "P{id}", peer: "120363411021224818@g.us", sender: "u1", text: "permanent", kind: Text, is_group: true)"#
        );
        let ev = InboundEvent::parse(EventEnvelope {
            raw: raw.into(),
            ts_unix_ms: 1000,
            ts_mono_ns: 0,
        });
        h.query_subsystem()
            .unwrap()
            .ingester()
            .ingest(id, (1000, 0), &ev)
            .expect("ingest");
    }

    #[test]
    fn name_is_correct() {
        assert_eq!(MessagesListEphemeral.name(), "messages.list_ephemeral");
    }

    #[tokio::test]
    async fn invalid_params_returns_minus_32602() {
        let err = MessagesListEphemeral
            .call(handle().unwrap(), serde_json::json!({"limit": "bad"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn missing_query_subsystem_returns_not_connected() {
        let err = MessagesListEphemeral
            .call(handle().unwrap(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_table_returns_empty_response() {
        let (_tmp, h) = handle_with_query();
        let r = MessagesListEphemeral
            .call(h, serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(r["count"], 0);
        assert_eq!(r["rows"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn ephemeral_message_appears_with_ttl() {
        let (_tmp, h) = handle_with_query();
        ingest_ephemeral(&h, 1, 86_400);
        let r = MessagesListEphemeral
            .call(h, serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["rows"][0]["ephemeral_expires_at_seconds"], 86_400);
        assert_eq!(r["rows"][0]["peer"], "120363411021224818@g.us");
    }

    #[tokio::test]
    async fn plain_message_excluded_from_ephemeral_list() {
        let (_tmp, h) = handle_with_query();
        ingest_ephemeral(&h, 1, 86_400);
        ingest_plain(&h, 2);
        let r = MessagesListEphemeral
            .call(h, serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["rows"][0]["ephemeral_expires_at_seconds"], 86_400);
    }

    fn handle() -> Result<DaemonHandle, RpcError> {
        let tmp = tempfile::tempdir().expect("tmpdir");
        Ok(Daemon::new_for_tests(tmp.path()).1)
    }

    use std::path::PathBuf;
}
