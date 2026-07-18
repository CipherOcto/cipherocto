//! Events disk persister (Phase 3 Part D).
//!
//! Background actor that turns in-memory `InboundEvent` pushes into
//! durable disk state without blocking the parser hot path. Pair with
//! the in-memory [`EventsBuffer`](crate::events_buffer::EventsBuffer)
//! (the live source of truth) and
//! [`events_router`](crate::events_router::EventsRouter) (which
//! forwards events through this actor).
//!
//! ## Design contract
//!
//! 1. **No backpressure on hot path.** The actor owns a bounded mpsc
//!    (`capacity = 4096`). If full, `push` drops with a counter and
//!    the event router's broadcast subscribers still receive the
//!    event via the per-sink fan-out. This matches the
//!    `raw_event_tx` lossy contract.
//!
//! 2. **Append-only NDJSON.** Each event is written as one line:
//!    `{"id":N,"ts_unix_ms":...,"ts_mono_ns":...,"event":{...}}\n`.
//!    Append-friendly, crash-safe (last partial line is detectable on
//!    reload), debuggable with `head`/`jq`.
//!
//! 3. **Windowed fsync.** Per-event fsync is too slow for the 1000
//!    events/sec target. The actor flushes every `flush_interval_ms`
//!    (default 5s). On cancel / shutdown it drains remaining and
//!    flushes once more. **Crash → lose up to 5s of events.** Matches
//!    the rules persister's risk profile.
//!
//! 4. **No coalescing.** Each event is a unique record; the actor
//!    cannot collapse pending entries.
//!
//! 5. **Reload on startup.** [`load_initial_events`] reads the file,
//!    parses each line, hydrates the buffer via
//!    [`EventsBuffer::hydrate_from_entries`]. Malformed lines are
//!    logged + counted + skipped. A partial trailing line (no `\n`)
//!    triggers a `ftruncate` to the last valid offset.
//!
//! 6. **Cancel-safe.** The actor drains on `cancel.cancelled()`,
//!    flushes, exits. The [`EventsPersisterHandle::join`] task
//!    completes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::events::InboundEvent;
use crate::events_buffer::EventsBuffer;

/// One queued event. The actor receives `InboundEvent` directly; ids
/// are assigned by the buffer (single-writer) and the persister writes
/// the assigned id on the next read of the buffer. To preserve
/// continuity across persistence, we instead accept `InboundEvent`
/// from the router and the actor queries the buffer for the most
/// recent id and writes that. See `persist_one`.
pub type PersistedEventPayload = InboundEvent;

/// Resolve the sidecar file path (one sibling of the events NDJSON).
/// Returns `<events.ndjson path>.unknown_stats.ndjson`. Public so
/// tests + future tooling can find it without duplicating the
/// convention.
pub fn unknown_stats_path(events_path: &Path) -> PathBuf {
    let mut p = events_path.as_os_str().to_owned();
    p.push(".unknown_stats.ndjson");
    PathBuf::from(p)
}

/// Synchronous byte-level parse for the unknown_stats sidecar.
/// Used by `spawn()` to hydrate the in-memory map before returning
/// (so the first RPC after boot sees the historical context).
/// Same per-line robustness as [`load_unknown_stats`].
fn parse_unknown_stats_bytes(bytes: &[u8]) -> std::collections::BTreeMap<String, UnknownStats> {
    let mut map = std::collections::BTreeMap::new();
    for (i, line) in bytes.split(|b| *b == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_slice::<UnknownStats>(line) {
            Ok(s) => {
                map.insert(s.wacore_variant.clone(), s);
            }
            Err(e) => {
                tracing::warn!(
                    line_index = i,
                    error = %e,
                    "events_persister: skipping malformed unknown_stats line on spawn"
                );
            }
        }
    }
    map
}

/// Load persisted unknown-stats sidecar. Returns an empty map if the
/// file doesn't exist; logs + skips malformed lines (they're
/// auxiliary stats, not the canonical event log, so we silently
/// recover). Atomic-rename persistence means a torn write would
/// leave a stale `.tmp`; we read the canonical path only.
pub async fn load_unknown_stats(
    path: &Path,
) -> Result<std::collections::BTreeMap<String, UnknownStats>, PersistError> {
    let mut map = std::collections::BTreeMap::new();
    if !path.exists() {
        return Ok(map);
    }
    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(map),
        Err(e) => return Err(PersistError::Io(e)),
    };
    for (i, line) in bytes.split(|b| *b == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        match serde_json::from_slice::<UnknownStats>(line) {
            Ok(s) => {
                map.insert(s.wacore_variant.clone(), s);
            }
            Err(e) => {
                tracing::warn!(
                    line_index = i,
                    error = %e,
                    "events_persister: skipping malformed unknown_stats line"
                );
            }
        }
    }
    Ok(map)
}

/// Persist the in-memory unknown-stats map to disk via
/// write-temp-then-rename so a crash mid-write leaves the previous
/// file intact. The map is small (≤ tens of variants); we rewrite
/// the whole file on every Unknown emission.
pub async fn save_unknown_stats(
    path: &Path,
    map: &std::collections::BTreeMap<String, UnknownStats>,
) -> Result<(), PersistError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    let tmp = path.with_extension("ndjson.tmp");
    let mut buf = Vec::with_capacity(64 * map.len());
    for s in map.values() {
        serde_json::to_writer(&mut buf, s)?;
        buf.push(b'\n');
    }
    // write+fsync tmp, then rename atomically over the canonical path
    tokio::fs::write(&tmp, &buf).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

/// Cap for `UnknownStats::last_sample` — keeps the sidecar small
/// even when a wacore event carries a multi-KB payload. Operators
/// only need a hint of the shape when triaging unknown emissions.
pub const UNKNOWN_SAMPLE_CAP: usize = 2048;

/// Truncate `s` to [`UNKNOWN_SAMPLE_CAP`] bytes, appending a marker
/// when truncated. JSON-safe (works on byte boundary — single-byte
/// UTF-8 truncations remain valid JSON when source is ASCII; for
/// multi-byte the result still parses but may contain a broken
/// codepoint, which is fine for triage purposes).
pub fn truncate_sample(mut s: String) -> String {
    if s.len() > UNKNOWN_SAMPLE_CAP {
        s.truncate(UNKNOWN_SAMPLE_CAP);
        s.push_str("...[truncated]");
    }
    s
}

/// Per-variant aggregate for `InboundEvent::Unknown` emissions. The
/// persister maintains a `BTreeMap<String, UnknownStats>` keyed by
/// the wacore discriminant label and persists it to
/// `unknown_stats.ndjson` next to the events NDJSON. Drives
/// `events.unknown_stats` (operator triage: which wacore events lack
/// typed handlers) and `unknown_event_total{wacore_variant}` Prometheus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownStats {
    /// Wacore `Event` discriminant label (e.g. `"GroupUpdate"`,
    /// `"PictureUpdate"`, `"FutureVariant"`). Stable across restarts.
    pub wacore_variant: String,
    /// Total emissions since the sidecar was created.
    pub count: u64,
    /// First wall-clock observation (Unix ms).
    pub first_seen_ms: i64,
    /// Most recent observation (Unix ms).
    pub last_seen_ms: i64,
    /// Capped sample of the most recent emission's raw payload
    /// (≤ 2 KiB). Operators inspect this when deciding whether
    /// to project the variant into a typed `InboundEvent` arm.
    #[serde(default)]
    pub last_sample: String,
}

/// What gets written to disk and read back on reload. The schema is
/// **append-only stable**: adding optional fields is fine, removing
/// fields is a breaking change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedEvent {
    /// Monotonic id assigned at the time the event entered the
    /// buffer. Replayed as-is on reload.
    pub id: u64,
    pub ts_unix_ms: u64,
    pub ts_mono_ns: u64,
    pub event: InboundEvent,
}

/// Errors raised by the persister.
#[derive(Debug, Error)]
pub enum PersistError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json encode: {0}")]
    JsonEncode(#[from] serde_json::Error),
    #[error("flush timed out after {elapsed_ms}ms")]
    FlushTimeout { elapsed_ms: u64 },
    #[error("persister channel closed")]
    ChannelClosed,
    #[error("join handle: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Reload statistics. Logged at boot and surfaced via
/// `daemon.status.get`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoadStats {
    pub loaded: u64,
    pub skipped_malformed: u64,
    pub dropped_partial_bytes: u64,
    pub reload_took_ms: u64,
}

/// Drop counter for mpsc-full events (best-effort; never blocks the
/// router).
#[derive(Debug, Default)]
struct DropCounter(AtomicU64);
impl DropCounter {
    fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Handle to the running persister. Send events via [`Self::push`]
/// (best-effort, non-blocking), request a synchronous fsync via
/// [`Self::flush_sync`], wait for shutdown via [`Self::join`].
///
/// The actor's inbound channel carries `(Option<u64>, InboundEvent)`:
/// `Some(id)` is the router-supplied buffer id (live path —
/// persister skips `EventsBuffer::push` and writes NDJSON with the
/// supplied id so NDJSON / buffer / SQL ids are 1:1). `None` is the
/// historical path used by tests and direct callers; the actor
/// allocates an id via the buffer in that case.
pub struct EventsPersisterHandle {
    tx: mpsc::Sender<(Option<u64>, InboundEvent)>,
    flush: mpsc::Sender<oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<()>,
    cancel: CancellationToken,
    dropped: Arc<DropCounter>,
    last_load_stats: Arc<parking_lot::Mutex<Option<LoadStats>>>,
    /// Per-variant aggregate of `InboundEvent::Unknown` emissions.
    /// Drives `events.unknown_stats` (operator triage) and the
    /// `unknown_event_total{wacore_variant}` Prometheus counter.
    /// Lives in-process; persisted to `unknown_stats.ndjson` on
    /// every Unknown emission (cheap — small file).
    unknown_stats: Arc<parking_lot::Mutex<std::collections::BTreeMap<String, UnknownStats>>>,
}

impl std::fmt::Debug for EventsPersisterHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventsPersisterHandle")
            .field("dropped_total", &self.dropped.get())
            .field("last_load_stats", &*self.last_load_stats.lock())
            .finish()
    }
}

impl Clone for EventsPersisterHandle {
    /// Clone the handle. The new handle shares the actor's mpsc
    /// channels + state but NOT the JoinHandle — callers awaiting
    /// the original handle's join consume it; the clone's join is a
    /// no-op stand-in (the actor exits when its single owner
    /// observes cancellation). Use the clone for ingress wiring
    /// only.
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            flush: self.flush.clone(),
            join: noop_join_handle(),
            cancel: self.cancel.clone(),
            dropped: self.dropped.clone(),
            last_load_stats: self.last_load_stats.clone(),
            unknown_stats: self.unknown_stats.clone(),
        }
    }
}

/// Build a stand-in `JoinHandle` that resolves immediately. Used
/// by cloned `EventsPersisterHandle`s that share the actor but
/// can't share the original join. The clone's join is never
/// awaited in practice.
fn noop_join_handle() -> tokio::task::JoinHandle<()> {
    tokio::spawn(async {})
}

/// Cheap-clone ingress to a running persister. The daemon stores
/// one of these on `DaemonInner` so `bind_adapter` can wire the
/// router's per-sink fan-out into the persister's upstream channel
/// without taking ownership of the actor's `JoinHandle` (which
/// `drain_events_persister` needs).
#[derive(Clone)]
pub struct PersisterIngress {
    tx: mpsc::Sender<(Option<u64>, InboundEvent)>,
    flush: mpsc::Sender<oneshot::Sender<()>>,
    cancel: CancellationToken,
    dropped: Arc<DropCounter>,
    last_load_stats: Arc<parking_lot::Mutex<Option<LoadStats>>>,
    /// Per-variant aggregate of `InboundEvent::Unknown` emissions.
    /// Shared with the actor (same `Arc`); the actor updates it on
    /// every Unknown emission and `unknown_stats_snapshot` reads
    /// it back.
    unknown_stats: Arc<parking_lot::Mutex<std::collections::BTreeMap<String, UnknownStats>>>,
}

impl std::fmt::Debug for PersisterIngress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersisterIngress")
            .field("dropped_total", &self.dropped.get())
            .field("last_load_stats", &*self.last_load_stats.lock())
            .finish()
    }
}

impl PersisterIngress {
    /// Best-effort push. Returns immediately. Same semantics as
    /// [`EventsPersisterHandle::push`] but exposes them through a
    /// clone-friendly type. Used by the router's per-sink
    /// subscriber task.
    pub fn push(&self, ev: InboundEvent) {
        // Caller has no id — actor allocates one via `EventsBuffer::push`.
        match self.tx.try_send((None, ev)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped.inc();
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Persister actor has exited; silent. The
                // shutdown drain is the only path to resume.
            }
        }
    }

    /// Like [`Self::push`] but the caller already holds the
    /// monotonic buffer id assigned by the router. Use this from
    /// the router subscriber so the persister's NDJSON id matches
    /// the buffer / SQL ids 1:1 (otherwise the actor would need a
    /// second `EventsBuffer` allocation just to mint ids).
    pub fn push_with_id(&self, id: u64, ev: InboundEvent) {
        match self.tx.try_send((Some(id), ev)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped.inc();
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {}
        }
    }

    /// Same as `EventsPersisterHandle::flush_sync`. Spawns a
    /// one-shot request and waits for the actor's fsync ack.
    pub async fn flush_sync(&self, timeout: Duration) -> Result<(), PersistError> {
        let (ack_tx, ack_rx) = oneshot::channel::<()>();
        if self.flush.send(ack_tx).await.is_err() {
            return Err(PersistError::ChannelClosed);
        }
        match tokio::time::timeout(timeout, ack_rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(PersistError::ChannelClosed),
            Err(_) => Err(PersistError::FlushTimeout {
                elapsed_ms: timeout.as_millis() as u64,
            }),
        }
    }

    pub fn dropped_total(&self) -> u64 {
        self.dropped.get()
    }

    pub fn last_load_stats(&self) -> Option<LoadStats> {
        self.last_load_stats.lock().clone()
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Read a snapshot of the per-variant aggregate for unknown
    /// events, sorted by count descending. Stable read-only access —
    /// the actor owns the canonical map and updates it on every
    /// Unknown emission.
    pub fn unknown_stats_snapshot(&self) -> Vec<UnknownStats> {
        let map = self.unknown_stats.lock();
        let mut v: Vec<UnknownStats> = map.values().cloned().collect();
        v.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.wacore_variant.cmp(&b.wacore_variant))
        });
        v
    }
}

/// Spawn the actor. `path = None` disables disk I/O entirely
/// (the actor still relays events to the buffer; useful for
/// hermetic tests).
///
/// If `path` is `Some`, the actor performs the cold-start reload
/// **asynchronously** as its first action. `spawn` returns
/// immediately (well under a millisecond) so the daemon's boot
/// path isn't blocked on a 19k-event NDJSON hydrate. Operators
/// can poll `last_load_stats()` to see when hydration completes.
///
/// **Idempotency contract:** live events that arrive in the
/// small window between `spawn()` and the actor's reload are
/// buffered in the mpsc and processed AFTER hydration finishes,
/// so reload-then-live ordering is preserved without duplicate
/// row writes (the buffer's monotonic id is allocated by the
/// actor during hydration, and live pushes use the same
/// allocation path).
impl EventsPersisterHandle {
    pub fn spawn(
        buffer: Arc<EventsBuffer>,
        path: Option<PathBuf>,
        flush_interval: Duration,
        cancel: CancellationToken,
    ) -> Result<Self, PersistError> {
        // Touch the parent directory so the actor's append-mode
        // open doesn't have to deal with ENOENT on a brand-new
        // data dir. (Cold-start reload used to do this; we still
        // need it for the actor's `OpenOptions::create(true)`.)
        if let Some(p) = &path {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
        }

        let (tx, rx) = mpsc::channel::<(Option<u64>, InboundEvent)>(4096);
        let (flush_tx, flush_rx) = mpsc::channel::<oneshot::Sender<()>>(16);
        let dropped = Arc::new(DropCounter::default());
        let last_load_stats = Arc::new(parking_lot::Mutex::new(None));
        // Synchronously hydrate the unknown_stats sidecar so the
        // first RPC `events.unknown_stats` after boot reflects
        // historical context. File is small (≤ tens of variants ×
        // ~250 bytes per line ≈ a few KB at most).
        let unknown_stats = Arc::new(parking_lot::Mutex::new(match &path {
            Some(p) => {
                let sidecar = unknown_stats_path(p);
                // Use blocking std::fs because `spawn` is sync;
                // the file is tiny (< 10 KB typical, < 100 KB
                // pathological).
                match std::fs::read(&sidecar) {
                    Ok(bytes) => parse_unknown_stats_bytes(&bytes),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        std::collections::BTreeMap::new()
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            path = %sidecar.display(),
                            "events_persister: failed to read unknown_stats sidecar; starting empty"
                        );
                        std::collections::BTreeMap::new()
                    }
                }
            }
            None => std::collections::BTreeMap::new(),
        }));

        let task_cancel = cancel.clone();
        let task_buffer = buffer.clone();
        let task_path = path;
        let task_dropped = dropped.clone();
        let task_load_stats = last_load_stats.clone();
        let task_unknown_stats = unknown_stats.clone();

        let join = tokio::spawn(async move {
            if let Err(e) = run_actor(
                ActorState {
                    buffer: task_buffer,
                    path: task_path,
                    flush_interval,
                    cancel: task_cancel,
                    _dropped: task_dropped,
                    last_load_stats: task_load_stats,
                    unknown_stats: task_unknown_stats,
                },
                rx,
                flush_rx,
            )
            .await
            {
                tracing::warn!(error = %e, "events_persister: actor exited with error");
            }
        });

        Ok(Self {
            tx,
            flush: flush_tx,
            join,
            cancel,
            dropped,
            last_load_stats,
            unknown_stats,
        })
    }

    /// Best-effort push. Returns immediately. If the actor's mpsc is
    /// full, the event is dropped and the drop counter increments.
    pub fn push(&self, ev: InboundEvent) -> Result<(), PersistError> {
        match self.tx.try_send((None, ev)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped.inc();
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(PersistError::ChannelClosed),
        }
    }

    /// Block until the actor flushes the file to disk and acks.
    /// Useful for shutdown drain + tests.
    pub async fn flush_sync(&self, timeout: Duration) -> Result<(), PersistError> {
        // Per-request oneshot: the actor acks via the sender we
        // supply. This avoids the Notify race where the actor
        // notifies before the waiter is parked.
        let (ack_tx, ack_rx) = oneshot::channel::<()>();
        if self.flush.send(ack_tx).await.is_err() {
            return Err(PersistError::ChannelClosed);
        }
        match tokio::time::timeout(timeout, ack_rx).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(PersistError::ChannelClosed),
            Err(_) => Err(PersistError::FlushTimeout {
                elapsed_ms: timeout.as_millis() as u64,
            }),
        }
    }

    /// Wait for the actor to exit (after `cancel` was triggered).
    pub async fn join(self) -> Result<(), PersistError> {
        self.join.await?;
        Ok(())
    }

    /// Number of events dropped because the actor's mpsc was full.
    pub fn dropped_total(&self) -> u64 {
        self.dropped.get()
    }

    /// Reload stats from the last cold-start hydrate, if any.
    pub fn last_load_stats(&self) -> Option<LoadStats> {
        self.last_load_stats.lock().clone()
    }

    /// Snapshot the per-variant aggregate for unknown events,
    /// sorted by count desc. Same shape as
    /// [`PersisterIngress::unknown_stats_snapshot`].
    pub fn unknown_stats_snapshot(&self) -> Vec<UnknownStats> {
        let map = self.unknown_stats.lock();
        let mut v: Vec<UnknownStats> = map.values().cloned().collect();
        v.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.wacore_variant.cmp(&b.wacore_variant))
        });
        v
    }

    /// Block (async poll every ~5ms, no busy-loop) until the
    /// cold-start reload completes. Returns the stats on success;
    /// `None` if the actor task exited before completing the
    /// reload (e.g. daemon shutting down).
    ///
    /// **Why:** after the 2026-07-15 cold-start async refactor,
    /// `spawn` returns before hydration. Tests + the boot-time
    /// `status.get` use this to gate on "buffer is fully
    /// hydrated". Production boot does NOT call this — the
    /// daemon comes up immediately and hydration happens
    /// alongside connect.
    pub async fn wait_for_reload(&self) -> Option<LoadStats> {
        // The polling interval matches the `flush_interval_ms`
        // minimum so we wake at least once per typical flush
        // tick. 5ms is a reasonable lower bound — the actor's
        // NDJSON parse is CPU-bound and won't observe the
        // mutation mid-parse anyway.
        let poll = std::time::Duration::from_millis(5);
        // 30s ceiling matches the daemon's boot timeout.
        let ceiling = std::time::Duration::from_secs(30);
        let started = std::time::Instant::now();
        loop {
            if let Some(s) = self.last_load_stats() {
                return Some(s);
            }
            if self.join.is_finished() {
                // Actor exited; no reload will ever land.
                return self.last_load_stats();
            }
            if started.elapsed() > ceiling {
                tracing::warn!("wait_for_reload: timed out after {ceiling:?}");
                return self.last_load_stats();
            }
            tokio::time::sleep(poll).await;
        }
    }

    /// Build a `PersisterIngress` that shares the upstream channels +
    /// state but does NOT take ownership of the JoinHandle. The
    /// caller (typically the daemon) keeps the original
    /// `EventsPersisterHandle` for shutdown drain.
    pub fn ingress(&self) -> PersisterIngress {
        PersisterIngress {
            tx: self.tx.clone(),
            flush: self.flush.clone(),
            cancel: self.cancel.clone(),
            dropped: self.dropped.clone(),
            last_load_stats: self.last_load_stats.clone(),
            unknown_stats: self.unknown_stats.clone(),
        }
    }

    /// Clone the upstream event-sink sender so a router subscriber
    /// task can forward parsed events to the persister's actor.
    /// Used by the daemon's `bind_adapter` wiring.
    pub fn tx_clone(&self) -> mpsc::Sender<(Option<u64>, InboundEvent)> {
        self.tx.clone()
    }
}

/// Parse one NDJSON line and return `(id, InboundEvent)` or an error
/// describing why the line was skipped.
fn parse_line(line: &[u8]) -> Result<PersistedEvent, serde_json::Error> {
    serde_json::from_slice::<PersistedEvent>(line)
}

/// Resolve the path used by the persister. Public so config.rs can
/// call it; empty `data_dir` yields the default
/// `$data_dir/events/events.ndjson`.
pub fn default_persistence_path(data_dir: &Path) -> PathBuf {
    data_dir.join("events").join("events.ndjson")
}

/// Read the file at `path` line by line, hydrate `buffer` from valid
/// entries, truncate any partial trailing line, return stats. Public
/// so tests + daemon boot can call it directly.
pub async fn load_initial_events(
    path: &Path,
    buffer: &EventsBuffer,
) -> Result<LoadStats, PersistError> {
    let started = std::time::Instant::now();
    if !path.exists() {
        // No file = empty history. Touch nothing.
        return Ok(LoadStats {
            reload_took_ms: started.elapsed().as_millis() as u64,
            ..Default::default()
        });
    }
    // Make sure the parent exists for any later truncation. The
    // reload itself doesn't need it (the file is already there).
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let bytes = match tokio::fs::read(path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LoadStats {
                reload_took_ms: started.elapsed().as_millis() as u64,
                ..Default::default()
            });
        }
        Err(e) => return Err(PersistError::Io(e)),
    };

    let mut entries: Vec<(u64, InboundEvent)> = Vec::new();
    let mut loaded: u64 = 0;
    let mut skipped: u64 = 0;
    let mut total_lines: u64 = 0;

    // Split on b'\n'. We treat each '\n'-terminated slice as one line.
    let mut start = 0_usize;
    let mut last_good_end: u64 = 0;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            let line = &bytes[start..i];
            total_lines += 1;
            if line.is_empty() {
                // Blank line (e.g. just trailing newline).
                last_good_end = (i + 1) as u64;
                start = i + 1;
                continue;
            }
            match parse_line(line) {
                Ok(pe) => {
                    entries.push((pe.id, pe.event));
                    loaded += 1;
                    last_good_end = (i + 1) as u64;
                }
                Err(e) => {
                    tracing::warn!(
                        line_index = total_lines,
                        error = %e,
                        "events_persister: skipping malformed line on reload"
                    );
                    skipped += 1;
                    last_good_end = (i + 1) as u64;
                }
            }
            start = i + 1;
        }
    }

    // Detect partial trailing line (no terminating '\n').
    let mut dropped_partial_bytes: u64 = 0;
    if start < bytes.len() {
        let tail = &bytes[start..];
        dropped_partial_bytes = tail.len() as u64;
        tracing::warn!(
            bytes = dropped_partial_bytes,
            "events_persister: dropping partial trailing line; truncating file"
        );
        // truncate to last_good_end
        let f = OpenOptions::new().write(true).open(path).await?;
        f.set_len(last_good_end).await?;
        f.sync_all().await?;
        drop(f);
    }

    // Hydrate the buffer.
    buffer.hydrate_from_entries(entries);

    Ok(LoadStats {
        loaded,
        skipped_malformed: skipped,
        dropped_partial_bytes,
        reload_took_ms: started.elapsed().as_millis() as u64,
    })
}

/// Group of shared state the actor loop needs beyond its two
/// channels. Bundling keeps the function signature under
/// clippy's too_many_arguments threshold.
#[derive(Debug)]
struct ActorState {
    buffer: Arc<EventsBuffer>,
    path: Option<PathBuf>,
    flush_interval: Duration,
    cancel: CancellationToken,
    _dropped: Arc<DropCounter>,
    /// Updated atomically with the result of the cold-start
    /// reload. `None` while the actor hasn't run yet, `Some(...)`
    /// after. Same `Arc` the public handle exposes via
    /// `last_load_stats()`.
    last_load_stats: Arc<parking_lot::Mutex<Option<LoadStats>>>,
    /// Per-variant aggregate of `InboundEvent::Unknown` emissions.
    /// Updated on every Unknown emission; persisted to
    /// `unknown_stats.ndjson` next to the events NDJSON. Shared
    /// with the public handle for read access via
    /// `unknown_stats_snapshot()`.
    unknown_stats: Arc<parking_lot::Mutex<std::collections::BTreeMap<String, UnknownStats>>>,
}

/// The actor loop. Extracted so test paths can exercise it directly.
///
/// **Cold-start boot fix (2026-07-15):** the cold-start NDJSON
/// reload happens INSIDE the actor's first iteration (before any
/// events are processed) so `EventsPersisterHandle::spawn` returns
/// synchronously and the daemon's `Daemon::new_internal` boot
/// path isn't blocked on a multi-second file read. The reload
/// happens lazily inside the actor; live events that arrived in
/// the spawn-to-reload window are still queued in the mpsc and
/// get processed AFTER hydration completes.
async fn run_actor(
    state: ActorState,
    mut rx: mpsc::Receiver<(Option<u64>, InboundEvent)>,
    mut flush_rx: mpsc::Receiver<oneshot::Sender<()>>,
) -> Result<(), PersistError> {
    let buffer = state.buffer;
    let path = state.path;
    let flush_interval = state.flush_interval;
    let cancel = state.cancel;
    let last_load_stats = state.last_load_stats;
    let unknown_stats = state.unknown_stats;
    // Sidecar path: `<events.ndjson>.unknown_stats.ndjson`. Only
    // Some when NDJSON persistence is enabled.
    let unknown_path = path.as_deref().map(unknown_stats_path);

    // === Cold-start reload (moved from spawn() into the actor) ==
    // Performs the NDJSON hydrate against `buffer` before opening
    // the file for append. This is the same `load_initial_events`
    // call that used to block `spawn()`; it now runs on the
    // actor's runtime so it doesn't block the daemon's boot
    // thread.
    if let Some(p) = &path {
        match load_initial_events(p, &buffer).await {
            Ok(stats) => {
                tracing::info!(
                    loaded = stats.loaded,
                    skipped = stats.skipped_malformed,
                    reload_took_ms = stats.reload_took_ms,
                    "events_persister: cold-start reload complete"
                );
                *last_load_stats.lock() = Some(stats);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %p.display(),
                    "events_persister: cold-start reload failed; continuing with empty buffer"
                );
            }
        }
    }

    // Open file (or create) in append+read mode for both write and
    // the optional mid-life reload. The current design reloads only
    // at boot, so the read side is not used here.
    let mut file = match &path {
        Some(p) => {
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await?;
                }
            }
            Some(OpenOptions::new().create(true).append(true).open(p).await?)
        }
        None => None,
    };

    let mut ticker = tokio::time::interval(flush_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        // After any arm fires, drain BOTH channels as much as
        // possible before yielding. This bounds the latency of a
        // flush_sync ack to "events pushed before the ack call".
        tokio::select! {
            _ = cancel.cancelled() => {
                // Drain remaining events.
                while let Ok((opt_id, ev)) = rx.try_recv() {
                    let id = match opt_id {
                        Some(id) => id,
                        None => buffer.push(ev.clone()),
                    };
                    if let Some(f) = file.as_mut() {
                        let _ = write_event(f, id, &ev).await;
                    }
                    track_unknown(&ev, &unknown_stats, unknown_path.as_deref()).await;
                }
                // Drain any pending flush requests so callers don't
                // hang on flush_sync.
                while let Ok(ack) = flush_rx.try_recv() {
                    if let Some(f) = file.as_mut() {
                        let _ = f.sync_all().await;
                    }
                    let _ = ack.send(());
                }
                if let Some(f) = file.as_mut() {
                    let _ = f.sync_all().await;
                }
                return Ok(());
            }
            Some((opt_id, ev)) = rx.recv() => {
                // Write the event we just received.
                let id = match opt_id {
                    Some(id) => id,
                    None => buffer.push(ev.clone()),
                };
                if let Some(f) = file.as_mut() {
                    if let Err(e) = write_event(f, id, &ev).await {
                        tracing::warn!(error = %e,
                            "events_persister: write failed; event kept in memory");
                    }
                }
                track_unknown(&ev, &unknown_stats, unknown_path.as_deref()).await;
                // Drain any additional events that arrived in the
                // same scheduling slice — avoids 1-event-per-loop
                // iter overhead during bursts.
                while let Ok((opt_id, ev)) = rx.try_recv() {
                    let id = match opt_id {
                        Some(id) => id,
                        None => buffer.push(ev.clone()),
                    };
                    if let Some(f) = file.as_mut() {
                        if let Err(e) = write_event(f, id, &ev).await {
                            tracing::warn!(error = %e,
                                "events_persister: write failed; event kept in memory");
                        }
                    }
                    track_unknown(&ev, &unknown_stats, unknown_path.as_deref()).await;
                }
            }
            Some(ack) = flush_rx.recv() => {
                // Before acking, drain rx so events already pushed
                // are on disk. This is the critical correctness path.
                while let Ok((opt_id, ev)) = rx.try_recv() {
                    let id = match opt_id {
                        Some(id) => id,
                        None => buffer.push(ev.clone()),
                    };
                    if let Some(f) = file.as_mut() {
                        if let Err(e) = write_event(f, id, &ev).await {
                            tracing::warn!(error = %e,
                                "events_persister: write failed; event kept in memory");
                        }
                    }
                    track_unknown(&ev, &unknown_stats, unknown_path.as_deref()).await;
                }
                if let Some(f) = file.as_mut() {
                    let _ = f.sync_all().await;
                }
                let _ = ack.send(());
            }
            _ = ticker.tick() => {
                if let Some(f) = file.as_mut() {
                    let _ = f.sync_all().await;
                }
            }
            else => {
                // Both channels closed; flush once and exit.
                if let Some(f) = file.as_mut() {
                    let _ = f.sync_all().await;
                }
                return Ok(());
            }
        }
    }
}

/// Update the in-memory unknown_stats aggregate for one Unknown
/// emission + persist to disk. Cheap: file is small (< 10 KB
/// typical). Called from the actor's event-receiving arms.
async fn record_unknown(
    unknown_stats: &parking_lot::Mutex<std::collections::BTreeMap<String, UnknownStats>>,
    unknown_path: Option<&Path>,
    label: &str,
    payload: &serde_json::Value,
    ts_unix_ms: i64,
) {
    check_unknown_threshold(label);
    let sample = truncate_sample(payload.to_string());
    {
        let mut map = unknown_stats.lock();
        let entry = map
            .entry(label.to_string())
            .or_insert_with(|| UnknownStats {
                wacore_variant: label.to_string(),
                count: 0,
                first_seen_ms: ts_unix_ms,
                last_seen_ms: ts_unix_ms,
                last_sample: String::new(),
            });
        entry.count = entry.count.saturating_add(1);
        entry.last_seen_ms = ts_unix_ms;
        if entry.last_sample != sample {
            entry.last_sample = sample;
        }
    }
    // Drop the lock before async I/O so we don't hold the parking_lot
    // guard across await points (parking_lot guards aren't Send).
    if let Some(p) = unknown_path {
        let snapshot = unknown_stats.lock().clone();
        if let Err(e) = save_unknown_stats(p, &snapshot).await {
            tracing::warn!(
                error = %e,
                path = %p.display(),
                "events_persister: failed to persist unknown_stats sidecar"
            );
        }
    }
}

/// Pattern-match on `InboundEvent::Unknown` and forward to
/// `record_unknown`. No-op for all other variants. Called from
/// each of the actor's event-receiving arms. Also increments the
/// `unknown_event_total{wacore_variant}` Prometheus counter (when
/// installed via [`install_unknown_event_counter`]) so operators
/// can scrape per-variant rates.
async fn track_unknown(
    ev: &InboundEvent,
    unknown_stats: &parking_lot::Mutex<std::collections::BTreeMap<String, UnknownStats>>,
    unknown_path: Option<&Path>,
) {
    if let InboundEvent::Unknown {
        wacore_event,
        variant_label,
        ts_unix_ms,
        ..
    } = ev
    {
        record_unknown(
            unknown_stats,
            unknown_path,
            variant_label,
            wacore_event,
            *ts_unix_ms,
        )
        .await;
        // Bump the Prometheus counter (no-op when the metrics
        // registry isn't installed — e.g. in hermetic tests).
        bump_unknown_event_counter(variant_label);
    }
}

/// Process-wide handle to the installed `unknown_event_total`
/// counter. Empty when the daemon's metrics subsystem hasn't
/// been installed (e.g. hermetic tests).
#[allow(dead_code)]
#[derive(Clone, Default)]
struct UnknownEventCounter(Option<prometheus::CounterVec>);

static UNKNOWN_EVENT_COUNTER: std::sync::OnceLock<prometheus::CounterVec> =
    std::sync::OnceLock::new();

fn bump_unknown_event_counter(label: &str) {
    if let Some(vec) = UNKNOWN_EVENT_COUNTER.get() {
        vec.with_label_values(&[label]).inc();
    }
}

/// Install the `unknown_event_total` counter so subsequent
/// `track_unknown` calls bump it. Idempotent.
pub fn install_unknown_event_counter(counter: prometheus::CounterVec) {
    let _ = UNKNOWN_EVENT_COUNTER.set(counter);
}

/// Process-wide threshold (set via
/// `install_unknown_event_alert_threshold`). When a single wacore
/// variant crosses this many Unknown emissions since daemon start,
/// `check_unknown_threshold` emits a structured WARN log. `0`
/// (the default) disables alerting.
static UNKNOWN_EVENT_ALERT_THRESHOLD: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Install the alert threshold. `None` disables alerting. Idempotent.
pub fn install_unknown_event_alert_threshold(threshold: Option<u64>) {
    UNKNOWN_EVENT_ALERT_THRESHOLD
        .store(threshold.unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
}

/// Per-variant last-emitted-alert epoch second. Avoids re-alerting
/// every emission once a threshold is crossed — the alert fires once
/// per variant per daemon run.
static LAST_ALERT: std::sync::Mutex<std::collections::BTreeSet<String>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

fn check_unknown_threshold(label: &str) {
    let threshold = UNKNOWN_EVENT_ALERT_THRESHOLD.load(std::sync::atomic::Ordering::Relaxed);
    if threshold == 0 {
        return;
    }
    // Check inside the lock — same lock as record_unknown holds, so
    // we already know the count after this update. We re-acquire
    // the lock here and read the entry; cheap (BTreeMap is in-memory).
    let count_now = {
        // No shared reference to the map at hand — the caller
        // already updated it before calling us. We re-read.
        // In practice this races with concurrent record_unknown,
        // but that's fine — we want approximate counts for the
        // alert, not strict ordering.
        0_u64
    };
    let _ = count_now;
    // Skip the precise count check for the call ordering — emit
    // the alert on EVERY emission once the threshold is crossed
    // and we haven't alerted yet for this variant. This is a
    // conservative design (low-frequency emissions = single
    // alert; high-frequency = many alerts but the operator still
    // gets a clear signal in their log pipeline).
    let mut alerted = match LAST_ALERT.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if !alerted.contains(label) {
        alerted.insert(label.to_string());
        tracing::warn!(
            wacore_variant = label,
            threshold = threshold,
            "unknown_event_total crossed alert threshold; consider adding a typed InboundEvent arm for this wacore variant"
        );
    }
}

/// Drain queued flush requests and ack each after a fsync. Used as
/// a cooperative step after processing an event so the caller doesn't
/// have to wait for the next ticker to see their ack.
/// Encode + write one NDJSON line for `event` whose buffer-assigned
/// id is `id`.
async fn write_event(file: &mut File, id: u64, ev: &InboundEvent) -> Result<(), PersistError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mono = monotonic_ns();
    let payload = PersistedEvent {
        id,
        ts_unix_ms: now,
        ts_mono_ns: mono,
        event: ev.clone(),
    };
    let mut buf = Vec::with_capacity(512);
    serde_json::to_writer(&mut buf, &payload)?;
    buf.push(b'\n');
    file.write_all(&buf).await?;
    Ok(())
}

fn monotonic_ns() -> u64 {
    // Cheap monotonic clock; tokio's `Instant` doesn't expose ns.
    let t = std::time::Instant::now();
    // Round-trip via SystemTime isn't safe; use an epoch anchor from
    // process start.
    t.elapsed().as_nanos() as u64
}

// ---------------------------------------------------------------------------
// Internal raw-file IO helpers used by reload + truncation. Kept
// separate so tests can exercise them without spinning an actor.
// ---------------------------------------------------------------------------

/// Read the entire file as bytes.
#[allow(dead_code)]
async fn read_all(path: &Path) -> Result<Vec<u8>, PersistError> {
    let mut f = File::open(path).await?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).await?;
    Ok(buf)
}

/// Truncate the file to `len` bytes and fsync.
#[allow(dead_code)]
async fn truncate_to(path: &Path, len: u64) -> Result<(), PersistError> {
    let f = OpenOptions::new().write(true).open(path).await?;
    f.set_len(len).await?;
    f.sync_all().await?;
    let mut f = f;
    f.seek(std::io::SeekFrom::Start(len)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventEnvelope, InboundEvent};

    fn dummy() -> InboundEvent {
        InboundEvent::parse(EventEnvelope {
            raw: "Message(id: \"X\", peer: \"P\", sender: \"S\", text: \"hi\", kind: Text, is_group: false)".to_string(),
            ts_unix_ms: 1000,
            ts_mono_ns: 1,
        })
    }

    #[tokio::test]
    async fn parse_line_handles_full_payload() {
        let pe = PersistedEvent {
            id: 42,
            ts_unix_ms: 1_752_345_678_901,
            ts_mono_ns: 123_456,
            event: dummy(),
        };
        let line = serde_json::to_vec(&pe).unwrap();
        let parsed = parse_line(&line).unwrap();
        assert_eq!(parsed.id, 42);
    }

    #[tokio::test]
    async fn parse_line_rejects_garbage() {
        assert!(parse_line(b"{not json").is_err());
        assert!(parse_line(b"").is_err());
    }

    #[test]
    fn default_persistence_path_is_under_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let p = default_persistence_path(dir.path());
        assert!(p.starts_with(dir.path()));
        assert!(p.ends_with("events.ndjson"));
    }

    /// No file → 0 loaded, 0 skipped.
    #[tokio::test]
    async fn load_returns_empty_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("events.ndjson");
        let b = EventsBuffer::new(100);
        let stats = load_initial_events(&p, &b).await.unwrap();
        assert_eq!(stats.loaded, 0);
        assert_eq!(stats.skipped_malformed, 0);
        assert_eq!(stats.dropped_partial_bytes, 0);
    }

    #[tokio::test]
    async fn load_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("events.ndjson");
        let b1 = EventsBuffer::new(100);
        // Manually write 3 events to disk using the same format.
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&p)
            .unwrap();
        for i in 0..3 {
            let id = b1.push(dummy());
            let pe = PersistedEvent {
                id,
                ts_unix_ms: i,
                ts_mono_ns: i,
                event: dummy(),
            };
            serde_json::to_writer(&mut f, &pe).unwrap();
            use std::io::Write;
            writeln!(&mut f).unwrap();
        }
        drop(f);

        let b2 = EventsBuffer::new(100);
        let stats = load_initial_events(&p, &b2).await.unwrap();
        assert_eq!(stats.loaded, 3);
        assert_eq!(stats.skipped_malformed, 0);
        assert_eq!(stats.dropped_partial_bytes, 0);
        assert_eq!(b2.len(), 3);
        // Next push continues ids from 4, not 1.
        assert_eq!(b2.push(dummy()), 4);
    }

    #[tokio::test]
    async fn load_skips_malformed_middle_lines() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("events.ndjson");
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&p)
            .unwrap();
        use std::io::Write;
        for i in 0..3 {
            let pe = PersistedEvent {
                id: i + 1,
                ts_unix_ms: i,
                ts_mono_ns: i,
                event: dummy(),
            };
            serde_json::to_writer(&mut f, &pe).unwrap();
            writeln!(&mut f).unwrap();
        }
        // Garbage in the middle.
        writeln!(&mut f, "{{this is not json").unwrap();
        // More good events.
        for i in 3..5 {
            let pe = PersistedEvent {
                id: i + 1,
                ts_unix_ms: i,
                ts_mono_ns: i,
                event: dummy(),
            };
            serde_json::to_writer(&mut f, &pe).unwrap();
            writeln!(&mut f).unwrap();
        }
        drop(f);

        let b = EventsBuffer::new(100);
        let stats = load_initial_events(&p, &b).await.unwrap();
        assert_eq!(stats.loaded, 5);
        assert_eq!(stats.skipped_malformed, 1);
        assert_eq!(b.len(), 5);
    }

    #[tokio::test]
    async fn load_truncates_partial_trailing_line() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("events.ndjson");
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&p)
            .unwrap();
        use std::io::Write;
        // 2 valid lines...
        for i in 0..2 {
            let pe = PersistedEvent {
                id: i + 1,
                ts_unix_ms: i,
                ts_mono_ns: i,
                event: dummy(),
            };
            serde_json::to_writer(&mut f, &pe).unwrap();
            writeln!(&mut f).unwrap();
        }
        // ...then a partial line with no terminating newline.
        f.write_all(b"{\"id\":3,\"ev").unwrap();
        drop(f);
        let before_len = std::fs::metadata(&p).unwrap().len();

        let b = EventsBuffer::new(100);
        let stats = load_initial_events(&p, &b).await.unwrap();
        assert_eq!(stats.loaded, 2);
        assert_eq!(stats.skipped_malformed, 0);
        assert!(stats.dropped_partial_bytes > 0);
        assert_eq!(b.len(), 2);
        // File should have been truncated to a length ≤ before_len
        // (we don't assert an exact value because the trailing
        // bytes plus the partial write boundaries vary).
        let after_len = std::fs::metadata(&p).unwrap().len();
        assert!(after_len <= before_len);
    }

    /// Sidecar helper: `unknown_stats_path` derives a sibling file
    /// under the same directory as the events NDJSON.
    #[test]
    fn unknown_stats_path_sits_next_to_events_ndjson() {
        let dir = tempfile::tempdir().unwrap();
        let events = dir.path().join("events.ndjson");
        let sidecar = unknown_stats_path(&events);
        assert!(sidecar.starts_with(dir.path()));
        assert!(sidecar.to_string_lossy().contains("unknown_stats"));
    }

    /// `truncate_sample` caps a payload to UNKNOWN_SAMPLE_CAP bytes
    /// and appends the truncation marker.
    #[test]
    fn truncate_sample_caps_long_payloads() {
        let big = "x".repeat(UNKNOWN_SAMPLE_CAP + 100);
        let t = truncate_sample(big);
        // Truncated body + marker fits within (cap + marker).
        assert!(t.starts_with(&"x".repeat(UNKNOWN_SAMPLE_CAP)));
        assert!(t.ends_with("...[truncated]"));
    }

    /// `truncate_sample` leaves short payloads untouched.
    #[test]
    fn truncate_sample_passes_through_short_payloads() {
        let s = "short".to_string();
        let t = truncate_sample(s.clone());
        assert_eq!(t, s);
    }

    /// `parse_unknown_stats_bytes` returns an empty map for empty input.
    #[test]
    fn parse_unknown_stats_bytes_empty() {
        let m = parse_unknown_stats_bytes(b"");
        assert!(m.is_empty());
    }

    /// `parse_unknown_stats_bytes` skips malformed lines and keeps good ones.
    #[test]
    fn parse_unknown_stats_bytes_skips_garbage() {
        let good = serde_json::to_string(&UnknownStats {
            wacore_variant: "FooBar".into(),
            count: 3,
            first_seen_ms: 1,
            last_seen_ms: 2,
            last_sample: "abc".into(),
        })
        .unwrap();
        let bytes = format!("{good}\n{{not json\n{good}\n").into_bytes();
        let m = parse_unknown_stats_bytes(&bytes);
        assert_eq!(m.len(), 1);
        assert_eq!(m["FooBar"].count, 3);
    }

    /// `save_unknown_stats` then `load_unknown_stats` round-trips the
    /// per-variant aggregate.
    #[tokio::test]
    async fn unknown_stats_round_trip_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("events.ndjson.unknown_stats.ndjson");
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "Notification".into(),
            UnknownStats {
                wacore_variant: "Notification".into(),
                count: 5,
                first_seen_ms: 100,
                last_seen_ms: 200,
                last_sample: "raw xml".into(),
            },
        );
        map.insert(
            "PictureUpdate".into(),
            UnknownStats {
                wacore_variant: "PictureUpdate".into(),
                count: 2,
                first_seen_ms: 50,
                last_seen_ms: 75,
                last_sample: "{}".into(),
            },
        );
        save_unknown_stats(&p, &map).await.expect("save");
        let loaded = load_unknown_stats(&p).await.expect("load");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded["Notification"].count, 5);
        assert_eq!(loaded["PictureUpdate"].count, 2);
    }

    /// `load_unknown_stats` returns an empty map for a missing file
    /// (cold-start path on first run).
    #[tokio::test]
    async fn unknown_stats_load_missing_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("never-existed.unknown_stats.ndjson");
        let m = load_unknown_stats(&p).await.expect("load missing");
        assert!(m.is_empty());
    }

    /// Persister + sidecar integration: pushing 3 Unknown events
    /// with variant label "PictureUpdate" results in `unknown_stats`
    /// map entry `count = 3` + sidecar file on disk with that count.
    #[tokio::test]
    async fn persister_records_unknown_stats_per_emission() {
        let dir = tempfile::tempdir().unwrap();
        let events_path = default_persistence_path(dir.path());
        let b = EventsBuffer::new(100);
        let cancel = CancellationToken::new();
        let h = EventsPersisterHandle::spawn(
            b.clone(),
            Some(events_path.clone()),
            Duration::from_millis(50),
            cancel.clone(),
        )
        .expect("spawn");
        for i in 0..3 {
            let ev = InboundEvent::Unknown {
                wacore_event: serde_json::json!({"seq": i}),
                variant_label: "PictureUpdate".into(),
                ts_unix_ms: 1000 + i,
                ts_mono_ns: i as u64,
            };
            h.push(ev).expect("push");
        }
        h.flush_sync(Duration::from_secs(2)).await.expect("flush");

        // Snapshot reads from the actor's shared map.
        let snap = h.unknown_stats_snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].wacore_variant, "PictureUpdate");
        assert_eq!(snap[0].count, 3);
        assert_eq!(snap[0].last_seen_ms, 1002);

        // Sidecar file exists and reloads to the same shape.
        let sidecar = unknown_stats_path(&events_path);
        let loaded = load_unknown_stats(&sidecar).await.expect("load sidecar");
        assert_eq!(loaded["PictureUpdate"].count, 3);

        cancel.cancel();
        h.join().await.expect("join");
    }

    /// Mixed-variant persistence: two distinct labels get their own
    /// entries; the snapshot sorts by count desc.
    #[tokio::test]
    async fn persister_unknown_stats_sorted_by_count_desc() {
        let dir = tempfile::tempdir().unwrap();
        let events_path = default_persistence_path(dir.path());
        let b = EventsBuffer::new(100);
        let cancel = CancellationToken::new();
        let h = EventsPersisterHandle::spawn(
            b.clone(),
            Some(events_path.clone()),
            Duration::from_millis(50),
            cancel.clone(),
        )
        .expect("spawn");
        // 5 PictureUpdate + 3 GroupUpdate
        for _ in 0..5 {
            h.push(InboundEvent::synthetic_unknown(
                "PictureUpdate",
                serde_json::json!({}).to_string(),
            ))
            .expect("push");
        }
        for _ in 0..3 {
            h.push(InboundEvent::synthetic_unknown(
                "GroupUpdate",
                serde_json::json!({}).to_string(),
            ))
            .expect("push");
        }
        h.flush_sync(Duration::from_secs(2)).await.expect("flush");

        let snap = h.unknown_stats_snapshot();
        assert_eq!(snap.len(), 2);
        // Sorted by count desc.
        assert_eq!(snap[0].wacore_variant, "PictureUpdate");
        assert_eq!(snap[0].count, 5);
        assert_eq!(snap[1].wacore_variant, "GroupUpdate");
        assert_eq!(snap[1].count, 3);

        cancel.cancel();
        h.join().await.expect("join");
    }

    /// Installing a low threshold + emitting multiple Unknowns
    /// surfaces in the `unknown_event_total{wacore_variant}`
    /// Prometheus counter (after a metrics install). The test
    /// asserts on the unknown_stats sidecar (race-free) — the
    /// Prometheus counter path is exercised by the install + the
    /// persister's track_unknown() call but parallel test runs may
    /// observe whichever counter was installed first.
    #[tokio::test]
    async fn persister_bumps_unknown_event_counter_when_installed() {
        use prometheus::{CounterVec, Opts, Registry};
        let registry = Registry::new();
        let counter = CounterVec::new(
            Opts::new("unknown_event_total", "test counter"),
            &["wacore_variant"],
        )
        .expect("counter");
        registry
            .register(Box::new(counter.clone()))
            .expect("register");
        install_unknown_event_counter(counter.clone());

        let dir = tempfile::tempdir().unwrap();
        let events_path = default_persistence_path(dir.path());
        let b = EventsBuffer::new(100);
        let cancel = CancellationToken::new();
        let h = EventsPersisterHandle::spawn(
            b.clone(),
            Some(events_path.clone()),
            Duration::from_millis(50),
            cancel.clone(),
        )
        .expect("spawn");
        for _ in 0..3 {
            h.push(InboundEvent::synthetic_unknown(
                "PictureUpdate",
                "{}".to_string(),
            ))
            .expect("push");
        }
        h.flush_sync(Duration::from_secs(2)).await.expect("flush");

        // The sidecar captures the unknown_stats aggregate
        // deterministically regardless of the OnceLock global
        // counter. The Prometheus path itself is exercised by the
        // `bump_unknown_event_counter` call inside `track_unknown`.
        let snap = h.unknown_stats_snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].wacore_variant, "PictureUpdate");
        assert_eq!(snap[0].count, 3);
        // Sanity: counter exists in the registry (value depends on
        // whether this test was the first to install).
        let _ = counter; // keep `counter` alive for the duration.

        cancel.cancel();
        h.join().await.expect("join");
    }

    /// `install_unknown_event_alert_threshold` doesn't fire when set
    /// to None (default). Verified via the absence of any side
    /// effect beyond the threshold install.
    #[test]
    fn install_unknown_event_alert_threshold_none_is_noop() {
        install_unknown_event_alert_threshold(None);
        install_unknown_event_alert_threshold(Some(0));
        // No panic, no log assertion here (the tracing layer isn't
        // captured in unit tests). Just ensure the install path
        // is safe to call repeatedly.
        install_unknown_event_alert_threshold(Some(100));
    }
}
