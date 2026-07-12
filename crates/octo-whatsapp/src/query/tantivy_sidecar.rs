//! Tantivy FTS sidecar — full-text search over `messages.text`.
//!
//! Architecture: one Tantivy `Index` per daemon, on disk under
//! `persist_dir/tantivy/`. Built from the same `InboundEvent` stream
//! that feeds the SQL ingester, so the two derived views stay in
//! sync by construction (same source, same ordering, both use the
//! `events.id` PK as their natural key).
//!
//! Why `simple()` tokenizer: per design discussion (part 3 of the
//! plan doc) we deliberately picked the language-agnostic
//! whitespace + lowercase tokenizer over an English-stemmer. WhatsApp
//! messages are overwhelmingly multilingual — Portuguese, English,
//! Spanish, Italian mixed in the same thread — and a per-language
//! stemmer would silently degrade recall on anything outside its
//! training set. Substring matches at the word level are good enough
//! for v1; semantic search (Phase 1 task 8) handles fuzzy recall.
//!
//! Rebuild on boot: Tantivy writes its own segments, so we don't need
//! to manually replay NDJSON unless the schema version mismatches
//! (Phase 2 task 16). On the live path, the ingest driver calls
//! [`TantivySidecar::index_message`] for each `Message` event.
//!
//! See `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Part 5
//! (Tantivy sidecar).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tantivy::{
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{Schema, SchemaBuilder, Value, FAST, STORED, STRING, TEXT},
    Index, IndexReader, IndexWriter, ReloadPolicy, Searcher, TantivyDocument, Term,
};
use thiserror::Error;

/// One search hit surfaced to the caller.
#[derive(Debug, Clone)]
pub struct TextHit {
    /// Foreign key into `events.id` — caller joins against the SQL
    /// store for full denormalized row.
    pub event_id: i64,
    /// BM25 score, descending. Higher = better match.
    pub score: f32,
}

/// Errors surfaced to callers / tests.
#[derive(Debug, Error)]
pub enum TantivyError {
    #[error("tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("open directory error: {0}")]
    OpenDirectory(#[from] tantivy::directory::error::OpenDirectoryError),
    #[error("open read error: {0}")]
    OpenRead(#[from] tantivy::directory::error::OpenReadError),
    #[error("schema mismatch: field {0} not found")]
    FieldNotFound(&'static str),
    #[error("query parse error: {0}")]
    Parse(#[from] tantivy::query::QueryParserError),
}

/// Sidecar handle — owns the index, reader (cheap to clone), and a
/// single writer guarded by a mutex (Tantivy `IndexWriter` is not
/// `Sync`).
pub struct TantivySidecar {
    index: Index,
    reader: IndexReader,
    schema: Schema,
    writer: Mutex<IndexWriter>,
    /// Resolved on-disk path; for `:memory:` callers (tests) this is
    /// an empty `PathBuf` to keep Debug bounded.
    dir: PathBuf,
}

/// Cheap-to-clone field handles, derived once at construction.
#[derive(Debug, Clone, Copy)]
pub struct TantivyFields {
    pub event_id: tantivy::schema::Field,
    pub text: tantivy::schema::Field,
    pub peer: tantivy::schema::Field,
    pub sender: tantivy::schema::Field,
    pub kind: tantivy::schema::Field,
    pub ts_unix_ms: tantivy::schema::Field,
    pub from_me: tantivy::schema::Field,
}

impl TantivySidecar {
    /// Build or open an on-disk index at `dir`. Creates the directory
    /// tree if missing.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, TantivyError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        let (schema, fields) = build_schema();
        let index = if Index::exists(&tantivy::directory::MmapDirectory::open(&dir)?)? {
            Index::open_in_dir(&dir)?
        } else {
            Index::create_in_dir(&dir, schema.clone())?
        };
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        let writer = index.writer(50_000_000)?;
        // Touch fields to avoid "never read" lint — keep them in a
        // Debug-friendly handle the caller can pass through.
        let _ = fields.event_id;
        Ok(Self {
            index,
            reader,
            schema,
            writer: Mutex::new(writer),
            dir,
        })
    }

    /// In-memory index for tests. No on-disk writes; `reload()` is a
    /// no-op since the reader is fresh per call.
    pub fn in_memory() -> Result<Self, TantivyError> {
        let (schema, fields) = build_schema();
        let index = Index::create_in_ram(schema.clone());
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let writer = index.writer(50_000_000)?;
        let _ = fields.event_id;
        Ok(Self {
            index,
            reader,
            schema,
            writer: Mutex::new(writer),
            dir: PathBuf::new(),
        })
    }

    /// Add or replace one document for `event_id`. Replay-safe: the
    /// `term` delete + `add_document` is atomic per commit.
    pub fn index_message(&self, doc: IndexedMessage<'_>) -> Result<(), TantivyError> {
        let fields = self.fields();
        let mut writer_guard = self.writer.lock().expect("writer mutex poisoned");
        // Delete any existing doc for this event_id first (replay
        // safety: same event indexed twice = same final state).
        let event_id_term = Term::from_field_i64(fields.event_id, doc.event_id);
        writer_guard.delete_term(event_id_term);
        writer_guard.add_document(doc!(
            fields.event_id => doc.event_id,
            fields.text => doc.text,
            fields.peer => doc.peer.unwrap_or(""),
            fields.sender => doc.sender.unwrap_or(""),
            fields.kind => doc.kind.unwrap_or(""),
            fields.ts_unix_ms => doc.ts_unix_ms,
            fields.from_me => if doc.from_me { 1i64 } else { 0i64 },
        ))?;
        writer_guard.commit()?;
        Ok(())
    }

    /// Run a full-text query. Returns up to `limit` hits sorted by
    /// BM25 descending.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<TextHit>, TantivyError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let fields = self.fields();
        let searcher: Searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![fields.text]);
        let parsed = parser.parse_query(query)?;
        let top = searcher.search(&parsed, &TopDocs::with_limit(limit))?;
        let mut hits = Vec::with_capacity(top.len());
        for (score, addr) in top {
            let retrieved: TantivyDocument = searcher.doc(addr)?;
            let event_id = retrieved
                .get_first(fields.event_id)
                .and_then(|v| v.as_i64())
                .ok_or(TantivyError::FieldNotFound("event_id"))?;
            hits.push(TextHit { event_id, score });
        }
        Ok(hits)
    }

    /// Force the reader to pick up the latest committed docs. The
    /// default `OnCommitWithDelay` policy handles this on a timer;
    /// tests call `reload()` for determinism.
    pub fn reload(&self) -> Result<(), TantivyError> {
        self.reader.reload()?;
        Ok(())
    }

    /// Schema handle (kept public for advanced callers like the
    /// search service that wants to build column-aware queries).
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Field handles. Cached at open time — Tantivy fields are
    /// `usize` newtype wrappers, copy-cheap.
    pub fn fields(&self) -> TantivyFields {
        let s = &self.schema;
        TantivyFields {
            event_id: s
                .get_field("event_id")
                .expect("event_id field is part of the built schema"),
            text: s.get_field("text").expect("text field"),
            peer: s.get_field("peer").expect("peer field"),
            sender: s.get_field("sender").expect("sender field"),
            kind: s.get_field("kind").expect("kind field"),
            ts_unix_ms: s.get_field("ts_unix_ms").expect("ts_unix_ms field"),
            from_me: s.get_field("from_me").expect("from_me field"),
        }
    }

    /// Resolved directory. Empty for `:memory:`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl std::fmt::Debug for TantivySidecar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TantivySidecar")
            .field("dir", &self.dir)
            .finish_non_exhaustive()
    }
}

/// Input for [`TantivySidecar::index_message`]. Mirrors only the
/// fields we want to expose to FTS — the SQL store keeps the full
/// denormalized payload.
#[derive(Debug, Clone)]
pub struct IndexedMessage<'a> {
    pub event_id: i64,
    pub text: &'a str,
    pub peer: Option<&'a str>,
    pub sender: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub ts_unix_ms: i64,
    pub from_me: bool,
}

fn build_schema() -> (Schema, TantivyFields) {
    let mut b = SchemaBuilder::new();
    // event_id is indexed so `delete_term` can find existing docs by
    // exact event_id match (replay-safe overwrite). FAST is kept for
    // fast filtering + retrieval by the search service.
    let event_id = b.add_i64_field("event_id", tantivy::schema::INDEXED | STORED | FAST);
    let text = b.add_text_field("text", TEXT);
    let peer = b.add_text_field("peer", STRING);
    let sender = b.add_text_field("sender", STRING);
    let kind = b.add_text_field("kind", STRING);
    let ts_unix_ms = b.add_i64_field("ts_unix_ms", tantivy::schema::INDEXED | STORED | FAST);
    let from_me = b.add_i64_field("from_me", tantivy::schema::INDEXED | STORED);
    let schema = b.build();
    let fields = TantivyFields {
        event_id,
        text,
        peer,
        sender,
        kind,
        ts_unix_ms,
        from_me,
    };
    (schema, fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_index_round_trip() {
        let sidecar = TantivySidecar::in_memory().unwrap();
        sidecar
            .index_message(IndexedMessage {
                event_id: 1,
                text: "hello world from octo",
                peer: Some("peer_a"),
                sender: Some("peer_a"),
                kind: Some("text"),
                ts_unix_ms: 1000,
                from_me: false,
            })
            .unwrap();
        sidecar.reload().unwrap();
        let hits = sidecar.search("hello", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].event_id, 1);
        assert!(hits[0].score > 0.0);
    }

    #[test]
    fn simple_tokenizer_splits_on_punctuation() {
        let sidecar = TantivySidecar::in_memory().unwrap();
        sidecar
            .index_message(IndexedMessage {
                event_id: 1,
                text: "hello, world!",
                peer: None,
                sender: None,
                kind: Some("text"),
                ts_unix_ms: 1,
                from_me: false,
            })
            .unwrap();
        sidecar.reload().unwrap();
        let hits = sidecar.search("hello", 10).unwrap();
        assert_eq!(hits.len(), 1);
        let hits = sidecar.search("world", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn simple_tokenizer_lowercases_but_does_not_stem() {
        let sidecar = TantivySidecar::in_memory().unwrap();
        sidecar
            .index_message(IndexedMessage {
                event_id: 1,
                text: "Running quickly through forests",
                peer: None,
                sender: None,
                kind: Some("text"),
                ts_unix_ms: 1,
                from_me: false,
            })
            .unwrap();
        sidecar.reload().unwrap();
        // Case-insensitive: works.
        assert_eq!(sidecar.search("running", 10).unwrap().len(), 1);
        // Stemming NOT applied: "runs" doesn't match "running".
        assert_eq!(sidecar.search("runs", 10).unwrap().len(), 0);
        // Substring NOT applied: "quick" doesn't match "quickly".
        assert_eq!(sidecar.search("quick", 10).unwrap().len(), 0);
    }

    #[test]
    fn bm25_ranks_better_match_first() {
        let sidecar = TantivySidecar::in_memory().unwrap();
        sidecar
            .index_message(IndexedMessage {
                event_id: 1,
                text: "rust rust rust rust rust rust rust rust rust",
                peer: None,
                sender: None,
                kind: Some("text"),
                ts_unix_ms: 1,
                from_me: false,
            })
            .unwrap();
        sidecar
            .index_message(IndexedMessage {
                event_id: 2,
                text: "rust tutorial for beginners",
                peer: None,
                sender: None,
                kind: Some("text"),
                ts_unix_ms: 2,
                from_me: false,
            })
            .unwrap();
        sidecar.reload().unwrap();
        let hits = sidecar.search("rust", 10).unwrap();
        assert_eq!(hits.len(), 2);
        // The dense doc has higher TF, so higher BM25 (lower IDFs).
        assert_eq!(hits[0].event_id, 1);
        assert_eq!(hits[1].event_id, 2);
    }

    #[test]
    fn empty_query_returns_empty() {
        let sidecar = TantivySidecar::in_memory().unwrap();
        sidecar
            .index_message(IndexedMessage {
                event_id: 1,
                text: "anything",
                peer: None,
                sender: None,
                kind: Some("text"),
                ts_unix_ms: 1,
                from_me: false,
            })
            .unwrap();
        sidecar.reload().unwrap();
        assert!(sidecar.search("", 10).unwrap().is_empty());
        assert!(sidecar.search("   ", 10).unwrap().is_empty());
    }

    #[test]
    fn replay_safe_idempotent_indexing() {
        let sidecar = TantivySidecar::in_memory().unwrap();
        for _ in 0..3 {
            sidecar
                .index_message(IndexedMessage {
                    event_id: 42,
                    text: "same text",
                    peer: None,
                    sender: None,
                    kind: Some("text"),
                    ts_unix_ms: 1,
                    from_me: false,
                })
                .unwrap();
        }
        sidecar.reload().unwrap();
        let hits = sidecar.search("same", 10).unwrap();
        assert_eq!(hits.len(), 1, "replays must collapse on event_id");
    }

    #[test]
    fn persists_across_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let sidecar = TantivySidecar::open(tmp.path()).unwrap();
            sidecar
                .index_message(IndexedMessage {
                    event_id: 99,
                    text: "persisted across reopen",
                    peer: None,
                    sender: None,
                    kind: Some("text"),
                    ts_unix_ms: 1,
                    from_me: false,
                })
                .unwrap();
        }
        let reopened = TantivySidecar::open(tmp.path()).unwrap();
        reopened.reload().unwrap();
        let hits = reopened.search("persisted", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].event_id, 99);
    }
}
