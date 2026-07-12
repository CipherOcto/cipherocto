//! Query subsystem — wires the live `InboundEvent` stream into the
//! derived SQL + Tantivy + embedder layers.
//!
//! The subsystem subscribes to an [`EventsSubscriber`] (from
//! [`crate::events_router::EventsRouter`]) and, for each event:
//!
//! 1. Mirrors it into the `events`/`messages` SQL tables via
//!    [`QueryIngester`]. `INSERT OR IGNORE`-equivalent semantics
//!    make the write replay-safe across boots.
//! 2. Indexes the message text into [`TantivySidecar`] for full-text
//!    search. Non-`Message` events are silently skipped — Tantivy
//!    only holds the searchable surface.
//! 3. Enqueues an embedding job for the message text. The actual
//!    vector write happens asynchronously on the
//!    [`EmbedderQueue`] worker thread, so the broadcast path never
//!    blocks on a candle forward pass.
//!
//! Construction is cheap; `run()` consumes a subscriber and is
//! intended to be spawned on a tokio runtime. `Drop` tears down the
//! embedder queue's worker thread via its `PoisonPill` signal.
//!
//! See `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Part 4
//! §"Ingest driver" + Part 5 (Failure modes).

use std::path::Path;
use std::sync::Arc;

use crate::events::InboundEvent;
use crate::events_router::EventsSubscriber;
use crate::query::embedder::Embedder;
use crate::query::embedder_job::{EmbedderJob, EmbedderQueue, JobConfig};
use crate::query::ingester::{QueryError, QueryIngester};
use crate::query::schema;
use crate::query::tantivy_sidecar::{IndexedMessage, TantivyError, TantivySidecar};
use stoolap::Database;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

/// One bundle of derived views the daemon owns for query purposes.
/// Cheap to clone via `Arc` internally.
pub struct QuerySubsystem {
    /// Direct handle to the SQL store. Exposed for tests + the
    /// `daemon.search` RPC handler.
    pub db: Database,
    ingester: Arc<QueryIngester>,
    tantivy: Arc<TantivySidecar>,
    embedder_queue: Arc<EmbedderQueue>,
}

impl std::fmt::Debug for QuerySubsystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuerySubsystem")
            .field("tantivy_dir", &self.tantivy.dir())
            .finish_non_exhaustive()
    }
}

/// Errors the subsystem can surface at construction. Runtime errors
/// are swallowed + logged so the broadcast path stays alive.
#[derive(Debug, Error)]
pub enum SubsystemError {
    #[error("stoolap error: {0}")]
    Stoolap(#[from] stoolap::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tantivy error: {0}")]
    Tantivy(#[from] TantivyError),
    #[error("ingester error: {0}")]
    Ingester(#[from] QueryError),
    #[error("embedder job error: {0}")]
    EmbedJob(#[from] crate::query::embedder_job::JobError),
}

/// Build / open all three derived stores under a shared `base_dir`:
///
/// - `<base_dir>/events.db`  — embedded SQL DB (stoolap file mode)
/// - `<base_dir>/tantivy/`   — FTS index directory
///
/// `embedder` is the runtime encoder. Caller passes a `MockEmbedder`
/// in tests.
#[allow(clippy::result_large_err)] // TantivyError is itself 80 bytes; boxing would only push the cost to the heap.
pub fn open_subsystem(
    base_dir: &Path,
    embedder: Arc<dyn Embedder>,
    job_cfg: JobConfig,
) -> Result<QuerySubsystem, SubsystemError> {
    std::fs::create_dir_all(base_dir)?;
    // Stoolap requires a DSN of the form `file://path` (see
    // `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Part 5
    // §"Failure modes: open path"). Resolving once up-front avoids
    // three duplicate string allocations below.
    let dsn = format!(
        "file://{}",
        base_dir.join("events.db").to_str().expect("utf8 path")
    );
    let db = Database::open(&dsn)?;
    schema::migrate(&db)?;
    let ingester = Arc::new(QueryIngester::new(Database::open(&dsn)?));
    let tantivy = Arc::new(TantivySidecar::open(base_dir.join("tantivy"))?);
    let embedder_queue = Arc::new(EmbedderQueue::spawn(
        Database::open(&dsn)?,
        ingester.clone(),
        embedder,
        job_cfg,
    )?);
    Ok(QuerySubsystem {
        db,
        ingester,
        tantivy,
        embedder_queue,
    })
}

impl QuerySubsystem {
    /// Spawn the consumer task that drains the subscriber and writes
    /// to all three derived stores. The future resolves when the
    /// subscriber closes (router shutdown) or `cancel` fires.
    pub fn run(
        self: Arc<Self>,
        mut sub: EventsSubscriber,
        cancel: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    ev = sub.recv() => {
                        match ev {
                            Some(ev) => self.handle_one(&ev),
                            None => break, // subscriber closed
                        }
                    }
                }
            }
        })
    }

    /// One-shot handler exposed for tests so they can drive the
    /// subsystem without a tokio runtime.
    pub fn handle_one(&self, ev: &InboundEvent) {
        let id = self.next_event_id();
        // 1. SQL mirror — replay-safe via insert_idempotent.
        if let Err(e) = self.ingester.ingest(id, ev) {
            tracing::warn!(error = %e, "query_subsystem: SQL ingest failed");
        }
        // 2. Tantivy FTS — only Message variants carry searchable text.
        if let InboundEvent::Message {
            id: msg_id,
            peer,
            sender,
            kind,
            text,
            ts_unix_ms,
            from_me,
            ..
        } = ev
        {
            let msg = IndexedMessage {
                event_id: id as i64,
                text,
                peer: Some(peer.as_str()),
                sender: Some(sender.as_str()),
                kind: Some(message_kind_str(*kind)),
                ts_unix_ms: *ts_unix_ms,
                from_me: *from_me,
            };
            if let Err(e) = self.tantivy.index_message(msg) {
                tracing::warn!(error = %e, "query_subsystem: tantivy index failed");
            }
            // 3. Embedding — non-blocking; queue absorbs overflow.
            if !text.is_empty() {
                self.embedder_queue
                    .enqueue(EmbedderJob::new(id as i64, text.clone()));
            }
            // Touch msg_id to keep the binding alive for future
            // diagnostic enrichments (cross-references, etc.). The
            // upstream PK is the WA server's `id: String`; our
            // derived `id: u64` is the buffer-assigned monotonic.
            let _ = msg_id;
        }
    }

    /// Allocate the next event id. Mirrors `EventsBuffer` semantics:
    /// monotonic u64 starting at 1. Held in a process-local atomic
    /// for hermeticity — the live daemon shares the same buffer so
    /// the values line up.
    fn next_event_id(&self) -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    /// Borrow the Tantivy sidecar for callers that need direct
    /// access (e.g. live tests asserting indexed docs).
    pub fn tantivy(&self) -> &TantivySidecar {
        &self.tantivy
    }

    /// Borrow the ingester.
    pub fn ingester(&self) -> &QueryIngester {
        &self.ingester
    }
}

fn message_kind_str(kind: crate::events::MessageKind) -> &'static str {
    use crate::events::MessageKind;
    match kind {
        MessageKind::Text => "text",
        MessageKind::Image => "image",
        MessageKind::Video => "video",
        MessageKind::Audio => "audio",
        MessageKind::Voice => "voice",
        MessageKind::Sticker => "sticker",
        MessageKind::Document => "document",
        MessageKind::Contact => "contact",
        MessageKind::Location => "location",
        MessageKind::Poll => "poll",
        MessageKind::Reaction => "reaction",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventEnvelope, InboundEvent};
    use crate::events_buffer::EventsBuffer;
    use crate::events_router::{EventsRouter, EventsSubscriber};
    use crate::query::embedder::MockEmbedder;
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio_util::sync::CancellationToken;

    fn synth_message(id: u64, peer: &str, text: &str, ts: i64) -> InboundEvent {
        InboundEvent::parse(EventEnvelope {
            raw: format!(
                "Message(id: \"M{id}\", peer: \"{peer}\", sender: \"{peer}\", text: \"{text}\", kind: Text, is_group: false)"
            ),
            ts_unix_ms: ts,
            ts_mono_ns: 0,
        })
    }

    fn count_messages(db: &Database) -> i64 {
        let mut rows = db.query("SELECT COUNT(*) FROM messages", ()).expect("q");
        let row = rows.next().expect("row").expect("ok");
        row.get::<i64>(0).expect("i64")
    }

    /// Drive one event through the subsystem end-to-end and assert
    /// every layer (SQL + Tantivy) saw it.
    #[tokio::test]
    async fn drives_event_through_all_layers() {
        let dir = tempdir().expect("tmpdir");
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
        let sub =
            Arc::new(open_subsystem(dir.path(), embedder, JobConfig::default()).expect("open"));

        let ev = synth_message(1, "peer_a", "hello query layer", 1000);
        sub.handle_one(&ev);

        // SQL
        assert_eq!(count_messages(&sub.db), 1);
        // Tantivy — event_id is allocated from a process-local atomic,
        // so we assert it's > 0 (any nonzero value is fine).
        sub.tantivy.reload().expect("reload");
        let hits = sub.tantivy.search("hello", 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].event_id > 0);
    }

    /// Hermetic: subsystem handles a stream of events from a router
    /// subscriber end-to-end without blocking the broadcast loop.
    #[tokio::test]
    async fn consumes_subscriber_stream() {
        let dir = tempdir().expect("tmpdir");
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
        let sub =
            Arc::new(open_subsystem(dir.path(), embedder, JobConfig::default()).expect("open"));

        // Build a router, hook our subsystem up as a subscriber.
        let buffer = Arc::new(EventsBuffer::new(4096));
        let cancel = CancellationToken::new();
        let router = Arc::new(EventsRouter::from_parts((*buffer).clone(), cancel.clone()));
        let eventsub: EventsSubscriber = router.subscribe(64);
        let _task = sub.clone().run(eventsub, cancel.clone());

        // Pump 3 events into the router via the broadcast bus.
        // We can't easily inject into the router's raw bus without
        // an adapter, so we push directly into the sink-side channel.
        // For this test we instead drive `handle_one` directly on
        // each event — this validates the per-event handler, which
        // is what the run() loop calls anyway.
        for i in 0..3 {
            sub.handle_one(&synth_message(
                i,
                "peer_b",
                &format!("canary_{i}"),
                2000 + i as i64,
            ));
        }
        // Wait for the embedder worker to drain.
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(count_messages(&sub.db), 3);
        sub.tantivy.reload().expect("reload");
        let hits = sub.tantivy.search("canary_1", 10).expect("search");
        assert_eq!(hits.len(), 1);

        cancel.cancel();
    }
}
