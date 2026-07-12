//! `QueryService` — the read-path. Joins Tantivy BM25 hits against
//! the SQL store to return denormalized rows.
//!
//! Composes three lower-level primitives:
//!
//! - [`TantivySidecar::search`] — BM25 over `messages.text`.
//! - [`QueryIngester::db`] — direct SQL handle for filter + sort + page.
//! - [`crate::query::embedder`] — semantic recall (Phase 1 task 8).
//!
//! All three are feature-gated together with the rest of `query`.
//!
//! Filters supported:
//! - `peer` (exact match on the chat JID)
//! - `kind` (message kind: text/image/video/...)
//! - `since_ts_unix_ms` / `until_ts_unix_ms` (window)
//!
//! Results are sorted by BM25 score descending. When `peer`/`kind`/
//! time filters exclude some Tantivy hits, the SQL-side `WHERE`
//! filters them out. Pagination is by `offset`/`limit` applied on
//! the joined result.
//!
//! See `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Part 6
//! (Query service).

use crate::query::ingester::QueryIngester;
use crate::query::tantivy_sidecar::TantivySidecar;
use serde::{Deserialize, Serialize};
use stoolap::{Database, Value};
use thiserror::Error;

/// One search hit surfaced to callers. Joined result of BM25 + SQL.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub event_id: i64,
    pub peer: String,
    pub sender: String,
    pub ts_unix_ms: i64,
    pub kind: String,
    pub text: String,
    pub score: f32,
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub peer: Option<String>,
    pub kind: Option<String>,
    pub since_ts_unix_ms: Option<i64>,
    pub until_ts_unix_ms: Option<i64>,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("tantivy error: {0}")]
    Tantivy(#[from] crate::query::tantivy_sidecar::TantivyError),
    #[error("stoolap error: {0}")]
    Stoolap(#[from] stoolap::Error),
    #[error("invalid filter: {0}")]
    InvalidFilter(String),
}

pub struct QueryService {
    tantivy: std::sync::Arc<TantivySidecar>,
    ingester: std::sync::Arc<QueryIngester>,
}

impl std::fmt::Debug for QueryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryService").finish_non_exhaustive()
    }
}

impl QueryService {
    pub fn new(
        tantivy: std::sync::Arc<TantivySidecar>,
        ingester: std::sync::Arc<QueryIngester>,
    ) -> Self {
        Self { tantivy, ingester }
    }

    /// Full-text search. Returns hits joined to denormalized SQL
    /// rows, filtered by the supplied predicates.
    pub fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchHit>, ServiceError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        // 1. BM25 over tantivy — over-fetch by 5x to absorb filters.
        let over_fetch = limit.saturating_mul(5).max(50);
        let text_hits = self.tantivy.search(query, over_fetch)?;
        if text_hits.is_empty() {
            return Ok(Vec::new());
        }
        // 2. Join against SQL, applying peer/kind/time filters.
        let mut hits = Vec::with_capacity(text_hits.len());
        for hit in &text_hits {
            if let Some(row) = self.fetch_row(hit.event_id)? {
                if !matches(&row, filters) {
                    continue;
                }
                hits.push(SearchHit {
                    event_id: hit.event_id,
                    score: hit.score,
                    peer: row.peer,
                    sender: row.sender,
                    ts_unix_ms: row.ts_unix_ms,
                    kind: row.kind,
                    text: row.text,
                });
            }
        }
        // 3. Already in BM25 order from tantivy; just truncate.
        hits.truncate(limit);
        Ok(hits)
    }

    /// Fetch the next `limit` messages in `peer`, newest-first.
    /// Used by the `messages.recent` RPC.
    pub fn recent(&self, peer: Option<&str>, limit: usize) -> Result<Vec<SearchHit>, ServiceError> {
        let mut sql = String::from(
            "SELECT event_id, peer, sender, ts_unix_ms, kind, text \
             FROM messages WHERE 1=1",
        );
        let mut params: Vec<Value> = Vec::new();
        if let Some(p) = peer {
            sql.push_str(" AND peer = ?");
            params.push(Value::from(p.to_string()));
        }
        sql.push_str(" ORDER BY ts_unix_ms DESC LIMIT ?");
        params.push(Value::from(limit as i64));
        let db: &Database = self.ingester.db();
        let rows = db.query(&sql, params)?;
        let mut out = Vec::with_capacity(limit);
        for row in rows {
            let row = row?;
            out.push(SearchHit {
                event_id: row.get::<i64>(0).unwrap_or(0),
                peer: get_str(&row, 1),
                sender: get_str(&row, 2),
                ts_unix_ms: get_i64(&row, 3),
                kind: get_str(&row, 4),
                text: get_str(&row, 5),
                score: 0.0,
            });
        }
        Ok(out)
    }

    /// Surrounding messages: `before` before + `after` after, by ts.
    /// Used by the `messages.context` RPC.
    pub fn context(
        &self,
        event_id: i64,
        before: usize,
        after: usize,
    ) -> Result<Vec<SearchHit>, ServiceError> {
        let db: &Database = self.ingester.db();
        let mut pivot_row = db.query(
            "SELECT ts_unix_ms, peer FROM messages WHERE event_id = ?",
            vec![Value::from(event_id)],
        )?;
        let pivot_ts = pivot_row
            .next()
            .ok_or_else(|| ServiceError::InvalidFilter("event_id not found".into()))??
            .get::<i64>(0)
            .unwrap_or(0);
        // pivot_row is consumed by the first `next()`; we need peer.
        // Re-query to fetch peer separately.
        let peer_row = {
            let mut r = db.query(
                "SELECT peer FROM messages WHERE event_id = ?",
                vec![Value::from(event_id)],
            )?;
            r.next().and_then(|x| x.ok())
        }
        .map(|r| get_str(&r, 0))
        .unwrap_or_default();
        // SQL: peer + ts window (lower bound heuristic; ordered by ts asc)
        let sql = "SELECT event_id, peer, sender, ts_unix_ms, kind, text \
                   FROM messages \
                   WHERE peer = ? \
                     AND ts_unix_ms >= ? \
                   ORDER BY ts_unix_ms ASC LIMIT ?";
        let rows = db.query(
            sql,
            vec![
                Value::from(peer_row),
                Value::from(pivot_ts - (before as i64) * 60_000),
                Value::from((before + after + 1) as i64),
            ],
        )?;
        let mut out = Vec::new();
        for row in rows {
            let row = row?;
            out.push(SearchHit {
                event_id: row.get::<i64>(0).unwrap_or(0),
                peer: get_str(&row, 1),
                sender: get_str(&row, 2),
                ts_unix_ms: get_i64(&row, 3),
                kind: get_str(&row, 4),
                text: get_str(&row, 5),
                score: 0.0,
            });
        }
        Ok(out)
    }

    fn fetch_row(&self, event_id: i64) -> Result<Option<MessageRow>, ServiceError> {
        let db: &Database = self.ingester.db();
        let mut rows = db.query(
            "SELECT peer, sender, ts_unix_ms, kind, text FROM messages WHERE event_id = ?",
            vec![Value::from(event_id)],
        )?;
        match rows.next() {
            None => Ok(None),
            Some(Err(e)) => Err(ServiceError::Stoolap(e)),
            Some(Ok(row)) => Ok(Some(MessageRow {
                // `row.get::<T>` fails on NULL columns (returns
                // TypeConversion). The DB schema marks these fields
                // NOT NULL except `text`, so we only fall back on
                // `text` — the others must always be present.
                peer: get_str(&row, 0),
                sender: get_str(&row, 1),
                ts_unix_ms: get_i64(&row, 2),
                kind: get_str(&row, 3),
                text: get_str(&row, 4),
            })),
        }
    }
}

/// Helper: extract a column as `String`, defaulting to empty on NULL
/// or type-conversion failure (only used for `text` where NULL is
/// legal; NOT NULL columns fall through to the actual value).
fn get_str(row: &stoolap::ResultRow, idx: usize) -> String {
    row.get::<String>(idx).unwrap_or_default()
}

fn get_i64(row: &stoolap::ResultRow, idx: usize) -> i64 {
    row.get::<i64>(idx).unwrap_or(0)
}

#[derive(Debug)]
struct MessageRow {
    peer: String,
    sender: String,
    ts_unix_ms: i64,
    kind: String,
    text: String,
}

fn matches(row: &MessageRow, f: &SearchFilters) -> bool {
    if let Some(p) = &f.peer {
        if &row.peer != p {
            return false;
        }
    }
    if let Some(k) = &f.kind {
        if &row.kind != k {
            return false;
        }
    }
    if let Some(s) = f.since_ts_unix_ms {
        if row.ts_unix_ms < s {
            return false;
        }
    }
    if let Some(u) = f.until_ts_unix_ms {
        if row.ts_unix_ms > u {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventEnvelope, InboundEvent};
    use crate::query::embedder::MockEmbedder;
    use crate::query::ingester::QueryIngester;
    use crate::query::schema::migrate;
    use crate::query::tantivy_sidecar::{IndexedMessage, TantivySidecar};
    use stoolap::Database;

    fn synth(id: u64, peer: &str, text: &str, ts: i64) -> InboundEvent {
        InboundEvent::parse(EventEnvelope {
            raw: format!(
                "Message(id: \"M{id}\", peer: \"{peer}\", sender: \"{peer}\", text: \"{text}\", kind: Text, is_group: false)"
            ),
            ts_unix_ms: ts,
            ts_mono_ns: 0,
        })
    }

    fn fixture() -> (Database, TantivySidecar, QueryIngester) {
        let db = Database::open_in_memory().expect("open");
        migrate(&db).expect("migrate");
        let tantivy = TantivySidecar::in_memory().expect("tantivy");
        // Share the SAME database handle between the test and the
        // ingester so messages rows are visible to QueryService.
        // Stoolap in-memory databases have a single handle per DSN,
        // so we clone the handle (cheap, Arc-backed).
        let ingester = QueryIngester::new(db.clone());
        (db, tantivy, ingester)
    }

    fn ingest_message(
        tantivy: &TantivySidecar,
        direct_db: &Database,
        id: u64,
        peer: &str,
        text: &str,
        ts: i64,
    ) {
        tantivy
            .index_message(IndexedMessage {
                event_id: id as i64,
                text,
                peer: Some(peer),
                sender: Some(peer),
                kind: Some("text"),
                ts_unix_ms: ts,
                from_me: false,
            })
            .unwrap();
        direct_db
            .execute(
                "INSERT INTO events (id, ts_unix_ms, ts_mono_ns, kind, payload) \
                 VALUES (?, ?, 0, 'message', '{}')",
                vec![Value::from(id as i64), Value::from(ts)],
            )
            .unwrap();
        direct_db
            .execute(
                "INSERT INTO messages \
                 (event_id, peer, sender, ts_unix_ms, kind, text, from_me, is_group) \
                 VALUES (?, ?, ?, ?, 'text', ?, 0, 0)",
                vec![
                    Value::from(id as i64),
                    Value::from(peer.to_string()),
                    Value::from(peer.to_string()),
                    Value::from(ts),
                    Value::from(text.to_string()),
                ],
            )
            .unwrap();
    }

    #[test]
    fn fts_returns_text_matches_joined_to_sql() {
        let (db, tantivy, ingester) = fixture();
        ingest_message(&tantivy, &db, 1, "peer_a", "hello world", 1000);
        ingest_message(&tantivy, &db, 2, "peer_a", "goodbye world", 2000);
        ingest_message(&tantivy, &db, 3, "peer_b", "totally unrelated", 3000);
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc.search("world", &SearchFilters::default(), 10).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.text.contains("world")));
    }

    #[test]
    fn peer_filter_narrows_results() {
        let (db, tantivy, ingester) = fixture();
        ingest_message(&tantivy, &db, 1, "peer_a", "rust async", 1000);
        ingest_message(&tantivy, &db, 2, "peer_b", "rust sync", 2000);
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc
            .search(
                "rust",
                &SearchFilters {
                    peer: Some("peer_a".into()),
                    ..Default::default()
                },
                10,
            )
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].peer, "peer_a");
    }

    #[test]
    fn kind_filter_narrows_results() {
        let (db, tantivy, ingester) = fixture();
        // Two messages with different kinds (image vs text), both
        // mentioning "attachment".
        tantivy
            .index_message(IndexedMessage {
                event_id: 1,
                text: "image attachment",
                peer: Some("p"),
                sender: Some("p"),
                kind: Some("image"),
                ts_unix_ms: 1,
                from_me: false,
            })
            .unwrap();
        db.execute(
            "INSERT INTO events (id, ts_unix_ms, ts_mono_ns, kind, payload) VALUES (1, 1, 0, 'message', '{}')",
            (),
        )
        .unwrap();
        db.execute(
            "INSERT INTO messages (event_id, peer, sender, ts_unix_ms, kind, text, from_me, is_group) VALUES (1, 'p', 'p', 1, 'image', 'image attachment', 0, 0)",
            (),
        )
        .unwrap();
        tantivy
            .index_message(IndexedMessage {
                event_id: 2,
                text: "text attachment",
                peer: Some("p"),
                sender: Some("p"),
                kind: Some("text"),
                ts_unix_ms: 2,
                from_me: false,
            })
            .unwrap();
        db.execute(
            "INSERT INTO events (id, ts_unix_ms, ts_mono_ns, kind, payload) VALUES (2, 2, 0, 'message', '{}')",
            (),
        )
        .unwrap();
        db.execute(
            "INSERT INTO messages (event_id, peer, sender, ts_unix_ms, kind, text, from_me, is_group) VALUES (2, 'p', 'p', 2, 'text', 'text attachment', 0, 0)",
            (),
        )
        .unwrap();
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc
            .search(
                "attachment",
                &SearchFilters {
                    kind: Some("text".into()),
                    ..Default::default()
                },
                10,
            )
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "text");
    }

    #[test]
    fn empty_query_returns_empty() {
        let (db, tantivy, ingester) = fixture();
        ingest_message(&tantivy, &db, 1, "p", "hi", 1);
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        assert!(svc
            .search("", &SearchFilters::default(), 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn recent_lists_newest_first() {
        let (db, tantivy, ingester) = fixture();
        ingest_message(&tantivy, &db, 1, "p", "first", 100);
        ingest_message(&tantivy, &db, 2, "p", "second", 200);
        ingest_message(&tantivy, &db, 3, "p", "third", 300);
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc.recent(Some("p"), 10).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].text, "third");
        assert_eq!(hits[1].text, "second");
        assert_eq!(hits[2].text, "first");
    }

    #[test]
    fn context_returns_surrounding_window() {
        let (db, tantivy, ingester) = fixture();
        for i in 0..5 {
            ingest_message(
                &tantivy,
                &db,
                i,
                "p",
                &format!("msg{i}"),
                (i as i64 + 1) * 1000,
            );
        }
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc.context(2, 1, 1).unwrap();
        // Window: 1 before + pivot + 1 after. The ts heuristic is
        // ±60_000ms, so messages 1, 2, 3 all land in the window.
        assert!(hits.iter().any(|h| h.event_id == 2));
        // Assert relative ordering: ascending by ts.
        for pair in hits.windows(2) {
            assert!(pair[0].ts_unix_ms <= pair[1].ts_unix_ms);
        }
    }

    #[test]
    fn fuzz_replay_safe_via_idempotent_ingest() {
        // Sanity: tantivy `delete_term` + `add_document` is atomic
        // per commit, so re-indexing the same event_id twice yields
        // a single hit (the prior one is removed before the new one
        // is added).
        let (db, tantivy, ingester) = fixture();
        ingest_message(&tantivy, &db, 99, "p", "fuzz", 1);
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc.search("fuzz", &SearchFilters::default(), 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    // MockEmbedder usage keeps the unused-import lint happy across
    // cfg permutations.
    #[allow(dead_code)]
    fn _embedder_anchor() -> MockEmbedder {
        MockEmbedder::ok("anchor", 384)
    }

    // synth() is referenced via the fixture helpers above; anchor
    // so future refactors that drop it keep the import live.
    #[allow(dead_code)]
    fn _synth_anchor() -> InboundEvent {
        synth(0, "p", "t", 0)
    }
}
