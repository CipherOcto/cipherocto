//! `daemon.search` — full-text + semantic search over the derived
//! `messages` view (stoolap SQL + Tantivy FTS + brute-force cosine).
//!
//! Backed by the `QuerySubsystem` installed on the daemon handle at
//! boot. Returns `{"hits":[...]}` with BM25 score per hit. The
//! handler is the read-path complement to `messages.search`, which
//! delegates to the WA adapter (limited to recent chat JIDs only).
//!
//! Filters: `peer` (exact), `kind` (message kind), `since_ts_unix_ms`,
//! `until_ts_unix_ms`. Pagination: `limit` (default 50, max 200).

use serde::Deserialize;
use serde_json::{json, Value};

use super::super::protocol::{RpcError, RpcErrorCode};
use super::super::server::RpcHandler;
use crate::daemon::DaemonHandle;
use crate::query::{SearchFilters, ServiceError};

#[derive(Deserialize, Default)]
struct Params {
    query: String,
    #[serde(default)]
    peer: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    since_ts_unix_ms: Option<i64>,
    #[serde(default)]
    until_ts_unix_ms: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug)]
pub struct DaemonSearch;

#[async_trait::async_trait]
impl RpcHandler for DaemonSearch {
    fn name(&self) -> &'static str {
        "daemon.search"
    }

    async fn call(&self, h: DaemonHandle, params: Value) -> Result<Value, RpcError> {
        let p: Params = serde_json::from_value(params).map_err(|e| RpcError {
            code: RpcErrorCode::InvalidParams.as_i32(),
            message: format!("invalid params: {e}"),
            data: None,
        })?;
        let svc = h.query_service().ok_or(RpcError {
            code: RpcErrorCode::NotConnected.as_i32(),
            message: "query subsystem not installed; \
                     enable the `query` cargo feature and a persist_dir"
                .into(),
            data: None,
        })?;
        let limit = p.limit.unwrap_or(50).min(200);
        let filters = SearchFilters {
            peer: p.peer.clone(),
            kind: p.kind.clone(),
            since_ts_unix_ms: p.since_ts_unix_ms,
            until_ts_unix_ms: p.until_ts_unix_ms,
        };
        let hits = svc
            .search(&p.query, &filters, limit)
            .map_err(|e| RpcError {
                code: RpcErrorCode::InternalError.as_i32(),
                message: format!("search failed: {e}"),
                data: None,
            })?;
        // Re-serialize via serde_json::to_value so the field order
        // matches the SearchHit struct.
        let hits_json: Vec<Value> = hits
            .iter()
            .filter_map(|h| serde_json::to_value(h).ok())
            .collect();
        Ok(json!({
            "hits": hits_json,
            "query": p.query,
            "count": hits_json.len(),
            "limit": limit,
        }))
    }
}

impl From<ServiceError> for RpcError {
    fn from(e: ServiceError) -> Self {
        RpcError {
            code: RpcErrorCode::InternalError.as_i32(),
            message: format!("query service error: {e}"),
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::Daemon;
    use crate::events::{EventEnvelope, InboundEvent};
    use crate::query::embedder::MockEmbedder;
    use crate::query::tantivy_sidecar::TantivySidecar;
    use crate::query::{QueryIngester, QueryService};
    use std::path::PathBuf;
    use std::sync::Arc;
    use stoolap::Database;
    use tempfile::tempdir;

    fn synth_message(id: u64, peer: &str, text: &str, ts: i64) -> InboundEvent {
        InboundEvent::parse(EventEnvelope {
            raw: format!(
                "Message(id: \"M{id}\", peer: \"{peer}\", sender: \"{peer}\", text: \"{text}\", kind: Text, is_group: false)"
            ),
            ts_unix_ms: ts,
            ts_mono_ns: 0,
        })
    }

    /// Build a daemon handle with a fully wired QuerySubsystem
    /// pointed at hermetic tmpdirs.
    fn handle_with_query() -> (tempfile::TempDir, DaemonHandle) {
        let tmp = tempdir().expect("tmpdir");
        let h = Daemon::new_for_tests(tmp.path()).1;
        // Build a separate sub-tmpdir for the query subsystem.
        let qdir = tempdir().expect("query tmpdir");
        let tantivy = TantivySidecar::in_memory().expect("tantivy");
        let db = Database::open("memory://daemon-search-test").expect("db");
        let ingester = QueryIngester::new(db.clone());
        // Construct subsystem manually with in-memory tantivy + an
        // existing DB handle.
        let _ = qdir;
        // Build a minimal subsystem via open_subsystem with a tmp
        // directory path.
        let base = tempdir().expect("subdir");
        let embedder: Arc<dyn crate::query::embedder::Embedder> =
            Arc::new(MockEmbedder::ok("test", 384));
        let s =
            crate::query::open_subsystem(base.path(), embedder, crate::query::JobConfig::default())
                .expect("open subsystem");
        let arc = Arc::new(s);
        // Index a couple messages into the subsystem.
        for (i, peer) in ["peer_a", "peer_b"].iter().enumerate() {
            arc.handle_one(&synth_message(
                i as u64,
                peer,
                if i == 0 { "hello world" } else { "goodbye" },
                1000 + i as i64,
            ));
        }
        arc.tantivy().reload().expect("reload");
        let installed = h.install_query_subsystem(arc);
        assert!(installed, "first install wins");
        let _ = (
            PathBuf::from("memory://daemon-search-test"),
            tantivy,
            ingester,
        );
        (tmp, h)
    }

    #[tokio::test]
    async fn search_returns_text_matches() {
        let (_tmp, h) = handle_with_query();
        let r = DaemonSearch
            .call(h, serde_json::json!({"query": "hello"}))
            .await
            .unwrap();
        let hits = r["hits"].as_array().unwrap();
        assert!(!hits.is_empty());
        assert_eq!(r["count"], hits.len());
    }

    #[tokio::test]
    async fn search_with_no_query_subsystem_returns_not_connected() {
        let tmp = tempdir().expect("tmpdir");
        let h = Daemon::new_for_tests(tmp.path()).1;
        let err = DaemonSearch
            .call(h, serde_json::json!({"query": "hello"}))
            .await
            .unwrap_err();
        assert_eq!(err.code, RpcErrorCode::NotConnected.as_i32());
    }

    #[tokio::test]
    async fn search_with_peer_filter() {
        let (_tmp, h) = handle_with_query();
        let r = DaemonSearch
            .call(
                h,
                serde_json::json!({
                    "query": "world",
                    "peer": "peer_a",
                }),
            )
            .await
            .unwrap();
        let hits = r["hits"].as_array().unwrap();
        // The "goodbye" message in peer_b doesn't contain "world".
        assert!(hits.iter().all(|h| h["peer"] == "peer_a"));
    }

    // Avoid unused-import warning for QueryService when only used in
    // type position through Arc.
    #[allow(dead_code)]
    fn _type_anchor(_: Arc<QueryService>) {}
}
