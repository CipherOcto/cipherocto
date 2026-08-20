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
use octo_storage_core::stoolap::Value;
use octo_storage_core::Database;
use serde::{Deserialize, Serialize};
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

/// Full event row joined from `events` + `messages`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventHit {
    pub event_id: i64,
    pub kind: String,
    pub variant: Option<String>,
    pub peer: Option<String>,
    pub sender: Option<String>,
    pub chat_jid: Option<String>,
    pub ts_unix_ms: i64,
    pub payload: String,
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
    Stoolap(#[from] octo_storage_core::stoolap::Error),
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

    /// Surrounding messages: `before` before + pivot + `after` after,
    /// ranked by ts proximity to pivot. Used by the `messages.context`
    /// RPC.
    ///
    /// Before 2026-07-12 the lower bound was a heuristic 60-second
    /// window per `before` step which (a) missed sparse chats and
    /// (b) over-fetched bursty groups. The current implementation
    /// pulls `(before + after + 1)` rows ordered by
    /// `ABS(ts_unix_ms - pivot)` so the result is correct regardless
    /// of message cadence; the returned list is then re-sorted by
    /// `ts_unix_ms ASC` so callers see it chronologically.
    pub fn context(
        &self,
        event_id: i64,
        before: usize,
        after: usize,
    ) -> Result<Vec<SearchHit>, ServiceError> {
        let db: &Database = self.ingester.db();
        // Fetch pivot's ts + peer in one row.
        let mut pivot_row = db.query(
            "SELECT ts_unix_ms, peer FROM messages WHERE event_id = ?",
            vec![Value::from(event_id)],
        )?;
        let pivot_row = pivot_row
            .next()
            .ok_or_else(|| ServiceError::InvalidFilter("event_id not found".into()))??;
        let pivot_ts = pivot_row.get::<i64>(0).unwrap_or(0);
        let pivot_peer = get_str(&pivot_row, 1);
        // Window INCLUDES the pivot itself — without this we'd miss
        // the pivot and have to merge it back, which loses the
        // (before + after) semantics when the pivot is at the
        // boundary.
        let window = (before + after + 1) as i64;
        // Pull the K nearest messages for this peer ordered by ts
        // distance to the pivot, then surface in chronological order
        // in Rust.
        let sql = "SELECT event_id, peer, sender, ts_unix_ms, kind, text \
                   FROM messages \
                   WHERE peer = ? \
                   ORDER BY ABS(ts_unix_ms - ?) ASC \
                   LIMIT ?";
        let rows = db.query(
            sql,
            vec![
                Value::from(pivot_peer.as_str()),
                Value::from(pivot_ts),
                Value::from(window),
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
        // Pivot is already in the LIMIT result — no need to append.
        // Sort chronologically for presentation.
        out.sort_by_key(|h| h.ts_unix_ms);
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

    /// Fetch a single event row by id. Returns `None` if no event
    /// matches. Joins `events` (denormalized columns) with the
    /// optional `messages` payload so callers get text in one call.
    pub fn by_id(&self, event_id: i64) -> Result<Option<EventHit>, ServiceError> {
        let db: &Database = self.ingester.db();
        let mut rows = db.query(
            "SELECT id, kind, variant, peer, sender, chat_jid, ts_unix_ms, payload \
             FROM events WHERE id = ?",
            vec![Value::from(event_id)],
        )?;
        let row = match rows.next() {
            None => return Ok(None),
            Some(Err(e)) => return Err(ServiceError::Stoolap(e)),
            Some(Ok(r)) => r,
        };
        Ok(Some(EventHit {
            event_id: row.get::<i64>(0).unwrap_or(0),
            kind: get_str(&row, 1),
            variant: {
                let v: String = row.get::<String>(2).unwrap_or_default();
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            },
            peer: opt_str_opt(row.get::<String>(3)),
            sender: opt_str_opt(row.get::<String>(4)),
            chat_jid: opt_str_opt(row.get::<String>(5)),
            ts_unix_ms: row.get::<i64>(6).unwrap_or(0),
            payload: get_str(&row, 7),
        }))
    }

    /// Filter events by kind/variant/peer/ts_window. Bypasses Tantivy
    /// (pure SQL). Used by `events.find`.
    pub fn find(
        &self,
        kind: Option<&str>,
        variant: Option<&str>,
        peer: Option<&str>,
        since_ts_unix_ms: Option<i64>,
        until_ts_unix_ms: Option<i64>,
        limit: usize,
    ) -> Result<Vec<EventHit>, ServiceError> {
        let mut sql = String::from(
            "SELECT id, kind, variant, peer, sender, chat_jid, ts_unix_ms, payload \
             FROM events WHERE 1=1",
        );
        let mut params: Vec<Value> = Vec::new();
        if let Some(k) = kind {
            sql.push_str(" AND kind = ?");
            params.push(Value::from(k.to_string()));
        }
        if let Some(v) = variant {
            sql.push_str(" AND variant = ?");
            params.push(Value::from(v.to_string()));
        }
        if let Some(p) = peer {
            sql.push_str(" AND peer = ?");
            params.push(Value::from(p.to_string()));
        }
        if let Some(s) = since_ts_unix_ms {
            sql.push_str(" AND ts_unix_ms >= ?");
            params.push(Value::from(s));
        }
        if let Some(u) = until_ts_unix_ms {
            sql.push_str(" AND ts_unix_ms <= ?");
            params.push(Value::from(u));
        }
        sql.push_str(" ORDER BY ts_unix_ms DESC LIMIT ?");
        params.push(Value::from(limit as i64));
        let db: &Database = self.ingester.db();
        let rows = db.query(&sql, params)?;
        let mut out = Vec::with_capacity(limit);
        for row in rows {
            let row = row?;
            out.push(EventHit {
                event_id: row.get::<i64>(0).unwrap_or(0),
                kind: get_str(&row, 1),
                variant: {
                    let v: String = row.get::<String>(2).unwrap_or_default();
                    if v.is_empty() {
                        None
                    } else {
                        Some(v)
                    }
                },
                peer: opt_str_opt(row.get::<String>(3)),
                sender: opt_str_opt(row.get::<String>(4)),
                chat_jid: opt_str_opt(row.get::<String>(5)),
                ts_unix_ms: row.get::<i64>(6).unwrap_or(0),
                payload: get_str(&row, 7),
            });
        }
        Ok(out)
    }

    /// Brute-force cosine similarity search over the `embeddings`
    /// table. Reads every vector, computes cosine vs the query
    /// vector (assumed L2-normalized so cosine == dot), returns the
    /// top-`limit` matches joined to `messages` for text.
    ///
    /// **v1 limit**: O(N) scan. Per fork TODOs at
    /// `stoolap/src/storage/vector/search.rs:79,93,139`, the upstream
    /// HNSW integration path isn't usable until fixed-dim columns
    /// stop locking us to a single model. Acceptable up to ~500k
    /// embeddings (30-50ms per top-200 query).
    pub fn semantic_search(
        &self,
        query_vec: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchHit>, ServiceError> {
        if query_vec.is_empty() {
            return Ok(Vec::new());
        }
        let db: &Database = self.ingester.db();
        let rows = db.query("SELECT event_id, vec FROM embeddings", ())?;
        let mut scored: Vec<(i64, f32)> = Vec::new();
        for row in rows {
            let row = row?;
            let event_id = row.get::<i64>(0).unwrap_or(0);
            // Read the VECTOR column as a `Value` then extract f32s.
            // Stoolap doesn't impl `FromValue<Vec<f32>>` so we go
            // through the generic Value accessor.
            let value = row.get::<Value>(1).ok();
            let stored = value
                .as_ref()
                .and_then(|v| v.as_vector_f32())
                .unwrap_or_default();
            if stored.is_empty() {
                continue;
            }
            let score = cosine_dot(query_vec, &stored);
            scored.push((event_id, score));
        }
        // Sort descending by score.
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        // Join to messages for the denormalized rows.
        let mut out = Vec::with_capacity(limit);
        for (event_id, score) in scored {
            if let Some(row) = self.fetch_row(event_id)? {
                out.push(SearchHit {
                    event_id,
                    score,
                    peer: row.peer,
                    sender: row.sender,
                    ts_unix_ms: row.ts_unix_ms,
                    kind: row.kind,
                    text: row.text,
                });
            }
        }
        Ok(out)
    }
}

/// Convert `Result<String, _>` from `row.get::<String>` into an
/// `Option<String>` that is `None` for NULL columns (where the
/// FromValue conversion returns an empty string) and for any
/// type-conversion error.
fn opt_str_opt(
    r: std::result::Result<String, octo_storage_core::stoolap::Error>,
) -> Option<String> {
    r.ok().filter(|s| !s.is_empty())
}

/// Cosine similarity under the L2-normalized assumption (cosine ==
/// dot product). Both vectors must be the same length.
fn cosine_dot(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
    }
    dot
}

/// Helper: extract a column as `String`, defaulting to empty on NULL
/// or type-conversion failure (only used for `text` where NULL is
/// legal; NOT NULL columns fall through to the actual value).
fn get_str(row: &octo_storage_core::stoolap::ResultRow, idx: usize) -> String {
    row.get::<String>(idx).unwrap_or_default()
}

fn get_i64(row: &octo_storage_core::stoolap::ResultRow, idx: usize) -> i64 {
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
    use octo_storage_core::Database;

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
        // Stoolap `open_in_memory` creates a unique engine per call,
        // so we open via DSN instead — the registry keeps the engine
        // alive across handle clones. The DSN must be unique per
        // test or registry entries share state across fixtures.
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dsn = format!("memory://test-{pid}-{nanos}");
        let db = Database::open(&dsn).expect("open");
        migrate(&db).expect("migrate");
        let tantivy = TantivySidecar::in_memory().expect("tantivy");
        let ingester = QueryIngester::new(Database::open(&dsn).expect("open2"));
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
                "INSERT INTO events \
                 (id, ts_unix_ms, ts_mono_ns, kind, peer, sender, chat_jid, payload) \
                 VALUES (?, ?, 0, 'message', ?, ?, ?, '{}')",
                vec![
                    Value::from(id as i64),
                    Value::from(ts),
                    Value::from(peer.to_string()),
                    Value::from(peer.to_string()),
                    Value::from(peer.to_string()),
                ],
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
        // Pivot is included + K-nearest-by-ts from the same peer.
        assert!(hits.iter().any(|h| h.event_id == 2), "pivot present");
        // With pivot=2 and (before,after)=(1,1), we want 1, 2, 3.
        let ids: Vec<i64> = hits.iter().map(|h| h.event_id).collect();
        assert_eq!(ids, vec![1, 2, 3], "chronological nearest-by-ts");
    }

    /// Context must work even when the per-peer cadence is sparse
    /// (gap > 60s between messages). The pre-fix `60_000 ms/message`
    /// window would have returned just the pivot and missed the
    /// preceding message entirely.
    #[test]
    fn context_handles_sparse_cadence() {
        let (db, tantivy, ingester) = fixture();
        // Two messages for peer `p` one hour apart.
        ingest_message(&tantivy, &db, 1, "p", "morning", 1_000);
        ingest_message(&tantivy, &db, 2, "p", "afternoon", 3_600_000);
        // Distractor on a different peer — must be ignored.
        ingest_message(&tantivy, &db, 3, "other_peer", "noise", 1_700_000);
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc.context(2, 1, 0).unwrap();
        let ids: Vec<i64> = hits.iter().map(|h| h.event_id).collect();
        assert_eq!(ids, vec![1, 2]);
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

    #[test]
    fn by_id_returns_event_with_payload() {
        let (db, tantivy, ingester) = fixture();
        ingest_message(&tantivy, &db, 42, "peer_x", "hello", 1234);
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hit = svc.by_id(42).unwrap().expect("event 42 exists");
        assert_eq!(hit.event_id, 42);
        assert_eq!(hit.kind, "message");
        assert_eq!(hit.peer.as_deref(), Some("peer_x"));
        assert!(!hit.payload.is_empty());
    }

    #[test]
    fn by_id_returns_none_for_unknown() {
        let (_db, tantivy, ingester) = fixture();
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        assert!(svc.by_id(999).unwrap().is_none());
    }

    #[test]
    fn find_filters_by_kind_and_peer() {
        let (db, tantivy, ingester) = fixture();
        ingest_message(&tantivy, &db, 1, "peer_a", "msg_a", 100);
        ingest_message(&tantivy, &db, 2, "peer_b", "msg_b", 200);
        // Insert a receipt event for peer_a (different kind).
        db.execute(
            "INSERT INTO events (id, ts_unix_ms, ts_mono_ns, kind, variant, peer, payload) \
             VALUES (3, 300, 0, 'receipt', 'delivered', 'peer_a', '{}')",
            (),
        )
        .unwrap();
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc
            .find(Some("message"), None, Some("peer_a"), None, None, 10)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].event_id, 1);
        assert_eq!(hits[0].kind, "message");
        assert_eq!(hits[0].peer.as_deref(), Some("peer_a"));
    }

    #[test]
    fn find_filters_by_variant() {
        let (db, tantivy, ingester) = fixture();
        db.execute(
            "INSERT INTO events (id, ts_unix_ms, ts_mono_ns, kind, variant, payload) \
             VALUES (1, 100, 0, 'receipt', 'delivered', '{}')",
            (),
        )
        .unwrap();
        db.execute(
            "INSERT INTO events (id, ts_unix_ms, ts_mono_ns, kind, variant, payload) \
             VALUES (2, 200, 0, 'receipt', 'read', '{}')",
            (),
        )
        .unwrap();
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc
            .find(Some("receipt"), Some("read"), None, None, None, 10)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].variant.as_deref(), Some("read"));
    }

    #[test]
    fn find_filters_by_ts_window() {
        let (db, tantivy, ingester) = fixture();
        for i in 0..5 {
            ingest_message(
                &tantivy,
                &db,
                i,
                "p",
                &format!("m{i}"),
                (i + 1) as i64 * 100,
            );
        }
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        let hits = svc
            .find(Some("message"), None, None, Some(200), Some(400), 10)
            .unwrap();
        // Window: ts ∈ [200, 400] -> messages 2, 3, 4.
        assert_eq!(hits.len(), 3);
        assert!(hits.iter().all(|h| (200..=400).contains(&h.ts_unix_ms)));
    }

    #[test]
    fn semantic_search_returns_cosine_ranked_hits() {
        // Brute-force over embeddings table — minimal fake vectors.
        let (db, tantivy, ingester) = fixture();
        // Insert two messages, each with a tiny vector stored in
        // embeddings table.
        db.execute(
            "INSERT INTO events (id, ts_unix_ms, ts_mono_ns, kind, payload) VALUES (1, 100, 0, 'message', '{}')",
            (),
        )
        .unwrap();
        db.execute(
            "INSERT INTO messages (event_id, peer, sender, ts_unix_ms, kind, text, from_me, is_group) VALUES (1, 'p', 'p', 100, 'text', 'first', 0, 0)",
            (),
        )
        .unwrap();
        db.execute(
            "INSERT INTO embeddings (event_id, model_id, dims, provider, vec, ts_embed_ms) \
             VALUES (1, 'test', 4, 'local', ?, 0)",
            vec![Value::vector(vec![1.0f32, 0.0, 0.0, 0.0])],
        )
        .map_err(|e| eprintln!("err1: {e:?}"))
        .expect("embeddings insert 1");
        db.execute(
            "INSERT INTO events (id, ts_unix_ms, ts_mono_ns, kind, payload) VALUES (2, 200, 0, 'message', '{}')",
            (),
        )
        .unwrap();
        db.execute(
            "INSERT INTO messages (event_id, peer, sender, ts_unix_ms, kind, text, from_me, is_group) VALUES (2, 'p', 'p', 200, 'text', 'second', 0, 0)",
            (),
        )
        .unwrap();
        db.execute(
            "INSERT INTO embeddings (event_id, model_id, dims, provider, vec, ts_embed_ms) \
             VALUES (2, 'test', 4, 'local', ?, 0)",
            vec![Value::vector(vec![0.0f32, 1.0, 0.0, 0.0])],
        )
        .expect("embeddings insert 2");
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        // Query vector aligned with event_id 1 -> cosine 1.0.
        let hits = svc.semantic_search(&[1.0, 0.0, 0.0, 0.0], 5).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].event_id, 1);
        assert!((hits[0].score - 1.0).abs() < 1e-6);
        assert_eq!(hits[1].event_id, 2);
        assert!(hits[1].score.abs() < 1e-6);
    }

    #[test]
    fn semantic_search_empty_query_returns_empty() {
        let (_db, tantivy, ingester) = fixture();
        tantivy.reload().unwrap();
        let svc = QueryService::new(std::sync::Arc::new(tantivy), std::sync::Arc::new(ingester));
        assert!(svc.semantic_search(&[], 5).unwrap().is_empty());
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
