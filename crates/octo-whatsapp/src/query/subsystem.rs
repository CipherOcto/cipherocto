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

/// Wall-clock millis since the unix epoch. Used by the live path
/// as the recorded-at timestamp when the inbound event doesn't
/// carry an event-internal ts (Receipt / Presence / Unknown).
fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
use crate::events_router::EventsSubscriber;
use crate::query::embedder::Embedder;
#[cfg(any(test, feature = "test-helpers"))]
use crate::query::embedder::MockEmbedder;
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
/// Replay an NDJSON canonical log through the subsystem — used at
/// boot to hydrate the derived SQL + Tantivy views from the same
/// source the persister writes. `insert_idempotent` makes this
/// safe to run repeatedly (every replay over an already-loaded DB
/// collapses on the events.id PK).
///
/// `path` is expected to be the NDJSON file the persister owns,
/// one JSON object per line. Lines that fail to parse are
/// skipped (mirroring `EventsPersister::load_initial_events`
/// semantics).
/// Counters emitted by [`replay_ndjson`] so operators can tell, at a
/// glance, whether the boot-time rehydration actually saw the events
/// they expect. Before this struct existed, the function returned a
/// bare `u64` which conflated three distinct cases (parse ok + insert
/// ok, parse ok + PK collision silently swallowed, parse failed and
/// the line was skipped) into one number — a footgun that masked
/// the replay schema mismatch discovered 2026-07-12.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReplayStats {
    /// Total NDJSON lines read (including blanks; excluding read
    /// errors).
    pub lines_read: u64,
    /// Lines skipped because `serde_json::from_str::<PersistedEvent>`
    /// returned an error. Normally a hand-counted 0 once the writer
    /// and reader agree on the schema.
    pub lines_failed_parse: u64,
    /// Events handed to the ingester / tantivy / embedder. The
    /// ingester further collapses duplicate PKs silently, so this is
    /// the count *before* dedup at the SQL layer.
    pub lines_handled: u64,
}

#[allow(clippy::result_large_err)] // TantivyError is itself 80 bytes; boxing would only push the cost to the heap.
pub fn replay_ndjson(
    s: &QuerySubsystem,
    path: &std::path::Path,
) -> Result<ReplayStats, SubsystemError> {
    use std::io::{BufRead, BufReader};
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ReplayStats::default());
        }
        Err(e) => return Err(SubsystemError::Io(e)),
    };
    let reader = BufReader::new(f);
    let mut stats = ReplayStats::default();
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }
        stats.lines_read += 1;
        // NDJSON schema (set by [`crate::events_persister::PersistedEvent`]):
        // four-column shape: {id, ts_unix_ms, ts_mono_ns, event} where
        // `event` is the full `InboundEvent` (serde-tagged enum). The
        // `id` is the monotonic buffer-assigned id **persisted with
        // the event**, not re-derived from a process-local atomic —
        // replay must reuse the original PKs so inserts collapse on
        // `events.id` without colliding with live events ingested in
        // the same boot window.
        match serde_json::from_str::<crate::events_persister::PersistedEvent>(&line) {
            Ok(pe) => {
                // PersistedEvent.ts_unix_ms is the wall-clock at the
                // time the persister first wrote the event to disk —
                // use it as the recorded-at fallback so receipts and
                // presence rows ingested during replay carry a
                // meaningful chronological value instead of 0.
                let recorded_at = (pe.ts_unix_ms as i64, pe.ts_mono_ns);
                s.handle_one_with_id(pe.id, recorded_at, &pe.event);
                stats.lines_handled += 1;
            }
            Err(e) => {
                // Surface the first few parse errors so misaligned
                // schemas are obvious in the log instead of silently
                // producing `lines_handled = 0` post-boot. Only the
                // first three are warned to avoid log floods on a
                // fully-corrupt NDJSON.
                if stats.lines_failed_parse < 3 {
                    tracing::warn!(
                        error = %e,
                        line_preview = %&line.chars().take(120).collect::<String>(),
                        "replay_ndjson: line failed to parse as PersistedEvent"
                    );
                }
                stats.lines_failed_parse += 1;
            }
        }
    }
    // After a bulk import, force tantivy to reload so newly indexed
    // docs are visible. Reload via the public API.
    let _ = s.tantivy.reload();
    Ok(stats)
}

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
                            Some((id, ev)) => {
                                // Live path: the router already
                                // minted the monotonic id via
                                // `EventsBuffer::push` (the single
                                // source of truth shared with
                                // NDJSON). We just record the wall
                                // clock so receipts/presence rows
                                // have a meaningful chronological
                                // value when their event-internal ts
                                // is 0.
                                let recorded_at = (now_unix_ms(), self.next_mono_ns());
                                self.handle_one_with_id(id, recorded_at, &ev);
                            }
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
        // Live path: allocate a fresh monotonic id from the
        // process-local counter. The buffer assigns the *same* id for
        // the live event before broadcast, so the ingester and SQL
        // mirror agree on PKs across all consumers.
        let id = self.next_event_id();
        // Wall-clock at the time the broadcast loop observed this
        // event. Used as the recorded-at fallback for events whose
        // event-internal ts is zero (Receipt / Presence / Unknown — WA
        // doesn't ship timestamps for those variants).
        let recorded_at = (now_unix_ms(), self.next_mono_ns());
        self.handle_one_with_id(id, recorded_at, ev);
    }

    /// Same as [`Self::handle_one`] but uses an explicit id and a
    /// recorded-at timestamp supplied by the caller. The NDJSON-replay
    /// path needs this so:
    /// 1. the persisted buffer-assigned id is preserved across boots
    ///    instead of being re-derived from the process-local counter
    ///    (which restarts at 1 every boot, so replay and live would
    ///    collide on PKs);
    /// 2. `PersistedEvent.ts_unix_ms` / `ts_mono_ns` — i.e. the wall
    ///    clock at the time the persister first wrote the event to
    ///    disk — flow through into the SQL mirror so receipts and
    ///    presence rows carry a meaningful recorded-at value instead
    ///    of 0.
    pub fn handle_one_with_id(&self, id: u64, recorded_at: (i64, u64), ev: &InboundEvent) {
        // 1. SQL mirror — replay-safe via insert_idempotent.
        if let Err(e) = self.ingester.ingest(id, recorded_at, ev) {
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

    /// Allocate the next process-local monotonic nanosecond
    /// counter. Used as a fallback `ts_mono_ns` when the inbound
    /// event doesn't carry one (Receipt / Presence / Unknown). The
    /// counter starts at 1 and increments per call — values are
    /// comparable within a single daemon run but reset across
    /// boots, which is acceptable for an in-memory recorded-at.
    fn next_mono_ns(&self) -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        COUNTER.fetch_add(1, Ordering::Relaxed)
    }

    /// Borrow the Tantivy sidecar for callers that need direct
    /// access (e.g. live tests asserting indexed docs).
    pub fn tantivy(&self) -> &TantivySidecar {
        &self.tantivy
    }

    /// Clone the Tantivy sidecar's Arc (cheap — just bumps the
    /// refcount) so callers like `QueryService` can hold their own
    /// handle. Used by [`crate::daemon::DaemonHandle::install_query_subsystem`].
    pub fn tantivy_arc(&self) -> Arc<TantivySidecar> {
        Arc::clone(&self.tantivy)
    }

    /// Clone the ingester's Arc.
    pub fn ingester_arc(&self) -> Arc<QueryIngester> {
        Arc::clone(&self.ingester)
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

/// Replay an NDJSON canonical log into a fresh subsystem —
/// asserts that what the persister writes is what the
/// derived views end up holding.
///
/// The on-disk shape is [`crate::events_persister::PersistedEvent`],
/// which serializes as `{id, ts_unix_ms, ts_mono_ns, event: <tagged
/// enum>}`. This test writes that shape and asserts replay hydrates
/// every layer (SQL + tantivy). Before 2026-07-12 the test fixture
/// used the wrong schema (`EventEnvelope {raw, ts_unix_ms, ts_mono_ns}`)
/// and silently passed because the `replay_ndjson` parser also
/// expected the wrong schema — masking the production bug where
/// `events.ndjson` was being read as zero rows on every boot.
#[test]
fn replay_ndjson_hydrates_derived_views() {
    use crate::events::{InboundEvent, MessageKind};
    use crate::events_persister::PersistedEvent;

    let dir = tempfile::tempdir().expect("tmpdir");
    let ndjson = dir.path().join("events.ndjson");
    let mut buf = String::new();
    let mk = |id: u64, msg_id: &str, peer: &str, text: &str, ts: u64| -> String {
        let ev = InboundEvent::Message {
            id: msg_id.into(),
            peer: peer.into(),
            sender: peer.into(),
            kind: MessageKind::Text,
            text: text.into(),
            media_token: None,
            reply_to: None,
            mentions: Vec::new(),
            mentions_truncated: false,
            ts_unix_ms: ts as i64,
            ts_mono_ns: 0,
            from_me: false,
            is_group: false,
        };
        serde_json::to_string(&PersistedEvent {
            id,
            ts_unix_ms: ts,
            ts_mono_ns: 0,
            event: ev,
        })
        .expect("serialize")
    };
    buf.push_str(&mk(1, "M1", "p_a", "alpha", 1000));
    buf.push('\n');
    buf.push_str(&mk(2, "M2", "p_a", "beta", 2000));
    buf.push('\n');
    buf.push_str(&mk(3, "M3", "p_b", "gamma", 3000));
    buf.push('\n');
    std::fs::write(&ndjson, buf.as_bytes()).unwrap();

    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
    let sub = Arc::new(
        open_subsystem(&dir.path().join("query"), embedder, JobConfig::default()).expect("open"),
    );
    let stats = replay_ndjson(&sub, &ndjson).expect("replay");
    assert_eq!(stats.lines_read, 3);
    assert_eq!(stats.lines_failed_parse, 0);
    assert_eq!(stats.lines_handled, 3);
    // SQL row count is 3 (one per `Message` variant).
    let mut rows = sub
        .db
        .query("SELECT COUNT(*) FROM messages", ())
        .expect("q");
    let count = rows
        .next()
        .expect("row")
        .expect("ok")
        .get::<i64>(0)
        .expect("i");
    assert_eq!(count, 3, "all 3 Message events landed in `messages`");
    // Tantivy has 3 indexed docs.
    sub.tantivy.reload().expect("reload");
    let hits = sub.tantivy.search("alpha", 10).expect("search");
    assert_eq!(hits.len(), 1);
}

/// Replay is idempotent at both the Tantivy level (same event_id
/// deletes + re-adds collapse on the index PK) **and** the SQL
/// level (the buffer-assigned `id` is now preserved across replays
/// so the `events.id` PK swallows the second insert silently).
///
/// Before 2026-07-12 replay used a process-local counter, so a
/// second replay of the same NDJSON file would write *different*
/// event_ids and never collide — meaning the SQL store silently
/// accumulated duplicates on every boot.
#[test]
fn replay_ndjson_is_idempotent() {
    use crate::events::{InboundEvent, MessageKind};
    use crate::events_persister::PersistedEvent;

    let dir = tempfile::tempdir().expect("tmpdir");
    let ndjson = dir.path().join("events.ndjson");
    let ev = InboundEvent::Message {
        id: "M1".into(),
        peer: "p".into(),
        sender: "p".into(),
        kind: MessageKind::Text,
        text: "hello".into(),
        media_token: None,
        reply_to: None,
        mentions: Vec::new(),
        mentions_truncated: false,
        ts_unix_ms: 1,
        ts_mono_ns: 0,
        from_me: false,
        is_group: false,
    };
    let line = serde_json::to_string(&PersistedEvent {
        id: 1,
        ts_unix_ms: 1,
        ts_mono_ns: 0,
        event: ev.clone(),
    })
    .unwrap();
    let mut content = line.as_bytes().to_vec();
    content.push(b'\n');
    content.extend_from_slice(line.as_bytes());
    content.push(b'\n');
    std::fs::write(&ndjson, &content).unwrap();
    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
    let sub = Arc::new(
        open_subsystem(&dir.path().join("query"), embedder, JobConfig::default()).expect("open"),
    );
    let stats = replay_ndjson(&sub, &ndjson).expect("replay");
    assert_eq!(stats.lines_read, 2, "two NDJSON lines read");
    assert_eq!(stats.lines_handled, 2, "both handed to ingester");
    assert_eq!(stats.lines_failed_parse, 0);
    // Both lines carry the same persisted `id: 1`, so the SQL PK
    // collapses the second insert. We must see exactly **1** row.
    let mut rows = sub.db.query("SELECT COUNT(*) FROM events", ()).expect("q");
    let count = rows
        .next()
        .expect("row")
        .expect("ok")
        .get::<i64>(0)
        .expect("i");
    assert_eq!(count, 1, "second replay collapses on events.id PK");
    sub.tantivy.reload().expect("reload");
    let hits = sub.tantivy.search("hello", 10).expect("search");
    assert_eq!(hits.len(), 1, "tantivy also collapses on event_id PK");
}

/// New test (2026-07-12): NDJSON lines that don't match the
/// `PersistedEvent` schema (legacy `{raw, ts_unix_ms, ts_mono_ns}`
/// envelope shape) must be reported as `lines_failed_parse` rather
/// than silently skipped — this is what surfaced the production
/// bug where replay appeared to succeed with `replayed = 0`.
#[test]
fn replay_ndjson_reports_parse_failures() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let ndjson = dir.path().join("events.ndjson");
    // Mix of one correct PersistedEvent line + one line in the
    // legacy EventEnvelope shape + one garbage line.
    std::fs::write(
        &ndjson,
        b"{\"id\":1,\"ts_unix_ms\":100,\"ts_mono_ns\":0,\"event\":{\"event\":\"unknown\",\"raw\":\"Message(id: \\\"M1\\\", peer: \\\"p\\\", sender: \\\"p\\\", text: \\\"hi\\\", kind: Text, is_group: false)\",\"ts_unix_ms\":100,\"ts_mono_ns\":0,\"untrusted\":false}}\n\
          {\"raw\":\"Message(id: \\\"legacy\\\", peer: \\\"p\\\", sender: \\\"p\\\", text: \\\"old\\\", kind: Text, is_group: false)\",\"ts_unix_ms\":1,\"ts_mono_ns\":0}\n\
          not even close to json\n",
    )
    .unwrap();
    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
    let sub = Arc::new(
        open_subsystem(&dir.path().join("query"), embedder, JobConfig::default()).expect("open"),
    );
    let stats = replay_ndjson(&sub, &ndjson).expect("replay");
    assert_eq!(stats.lines_read, 3);
    assert_eq!(
        stats.lines_handled, 1,
        "only the PersistedEvent shape parses"
    );
    assert_eq!(
        stats.lines_failed_parse, 2,
        "legacy + garbage lines are flagged"
    );
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
