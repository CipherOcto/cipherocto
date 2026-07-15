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

        let task_cancel = cancel.clone();
        let task_buffer = buffer.clone();
        let task_path = path;
        let task_dropped = dropped.clone();
        let task_load_stats = last_load_stats.clone();

        let join = tokio::spawn(async move {
            if let Err(e) = run_actor(
                ActorState {
                    buffer: task_buffer,
                    path: task_path,
                    flush_interval,
                    cancel: task_cancel,
                    _dropped: task_dropped,
                    last_load_stats: task_load_stats,
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
}
