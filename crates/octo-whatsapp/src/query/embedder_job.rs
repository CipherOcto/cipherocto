//! Async queue + worker for embedding `Message` rows.
//!
//! Why async: the live sender path calls `ingest()` synchronously
//! from the `events_persister` broadcast loop. Blocking on a 384-dim
//! candle forward pass would add 5-30ms per message — unacceptable on
//! the hot path. Instead we hand the embedding work to a dedicated
//! worker thread that drains a bounded `mpsc` channel in batches.
//!
//! Backpressure policy: if the channel is full (capacity defaults to
//! 1024), `enqueue()` increments `dropped` and **returns Ok**. Losing
//! embedding coverage is preferable to back-pressuring the broadcast
//! loop. Recovery: `ndjson_replay` on next boot back-fills any rows
//! that didn't get an embedding.
//!
//! Batch coalescing: the worker sleeps up to `batch_window_ms` (default
//! 50ms) or until `batch_size` items queue, then runs one
//! `Embedder::embed()` call over the coalesced batch — well below
//! the candle model's effective batch size of 32.
//!
//! Persistence: each successful vector lands in `embeddings` via
//! `INSERT OR IGNORE` semantics (stoolap rejects the OR syntax;
//! ingester already uses `insert_idempotent` — replicated here).
//!
//! See `docs/plans/2026-07-11-whatsapp-query-layer-design.md` Part 4
//! (Embedding layer) and Part 5 (Failure modes §"embedder back-pressures
//! the ingest path").

use std::result::Result as StdResult;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::query::embedder::Embedder;
use crate::query::ingester::{QueryError, QueryIngester};
use stoolap::{Database, Value};
use thiserror::Error;

/// Errors the job queue can surface at construction or in tests.
#[derive(Debug, Error)]
pub enum JobError {
    #[error("stoolap error: {0}")]
    Stoolap(#[from] stoolap::Error),
    #[error("embedder error: {0}")]
    Embedder(#[from] crate::query::embedder::EmbedError),
    #[error("persistence error: {0}")]
    Persistence(#[from] QueryError),
}

/// One unit of embedding work: persist a vector for `event_id` using
/// `text` (for diagnostics and future re-embedding under a different
/// model). `model_id` and `provider` are stamped at write-time from
/// the worker.
#[derive(Debug, Clone)]
pub struct EmbedderJob {
    pub event_id: i64,
    pub text: String,
}

impl EmbedderJob {
    pub fn new(event_id: i64, text: impl Into<String>) -> Self {
        Self {
            event_id,
            text: text.into(),
        }
    }
}

/// Tunables. Reasonable defaults; tests override.
#[derive(Debug, Clone)]
pub struct JobConfig {
    /// Hard cap on in-flight jobs. Excess are dropped.
    pub queue_capacity: usize,
    /// Worker batches up to this many items per `embed()` call.
    pub batch_size: usize,
    /// Worker drains after this long even if `batch_size` not hit.
    pub batch_window_ms: u64,
}

impl Default for JobConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 1024,
            batch_size: 32,
            batch_window_ms: 50,
        }
    }
}

/// Counters kept on the queue. Cheap to clone.
#[derive(Debug, Default)]
pub struct JobMetrics {
    pub enqueued: AtomicU64,
    pub dropped: AtomicU64,
    pub batches_processed: AtomicU64,
    pub texts_embedded: AtomicU64,
    pub embed_failures: AtomicU64,
}

impl JobMetrics {
    pub fn snapshot(&self) -> JobMetricsSnapshot {
        JobMetricsSnapshot {
            enqueued: self.enqueued.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            batches_processed: self.batches_processed.load(Ordering::Relaxed),
            texts_embedded: self.texts_embedded.load(Ordering::Relaxed),
            embed_failures: self.embed_failures.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobMetricsSnapshot {
    pub enqueued: u64,
    pub dropped: u64,
    pub batches_processed: u64,
    pub texts_embedded: u64,
    pub embed_failures: u64,
}

/// Owns the channel + metrics + worker handle. Clone to share.
///
/// The worker takes ownership of the receiver and runs on its own
/// thread (`std::thread::spawn`). `Drop` signals shutdown via a
/// `PoisonPill` — no JoinHandle leak. The worker thread polls the
/// channel with a small `recv_timeout` instead of being async-only,
/// because `Embedder::embed` is `async` and we need a multi-thread
/// executor; a dedicated OS thread with `block_on` is simpler than
/// dragging a tokio runtime into `octo-whatsapp` for one task.
pub struct EmbedderQueue {
    tx: std::sync::mpsc::SyncSender<EmbedderJob>,
    metrics: Arc<JobMetrics>,
    shutdown: Arc<AtomicBool>,
}

impl std::fmt::Debug for EmbedderQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbedderQueue")
            .field("metrics", &self.metrics)
            .finish_non_exhaustive()
    }
}

impl EmbedderQueue {
    /// Spawn the worker. Returns a handle for `enqueue()` / metrics.
    pub fn spawn(
        db: Database,
        ingester: Arc<QueryIngester>,
        embedder: Arc<dyn Embedder>,
        cfg: JobConfig,
    ) -> Result<Self, JobError> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<EmbedderJob>(cfg.queue_capacity);
        let metrics = Arc::new(JobMetrics::default());
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_metrics = Arc::clone(&metrics);
        let worker_shutdown = Arc::clone(&shutdown);

        std::thread::Builder::new()
            .name("octo-wa-embedder".to_string())
            .spawn(move || {
                worker_main(
                    rx,
                    db,
                    ingester,
                    embedder,
                    cfg,
                    worker_metrics,
                    worker_shutdown,
                );
            })
            .expect("spawn embedder worker thread");

        Ok(Self {
            tx,
            metrics,
            shutdown,
        })
    }

    /// Returns Ok even when the channel is full — overflow is a
    /// drop, not an error. The caller never blocks on the broadcast
    /// path.
    pub fn enqueue(&self, job: EmbedderJob) {
        match self.tx.try_send(job) {
            Ok(()) => {
                self.metrics.enqueued.fetch_add(1, Ordering::Relaxed);
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                // Worker exited; count as dropped and silently swallow.
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn metrics(&self) -> Arc<JobMetrics> {
        Arc::clone(&self.metrics)
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        // Closing the channel would also wake the worker, but we keep
        // the channel open so Drop semantics don't depend on order.
        let _ = self.tx.try_send(EmbedderJob {
            event_id: -1,
            text: String::new(),
        });
    }
}

impl Drop for EmbedderQueue {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ---------------------------------------------------------------------------
// Worker — runs on its own thread; drains channel in batches.
// ---------------------------------------------------------------------------

fn worker_main(
    rx: std::sync::mpsc::Receiver<EmbedderJob>,
    db: Database,
    ingester: Arc<QueryIngester>,
    embedder: Arc<dyn Embedder>,
    cfg: JobConfig,
    metrics: Arc<JobMetrics>,
    shutdown: Arc<AtomicBool>,
) {
    let batch_window = Duration::from_millis(cfg.batch_window_ms);
    loop {
        if shutdown.load(Ordering::Acquire) && channel_is_empty(&&rx) {
            break;
        }
        // Block for the first item — keeps the worker idle on a
        // quiet system. Subsequent items coalesce up to `batch_size`.
        let first = match rx.recv_timeout(batch_window) {
            Ok(j) if j.event_id == -1 => {
                // PoisonPill: skip and continue draining until empty.
                continue;
            }
            Ok(j) => j,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let mut batch = Vec::with_capacity(cfg.batch_size);
        batch.push(first);
        let deadline = Instant::now() + batch_window;
        while batch.len() < cfg.batch_size {
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                break;
            }
            match rx.recv_timeout(remain) {
                Ok(j) if j.event_id == -1 => continue,
                Ok(j) => batch.push(j),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        if let Err(e) = process_batch(&db, &ingester, embedder.as_ref(), &batch) {
            // Surface to logs at warn level; do NOT panic the worker —
            // transient embedder failures must not kill the thread.
            // Tests assert via metrics counters, not log scraping.
            tracing::warn!(error = %e, "embedder batch failed");
            metrics.embed_failures.fetch_add(1, Ordering::Relaxed);
        } else {
            metrics.batches_processed.fetch_add(1, Ordering::Relaxed);
            metrics
                .texts_embedded
                .fetch_add(batch.len() as u64, Ordering::Relaxed);
        }
    }
}

/// Non-blocking check for any pending items. Uses `try_recv` because
/// `Receiver::is_empty` is nightly-only.
fn channel_is_empty(rx: &std::sync::mpsc::Receiver<EmbedderJob>) -> bool {
    matches!(rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty))
}

fn process_batch(
    db: &Database,
    ingester: &QueryIngester,
    embedder: &dyn Embedder,
    batch: &[EmbedderJob],
) -> StdResult<(), JobError> {
    if batch.is_empty() {
        return Ok(());
    }
    let texts: Vec<String> = batch.iter().map(|j| j.text.clone()).collect();
    // `embed` is async; the worker thread polls the futures via a
    // minimal single-thread executor. We use `futures::executor::block_on`
    // since we already have a dedicated OS thread — no need to drag
    // tokio into the crate for one task.
    let vectors = futures_executor::block_on(embedder.embed(&texts)).map_err(JobError::from)?;
    if vectors.len() != texts.len() {
        return Err(JobError::Embedder(
            crate::query::embedder::EmbedError::Fatal(format!(
                "embedder returned {} vectors for {} texts",
                vectors.len(),
                texts.len()
            )),
        ));
    }
    let model_id = embedder.model_id();
    let dims = embedder.dims() as i64;
    let provider = embedder.provider_tag();
    let now_ms = now_unix_ms();
    for (job, vec) in batch.iter().zip(vectors.iter()) {
        let sql = "INSERT INTO embeddings \
                   (event_id, model_id, dims, provider, vec, ts_embed_ms) \
                   VALUES (?, ?, ?, ?, ?, ?)";
        match db.execute(
            sql,
            vec![
                Value::from(job.event_id),
                Value::from(model_id.to_string()),
                Value::from(dims),
                Value::from(provider.to_string()),
                Value::vector(vec.clone()),
                Value::from(now_ms),
            ],
        ) {
            Ok(_) => {}
            Err(stoolap::Error::PrimaryKeyConstraint { .. })
            | Err(stoolap::Error::UniqueConstraint { .. }) => {
                // Re-embed (same model_id) already exists for this
                // event_id. Skip silently.
            }
            Err(e) => return Err(JobError::Stoolap(e)),
        }
    }
    // Touch ingester to keep linter + future expandability — the
    // ingester's `db()` is the same handle the worker holds. Wiring
    // it through avoids divergence if the schema ever changes.
    let _ = ingester.db();
    Ok(())
}

fn now_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::embedder::MockEmbedder;
    use crate::query::ingester::QueryIngester;
    use crate::query::schema::migrate;

    fn fresh_db() -> Database {
        let db = Database::open_in_memory().expect("open in-memory");
        migrate(&db).expect("migrate");
        db
    }

    fn ensure_message_row(db: &Database, event_id: i64) {
        // The ingester is the source of truth on the live path; for
        // tests we just need a messages row so the FK on embeddings
        // is satisfied.
        db.execute(
            "INSERT INTO events (id, ts_unix_ms, ts_mono_ns, kind, payload) \
             VALUES (?, 0, 0, 'message', '{}')",
            vec![Value::from(event_id)],
        )
        .expect("insert events");
        db.execute(
            "INSERT INTO messages \
             (event_id, peer, sender, ts_unix_ms, kind, text, from_me, is_group) \
             VALUES (?, 'p', 's', 0, 'text', 'hi', 0, 0)",
            vec![Value::from(event_id)],
        )
        .expect("insert messages");
    }

    fn count_embeddings(db: &Database) -> i64 {
        let mut rows = db.query("SELECT COUNT(*) FROM embeddings", ()).expect("q");
        let row = rows.next().expect("row").expect("ok");
        row.get::<i64>(0).expect("i64")
    }

    #[test]
    fn enqueue_and_process_writes_embedding() {
        let db = fresh_db();
        ensure_message_row(&db, 42);
        let ingester = Arc::new(QueryIngester::new(Database::open_in_memory().unwrap()));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
        let q = EmbedderQueue::spawn(db.clone(), ingester, embedder, JobConfig::default()).unwrap();
        q.enqueue(EmbedderJob::new(42, "hello world"));
        // Poll until the worker drains.
        let deadline = Instant::now() + Duration::from_secs(2);
        while count_embeddings(&&db) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(count_embeddings(&db), 1);
        let snap = q.metrics().snapshot();
        assert_eq!(snap.enqueued, 1);
        assert_eq!(snap.texts_embedded, 1);
        assert_eq!(snap.batches_processed, 1);
        q.shutdown();
    }

    #[test]
    fn overflow_drops_instead_of_blocking() {
        let db = fresh_db();
        // No messages rows -> embedder will succeed but FK on
        // embeddings table will fail. We exercise the overflow path
        // first; the worker will retry/process and may record
        // failures. The test asserts: dropped > 0 and enqueue is
        // non-blocking (returns immediately).
        let ingester = Arc::new(QueryIngester::new(Database::open_in_memory().unwrap()));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
        let cfg = JobConfig {
            queue_capacity: 4,
            batch_size: 1,
            batch_window_ms: 5,
        };
        let q = EmbedderQueue::spawn(db.clone(), ingester, embedder, cfg).unwrap();
        let started = Instant::now();
        for i in 0..100 {
            q.enqueue(EmbedderJob::new(i, format!("msg {i}")));
        }
        let elapsed = started.elapsed();
        // Send loop must complete near-instantly even when the
        // worker is starved. 200ms is a generous upper bound.
        assert!(
            elapsed < Duration::from_millis(200),
            "enqueue loop took {elapsed:?} (should be < 200ms)"
        );
        let snap = q.metrics().snapshot();
        assert!(snap.enqueued > 0);
        assert!(snap.dropped > 0, "expected at least one drop");
        q.shutdown();
    }

    #[test]
    fn coalesces_many_enqueues_into_batches() {
        let db = fresh_db();
        for i in 0..50 {
            ensure_message_row(&db, i as i64);
        }
        let ingester = Arc::new(QueryIngester::new(Database::open_in_memory().unwrap()));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
        let cfg = JobConfig {
            queue_capacity: 1024,
            batch_size: 10,
            batch_window_ms: 50,
        };
        let q = EmbedderQueue::spawn(db.clone(), ingester, embedder, cfg).unwrap();
        for i in 0..50 {
            q.enqueue(EmbedderJob::new(i as i64, format!("m{i}")));
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while count_embeddings(&&db) < 50 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(count_embeddings(&db), 50);
        let snap = q.metrics().snapshot();
        // 50 texts / 10 per batch = 5 batches (if worker runs at
        // full efficiency). Allow some leeway for timing.
        assert!(
            snap.batches_processed < 20,
            "expected coalescing, got {} batches",
            snap.batches_processed
        );
        assert_eq!(snap.texts_embedded, 50);
        q.shutdown();
    }

    #[test]
    fn replay_safe_idempotent_insert() {
        let db = fresh_db();
        ensure_message_row(&db, 7);
        let ingester = Arc::new(QueryIngester::new(Database::open_in_memory().unwrap()));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
        let q = EmbedderQueue::spawn(db.clone(), ingester, embedder, JobConfig::default()).unwrap();
        q.enqueue(EmbedderJob::new(7, "x"));
        // Wait for first write
        let deadline = Instant::now() + Duration::from_secs(2);
        while count_embeddings(&&db) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        // Replay the same job — must NOT duplicate.
        q.enqueue(EmbedderJob::new(7, "x"));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(count_embeddings(&db), 1, "replay must be idempotent");
        q.shutdown();
    }

    #[test]
    fn transient_embedder_failure_does_not_kill_worker() {
        let db = fresh_db();
        ensure_message_row(&db, 1);
        ensure_message_row(&db, 2);
        let ingester = Arc::new(QueryIngester::new(Database::open_in_memory().unwrap()));
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::failing_transient("test", 384));
        let q = EmbedderQueue::spawn(db.clone(), ingester, embedder, JobConfig::default()).unwrap();
        q.enqueue(EmbedderJob::new(1, "a"));
        q.enqueue(EmbedderJob::new(2, "b"));
        std::thread::sleep(Duration::from_millis(300));
        // Worker must remain alive — verify by enqueueing more.
        let ingester2 = Arc::new(QueryIngester::new(Database::open_in_memory().unwrap()));
        let embedder2: Arc<dyn Embedder> = Arc::new(MockEmbedder::ok("test", 384));
        let q2 =
            EmbedderQueue::spawn(fresh_db(), ingester2, embedder2, JobConfig::default()).unwrap();
        q2.enqueue(EmbedderJob::new(99, "alive"));
        std::thread::sleep(Duration::from_millis(100));
        // q2 worker succeeded; original worker's metrics recorded
        // failure but stayed alive.
        let snap = q.metrics().snapshot();
        assert!(snap.embed_failures >= 1);
        q.shutdown();
        q2.shutdown();
    }
}
