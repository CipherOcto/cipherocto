//! Central event router. Phase 3 Part C.
//!
//! Subscribes to the adapter's `raw_event_tx` broadcast, parses each
//! raw event to a typed `InboundEvent`, persists it to the
//! `EventsBuffer` (single-writer `db_writer` task), and fans out to
//! per-sink bounded mpsc channels. Each sink tracks its own Lagged
//! counter so a slow consumer never blocks the others.
//!
//! Design references:
//! - §Event Stream: 8-variant typed InboundEvent (parser in events.rs).
//! - §Fan-out: per-sink mpsc; on TrySendError::Full the event is
//!   dropped and the sink's lagged counter increments (no backpressure
//!   on the parser hot path).
//! - §Loss recovery: subscribers use `events.list --since-id` to
//!   backfill after a Lagged event.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::events::{EventEnvelope, InboundEvent};
use crate::events_buffer::EventsBuffer;
use crate::observability::metrics::Metrics;

/// Per-sink mpsc channel + Lagged counter. `EventsSink` is the
/// producer-side handle; `EventsSubscriber` is the consumer-side
/// handle (the `mpsc::Receiver` paired with a `LaggedProbe`).
pub struct EventsSink {
    tx: mpsc::Sender<InboundEvent>,
    lagged: Arc<AtomicU64>,
    /// Static name used in `status.sink_lagged_total` and `warn!`
    /// logs so operators can tell which consumer fell behind. The
    /// default `subscribe()` assigns a numeric fallback; named
    /// subscribers should use `subscribe_named()`.
    name: &'static str,
}

impl std::fmt::Debug for EventsSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventsSink")
            .field("name", &self.name)
            .field("capacity", &self.tx.capacity())
            .field("lagged", &self.lagged())
            .finish()
    }
}

impl EventsSink {
    pub fn lagged(&self) -> u64 {
        self.lagged.load(Ordering::Relaxed)
    }
}

/// Consumer-side handle. Pairs a bounded `mpsc::Receiver` with a
/// `LaggedProbe` that lets the consumer observe its own Lagged
/// counter without holding a reference to the sink.
pub struct EventsSubscriber {
    rx: mpsc::Receiver<InboundEvent>,
    lagged: Arc<AtomicU64>,
}

impl std::fmt::Debug for EventsSubscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventsSubscriber")
            .field("lagged", &self.lagged())
            .finish()
    }
}

impl EventsSubscriber {
    /// Await the next event. Returns `None` if the channel is closed
    /// (router cancelled). Lagged is incremented by the producer
    /// (router) when a `try_send` fails; this method does NOT mutate
    /// the counter.
    pub async fn recv(&mut self) -> Option<InboundEvent> {
        self.rx.recv().await
    }

    pub fn lagged(&self) -> u64 {
        self.lagged.load(Ordering::Relaxed)
    }

    /// Try to receive without awaiting. Returns `None` if no event is
    /// available right now (channel still open but empty).
    pub fn try_recv(&mut self) -> Option<InboundEvent> {
        self.rx.try_recv().ok()
    }
}

/// Central event router. Holds the source receiver, the buffer, and
/// the set of registered sinks. `spawn` returns a `JoinHandle` that
/// completes when the source channel closes (adapter disconnected)
/// or the cancel token fires.
pub struct EventsRouter {
    buffer: Arc<EventsBuffer>,
    sinks: parking_lot::Mutex<Vec<Arc<EventsSink>>>,
    cancel: CancellationToken,
    /// Phase 5 Part B: optional Prometheus hook. Increments
    /// `inbound_events_total{kind=hash}` per parsed event.
    metrics: Option<Arc<Metrics>>,
    /// Phase 5 Part F: optional action-dispatch hook. Called once per
    /// parsed event after buffer push + sink fanout. Closure runs
    /// synchronously in the router's own task — slow dispatchers must
    /// themselves spawn work to avoid stalling the event loop.
    action_hook: Option<Arc<dyn Fn(crate::events::InboundEvent) + Send + Sync>>,
}

impl std::fmt::Debug for EventsRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventsRouter")
            .field("sinks", &self.sink_count())
            .field("total_lagged", &self.total_lagged())
            .field("metrics", &self.metrics.is_some())
            .finish_non_exhaustive()
    }
}

impl EventsRouter {
    /// Build a new router. Source is a broadcast::Receiver<String>
    /// from `octo-adapter-whatsapp::WhatsAppWebAdapter::subscribe_raw_events()`.
    /// Buffer is the daemon's `EventsBuffer`.
    pub fn new(buffer: Arc<EventsBuffer>, cancel: CancellationToken) -> Arc<Self> {
        Arc::new(Self {
            buffer,
            sinks: parking_lot::Mutex::new(Vec::new()),
            cancel,
            metrics: None,
            action_hook: None,
        })
    }

    /// Construct from owned parts. Used by `DaemonHandle::build_event_router`
    /// in test-helpers mode where the broadcast source is not bound.
    pub fn from_parts(buffer: Arc<EventsBuffer>, cancel: CancellationToken) -> Self {
        Self {
            buffer,
            sinks: parking_lot::Mutex::new(Vec::new()),
            cancel,
            metrics: None,
            action_hook: None,
        }
    }

    /// Phase 5 Part B: attach the Prometheus registry. Idempotent.
    pub fn with_metrics(mut self, m: Arc<Metrics>) -> Self {
        self.metrics = Some(m);
        self
    }

    /// Phase 5 Part F: register an action dispatch hook. Called for
    /// every parsed inbound event after buffer push + sink fanout.
    /// The hook runs on the router's own task (fire-and-forget — the
    /// event loop must not block on slow action latencies).
    pub fn with_action_dispatcher<F>(mut self, hook: F) -> Self
    where
        F: Fn(crate::events::InboundEvent) + Send + Sync + 'static,
    {
        self.action_hook = Some(Arc::new(hook));
        self
    }

    /// Register a new sink. The returned `EventsSubscriber` is the
    /// consumer side. Each sink has its own bounded mpsc; the
    /// capacity is `capacity` events (drops beyond that increment
    /// the sink's Lagged counter).
    pub fn subscribe(self: &Arc<Self>, capacity: usize) -> EventsSubscriber {
        let (tx, rx) = mpsc::channel(capacity);
        let lagged = Arc::new(AtomicU64::new(0));
        // Auto-generated sink names: each subscribe() without an
        // explicit name gets "sink-N" so operators always see a
        // non-empty label in status.
        let name = format!("sink-{}", self.sinks.lock().len());
        let name: &'static str = Box::leak(name.into_boxed_str());
        let sink = Arc::new(EventsSink {
            tx,
            lagged: lagged.clone(),
            name,
        });
        self.sinks.lock().push(sink);
        EventsSubscriber { rx, lagged }
    }

    /// Like [`Self::subscribe`] but tags the sink with a stable,
    /// human-readable name (e.g. `"persister"`, `"query"`). Used by
    /// production wiring so `status.sink_lagged_total` shows the
    /// real consumer identity instead of numeric placeholders.
    pub fn subscribe_named(
        self: &Arc<Self>,
        capacity: usize,
        name: &'static str,
    ) -> EventsSubscriber {
        let (tx, rx) = mpsc::channel(capacity);
        let lagged = Arc::new(AtomicU64::new(0));
        let sink = Arc::new(EventsSink {
            tx,
            lagged: lagged.clone(),
            name,
        });
        self.sinks.lock().push(sink);
        EventsSubscriber { rx, lagged }
    }

    /// Snapshot of every registered sink as `(name, lagged)`. Used
    /// by `status.get` to surface per-consumer drop counts.
    pub fn sink_lagged_snapshot(&self) -> Vec<(String, u64)> {
        self.sinks
            .lock()
            .iter()
            .map(|s| (s.name.to_string(), s.lagged()))
            .collect()
    }

    /// Number of registered sinks.
    pub fn sink_count(&self) -> usize {
        self.sinks.lock().len()
    }

    /// Sum of all sinks' Lagged counters.
    pub fn total_lagged(&self) -> u64 {
        self.sinks.lock().iter().map(|s| s.lagged()).sum()
    }

    /// Main loop. Spawn this on a tokio runtime:
    /// ```ignore
    /// tokio::spawn(router.clone().run(mut raw_rx));
    /// ```
    ///
    /// On every raw event:
    /// 1. Parse to `InboundEvent`.
    /// 2. Persist to `buffer.push`.
    /// 3. For each sink: `try_send` a clone; on `Full`/`Closed` increment sink.lagged.
    pub async fn run(self: Arc<Self>, mut raw_rx: tokio::sync::broadcast::Receiver<String>) {
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    tracing::info!("events_router: cancelled; exiting");
                    break;
                }
                recv = raw_rx.recv() => {
                    match recv {
                        Ok(raw) => {
                            // Parse the envelope into one or more
                            // events. A `Messages(MessageBatch { ... })`
                            // envelope fans out to one event per inner
                            // message so group conversations land as
                            // searchable rows instead of an opaque
                            // Unknown (see events::parse_many).
                            let events = InboundEvent::parse_many(
                                EventEnvelope {
                                    raw,
                                    ts_unix_ms: 0,
                                    ts_mono_ns: 0,
                                },
                                None,
                            );
                            for ev in events {
                                if let Some(m) = &self.metrics {
                                    let kind = event_kind_label(&ev);
                                    m.inc_inbound_event(&kind);
                                }
                                self.buffer.push(ev.clone());
                                self.fanout(ev.clone());
                                // Phase 5 Part F: fire the action-dispatch hook
                                // (if registered) on a clone so the buffer
                                // entry + sink fan-out are unaffected.
                                if let Some(hook) = self.action_hook.as_ref() {
                                    let hook = hook.clone();
                                    let ev2 = ev.clone();
                                    tokio::spawn(async move {
                                        hook(ev2);
                                    });
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            // The raw bus is lossy by design (capacity 1000).
                            // We don't recover the dropped events here — the
                            // sink consumer uses `events.list --since-id` to
                            // backfill per design §Loss recovery.
                            tracing::warn!(lagged = n, "events_router: raw bus lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::info!("events_router: raw bus closed; exiting");
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Send a clone of `ev` to every sink. Per-sink `try_send` errors
    /// (`Full` or `Closed`) increment that sink's `lagged` counter.
    /// This is the only place that touches the sinks lock during
    /// steady-state operation; it is never held across `.await`.
    fn fanout(&self, ev: InboundEvent) {
        let sinks = self.sinks.lock();
        for sink in sinks.iter() {
            match sink.tx.try_send(ev.clone()) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    // Slow consumer: drop the event for this sink,
                    // count it, and surface the first drop of a burst
                    // as a warn! so operators see the incident in the
                    // log instead of only in the post-mortem counter.
                    // Subsequent drops in the same burst stay quiet
                    // to avoid log floods (the count remains in
                    // status.sink_lagged_total).
                    let now = sink.lagged.fetch_add(1, Ordering::Relaxed) + 1;
                    if now == 1 || now.is_power_of_two() {
                        tracing::warn!(
                            sink = sink.name,
                            lagged_total = now,
                            capacity = sink.tx.capacity(),
                            "events_router: sink lagged (mpsc full); event dropped for this consumer"
                        );
                    }
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    // Sink consumer dropped its `EventsSubscriber`.
                    // Count it as lagged and skip — the dead sink will
                    // be removed by the next call to `subscribe` or by
                    // the router's own pruning pass.
                    sink.lagged.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        sink = sink.name,
                        "events_router: sink closed (consumer dropped)"
                    );
                }
            }
        }
    }
}

/// Phase 5 Part B: stable per-event-kind string used as the
/// `inbound_events_total{kind}` label pre-hash.
fn event_kind_label(ev: &InboundEvent) -> String {
    match ev {
        InboundEvent::Message { .. } => "message".into(),
        InboundEvent::Reaction { .. } => "reaction".into(),
        InboundEvent::Receipt { .. } => "receipt".into(),
        InboundEvent::GroupChange { .. } => "group_change".into(),
        InboundEvent::Presence { .. } => "presence".into(),
        InboundEvent::Connection { .. } => "connection".into(),
        InboundEvent::Call { .. } => "call".into(),
        InboundEvent::Story { .. } => "story".into(),
        InboundEvent::Unknown { .. } => "unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    fn make_router(
        buffer_capacity: usize,
    ) -> (Arc<EventsRouter>, Arc<EventsBuffer>, CancellationToken) {
        let buffer = EventsBuffer::new(buffer_capacity);
        let cancel = CancellationToken::new();
        let router = EventsRouter::new(buffer.clone(), cancel.clone());
        (router, buffer, cancel)
    }

    fn dummy_msg(id: &str) -> String {
        format!(
            "Message(id: \"{id}\", peer: \"P\", sender: \"S\", text: \"hi\", kind: Text, is_group: false)"
        )
    }

    #[tokio::test]
    async fn router_persists_and_fans_out_to_sinks() {
        let (router, buffer, _cancel) = make_router(100);
        let (tx, _rx) = broadcast::channel::<String>(16);
        let mut sub = router.subscribe(8);

        let router2 = router.clone();
        let rx = tx.subscribe();
        let handle = tokio::spawn(async move { router2.run(rx).await });

        tx.send(dummy_msg("M1")).unwrap();
        tx.send(dummy_msg("M2")).unwrap();

        // Wait for both events to land.
        let e1 = sub.recv().await.expect("first event");
        let e2 = sub.recv().await.expect("second event");

        // Buffer has both.
        assert_eq!(buffer.len(), 2);

        // First sink event has id M1, second M2.
        let ids: Vec<String> = [&e1, &e2]
            .iter()
            .map(|e| match e {
                InboundEvent::Message { id, .. } => id.clone(),
                _ => panic!("expected Message"),
            })
            .collect();
        assert_eq!(ids, vec!["M1", "M2"]);

        drop(sub);
        drop(tx);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn slow_sink_increments_lagged_counter() {
        let (router, _buffer, _cancel) = make_router(100);
        let (tx, _rx) = broadcast::channel::<String>(16);

        // Capacity 1 → second event will fail TrySendError::Full.
        let _sub = router.subscribe(1);

        let router2 = router.clone();
        let rx = tx.subscribe();
        let handle = tokio::spawn(async move { router2.run(rx).await });

        tx.send(dummy_msg("M1")).unwrap();
        tx.send(dummy_msg("M2")).unwrap();
        tx.send(dummy_msg("M3")).unwrap();

        // Give the router a beat to drain.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let lagged = router.total_lagged();
        assert!(
            lagged >= 1,
            "expected at least 1 lagged event, got {lagged}"
        );

        drop(tx);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn router_cancellation_exits_run_loop() {
        let (router, _buffer, cancel) = make_router(100);
        let (_tx, _rx) = broadcast::channel::<String>(16);
        let rx = _tx.subscribe();

        let router2 = router.clone();
        let handle = tokio::spawn(async move { router2.run(rx).await });

        cancel.cancel();
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("router should exit on cancel within 1s")
            .expect("router task panicked");
    }
}
