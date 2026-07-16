//! `messages.list_unavailable` — return rows from
//! `unavailable_messages`, the Phase 7.K table populated when the
//! WA server fanout emits `<unavailable type="...">`. The `kind`
//! column carries the wire-format discriminant
//! (`unknown` | `view_once` | `hosted` | `bot`).
//!
//! **Scoping:** requires the daemon's `QuerySubsystem` (gated behind
//! the `query` cargo feature) so the table can be SELECTed.

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;

#[derive(Deserialize, Default)]
struct Params {
    /// Optional kind filter (`view_once`, `hosted`, `bot`, `unknown`).
    #[serde(default)]
    kind: Option<String>,
    /// Optional peer JID filter.
    #[serde(default)]
    peer: Option<String>,
    /// Optional lower bound on `ts_unix_ms`.
    #[serde(default)]
    since_ts_unix_ms: Option<i64>,
    /// Optional upper bound on `ts_unix_ms`.
    #[serde(default)]
    until_ts_unix_ms: Option<i64>,
    /// Maximum rows to return (default 100, hard cap 500).
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug)]
pub struct MessagesListUnavailable;

#[async_trait::async_trait]
impl RpcHandler for MessagesListUnavailable {
    fn name(&self) -> &'static str {
        "messages.list_unavailable"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;

        let subsystem = h.query_subsystem().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "messages.list_unavailable requires the query subsystem (rebuild with `--features query`)".into(),
            data: None,
        })?;
        let db = subsystem.ingester().db();

        // Build the WHERE clauses incrementally so the SQL stays
        // legible + the param binding matches the column order.
        let mut where_clauses: Vec<String> = Vec::new();
        let mut param_values: Vec<String> = Vec::new();
        if let Some(k) = &p.kind {
            where_clauses.push(format!("kind = '{}'", k.replace('\'', "''")));
            param_values.push(k.clone());
        }
        if let Some(peer) = &p.peer {
            where_clauses.push(format!("peer = '{}'", peer.replace('\'', "''")));
            param_values.push(peer.clone());
        }
        if let Some(since) = p.since_ts_unix_ms {
            where_clauses.push(format!("ts_unix_ms >= {since}"));
        }
        if let Some(until) = p.until_ts_unix_ms {
            where_clauses.push(format!("ts_unix_ms <= {until}"));
        }
        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", where_clauses.join(" AND "))
        };
        let limit = p.limit.unwrap_or(100).min(500);
        let sql = format!(
            "SELECT id, ts_unix_ms, ts_mono_ns, kind, peer, sender, is_unavailable FROM unavailable_messages{} ORDER BY ts_unix_ms DESC LIMIT {}",
            where_sql, limit
        );
        let _ = param_values; // unused: we inline-format for the same reason as messages.read_view_once.

        let mut rows = db.query(&sql, ()).map_err(|e| RpcError {
            code: RpcErrorCode::Internal.as_i32(),
            message: format!("messages.list_unavailable: select failed: {e}"),
            data: None,
        })?;
        let mut items: Vec<Value> = Vec::new();
        for row_res in rows.by_ref() {
            let row = row_res.map_err(|e| RpcError {
                code: RpcErrorCode::Internal.as_i32(),
                message: format!("messages.list_unavailable: row error: {e}"),
                data: None,
            })?;
            items.push(json!({
                "id": row.get::<i64>(0).unwrap_or(0),
                "ts_unix_ms": row.get::<i64>(1).unwrap_or(0),
                "ts_mono_ns": row.get::<i64>(2).unwrap_or(0),
                "kind": row.get::<String>(3).unwrap_or_default(),
                "peer": row.get::<String>(4).unwrap_or_default(),
                "sender": row.get::<String>(5).unwrap_or_default(),
                "is_unavailable": row.get::<i64>(6).map(|v| v == 1).unwrap_or(false),
            }));
        }
        Ok(json!({
            "rows": items,
            "count": items.len(),
            "limit": limit,
        }))
    }
}

#[cfg(all(test, feature = "query"))]
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
        let _db = Database::open("memory://messages-list-unavailable-test").expect("db");
        let embedder: Arc<dyn crate::query::embedder::Embedder> =
            Arc::new(MockEmbedder::ok("test", 384));
        let s =
            crate::query::open_subsystem(base.path(), embedder, crate::query::JobConfig::default())
                .expect("open subsystem");
        h.install_query_subsystem(Arc::new(s));
        let _ = (
            PathBuf::from("memory://messages-list-unavailable-test"),
            base,
        );
        (tmp, h)
    }

    fn ingest_unavailable(h: &DaemonHandle, id: u64, kind: &str) {
        let raw = format!(
            r#"Unavailable(id: "U{id}", peer: "120363411021224818@g.us", sender: "u1", kind: {kind}, ts: 1000, is_unavailable: true)"#
        );
        let ev = InboundEvent::parse(EventEnvelope {
            raw,
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
        assert_eq!(MessagesListUnavailable.name(), "messages.list_unavailable");
    }

    #[tokio::test]
    async fn invalid_params_returns_minus_32602() {
        let err = MessagesListUnavailable
            .call(
                handle().unwrap(),
                serde_json::json!({"limit": "not-a-number"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::InvalidParams.as_i32());
    }

    #[tokio::test]
    async fn missing_query_subsystem_returns_not_connected() {
        let err = MessagesListUnavailable
            .call(handle().unwrap(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn empty_table_returns_empty_response() {
        let (_tmp, h) = handle_with_query();
        let r = MessagesListUnavailable
            .call(h, serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(r["count"], 0);
        assert_eq!(r["rows"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn ingested_unavailable_appears_in_response() {
        let (_tmp, h) = handle_with_query();
        ingest_unavailable(&h, 1, "view_once");
        let r = MessagesListUnavailable
            .call(h, serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["rows"][0]["kind"], "view_once");
        assert_eq!(r["rows"][0]["peer"], "120363411021224818@g.us");
        assert_eq!(r["rows"][0]["is_unavailable"], true);
    }

    #[tokio::test]
    async fn kind_filter_excludes_non_matching_rows() {
        let (_tmp, h) = handle_with_query();
        ingest_unavailable(&h, 1, "view_once");
        ingest_unavailable(&h, 2, "hosted");
        let r = MessagesListUnavailable
            .call(h, serde_json::json!({"kind": "hosted"}))
            .await
            .unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["rows"][0]["kind"], "hosted");
    }

    // Hermetic handle() — no query subsystem.
    fn handle() -> Result<DaemonHandle, RpcError> {
        let tmp = tempfile::tempdir().expect("tmpdir");
        Ok(Daemon::new_for_tests(tmp.path()).1)
    }

    use std::path::PathBuf;
}
