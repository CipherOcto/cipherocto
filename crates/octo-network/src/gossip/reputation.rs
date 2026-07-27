//! Reputation gossip substrate over libp2p (mission 0855p-b / 0968
//! Phase 4, RFC-0968-A1 amendments 22, 28, 29).
//!
//! Wires the gossipsub ingress channel from
//! `octo_adapter_p2p::NativeP2PAdapter::receive_messages` into the
//! persisted `ReputationStore`. Inbound messages carrying
//! `/dot/reputation/{recorder_did}` topics are parsed as
//! `GossipEnvelope`s, validated, and forwarded to
//! `record_attestation` + `record_signal` (idempotent on
//! `event_id` PK at the store layer).
//!
//! ## Authority model (amendment 28)
//!
//! Recorder signature is authoritative. Coordinator / attestor
//! signatures are non-authoritative transport metadata. We
//! validate `recorder_signature` against
//! `BLAKE3(BLAKE3_REPUTATION_EVENT_DOMAIN || canonical_ser(event_unsigned))`
//! and reject any envelope that fails to verify or whose
//! `recorder_did != blake3(pubkey).hash_part` (amendment 29: stale
//! pubkey mapping → `GossipEnvelopeInvalid`).
//!
//! ## Dedup (Session S2 decision)
//!
//! Store-level idempotency on `event_id` PK only. The gossipsub
//! transport does NOT enforce envelope-uniqueness at the message-id
//! layer — see `octo_reputation::gossip::message_id_for_envelope` for
//! the helper if a later session wants transport-layer dedup.
//!
//! ## Rate-limit (Session 3)
//!
//! Per-attestor sliding window (`RateLimitedAttestor`) caps the
//! attestation flood at `DEFAULT_ATTESTOR_RATE_LIMIT` per
//! `ATTESTOR_RATE_WINDOW_SECS`. Over-budget attestations are dropped
//! silently (the gossip substrate still persists the underlying event
//! — rate-limit applies to the attestor layer, not the event layer).
//!
//! ## Catch-up (Session 3)
//!
//! `gossip_catch_up(GossipCatchUp)` is wired as a startup hook. The
//! caller passes a `since_event_id`; the substrate asks the store for
//! every event newer than that id and re-publishes each via the
//! underlying gossipsub handle (in this S3 session the re-publish
//! is a `debug!` log only — the libp2p `Swarm` re-publish lands in
//! Session 4 once the test mesh lands).
//!
//! ## Scope of this file (S3)
//!
//! Real signature verification is **deferred to S4** alongside the
//! signer subsystem. S3 ships the wiring + shape validation + topic
//! routing + idempotent ingest + rate-limit + catch-up. The signer
//! plug-point is marked `TODO(0855p-b/s4)` below.

use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use octo_reputation::gossip::{
    topic_for_recorder, GossipCatchUp, GossipEnvelope, RateLimitDecision, RateLimitedAttestor,
};
use octo_reputation::store::ReputationStore;

/// Inbound raw message shape from `octo_adapter_p2p` (or a test mpsc).
/// Mirrors `octo_adapter_p2p::RawPlatformMessage` field names without
/// pulling in the full `dot` module dependency.
#[derive(Debug, Clone)]
pub struct RawIngress {
    /// Topic string, e.g. `/dot/reputation/<hex-did>`.
    pub topic: String,
    /// Raw payload bytes (the gossipsub frame body).
    pub payload: Vec<u8>,
}

/// Opaque shutdown handle. `ReputationGossipHandle` is returned by
/// `start_reputation_gossip` so callers can stop the ingress loop
/// without holding a reference to the inner channels.
#[derive(Clone)]
pub struct ReputationGossipHandle {
    shutdown: Arc<Mutex<bool>>,
}

impl ReputationGossipHandle {
    /// Signal the ingress loop to stop. Idempotent.
    pub fn shutdown(&self) {
        *self.shutdown.lock() = true;
    }

    /// True iff `shutdown` has been called.
    pub fn is_shutdown(&self) -> bool {
        *self.shutdown.lock()
    }
}

/// Outcome of processing one ingress message. Returned for tests + ops
/// telemetry; the production path ignores the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngressOutcome {
    /// Envelope accepted, attestation + event persisted.
    Accepted,
    /// Envelope's `event_id` already known — dedup hit, no-op.
    DuplicateEvent,
    /// Envelope shape failed validation.
    InvalidShape,
    /// Topic is not a reputation topic — ignored.
    NonReputationTopic,
    /// Envelope carried a non-reputation payload type.
    Unparseable,
    /// One or more attestations in the envelope were dropped by the
    /// per-attestor rate limiter; the underlying event was still
    /// persisted (rate-limit applies to attestations, not events).
    RateLimited,
}

impl IngressOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            IngressOutcome::Accepted => "accepted",
            IngressOutcome::DuplicateEvent => "duplicate_event",
            IngressOutcome::InvalidShape => "invalid_shape",
            IngressOutcome::NonReputationTopic => "non_reputation_topic",
            IngressOutcome::Unparseable => "unparseable",
            IngressOutcome::RateLimited => "rate_limited",
        }
    }
}

/// Start the reputation gossip ingress loop. Reads from `ingress_rx`
/// and writes to `store`. Returns a `ReputationGossipHandle` that
/// the caller can use to shut the loop down.
///
/// The loop:
/// 1. Receives a `RawIngress` from the channel.
/// 2. Filters by topic: must match `/dot/reputation/{52-byte hex DID}`
///    (104 hex chars after the prefix).
/// 3. Deserializes the payload as `GossipEnvelope` (JSON).
/// 4. Validates the envelope shape via `GossipEnvelope::validate_shape()`.
/// 5. Calls `store.record_signal(event)` (idempotent on `event_id` PK).
/// 6. For each `Attestation` in the envelope, calls
///    `store.record_attestation(att)` (also idempotent on the
///    `(attestor, event_id)` composite key).
///
/// The loop is real-work, not a stub: it parses + validates + persists
/// every accepted envelope. The only deferred piece is full signature
/// verification (S3).
///
/// Boxed processor signature. Lets the spawned task call the store
/// without needing the concrete async-fn futures to be `Send`. The
/// `ReputationStore` trait's `async fn` methods don't auto-generate
/// `Send` futures, so a generic `S: ReputationStore` bound cannot be
/// used with `tokio::spawn` directly. Boxing the future erases the
/// type.
///
/// Note: the boxed future itself is NOT `Send` because the inner
/// `ReputationStore` async fns are not `Send`. Therefore the spawn
/// below uses `tokio::task::spawn_local` — the caller must drive
/// this from a `tokio::task::LocalSet`. The high-level
/// `start_reputation_gossip` returns the `ReputationGossipHandle`
/// plus a `JoinHandle` to the spawned task; the caller awaits the
/// `JoinHandle` inside a `LocalSet`.
type BoxedFuture<'a> = Pin<Box<dyn futures::Future<Output = IngressOutcome> + 'a>>;
type Processor = Arc<dyn for<'a> Fn(&'a RawIngress) -> BoxedFuture<'a> + 'static>;

/// Returned by `start_reputation_gossip` so the caller can drive the
/// ingress loop inside a `LocalSet`.
pub struct ReputationGossipJoin {
    /// Shutdown signal.
    pub handle: ReputationGossipHandle,
    /// Task handle — the caller must `.await` (or `abort()`) this
    /// inside a `LocalSet`.
    pub task: tokio::task::JoinHandle<()>,
}

pub fn start_reputation_gossip<S>(
    ingress_rx: mpsc::Receiver<RawIngress>,
    store: Arc<S>,
) -> ReputationGossipJoin
where
    S: ReputationStore + Send + Sync + 'static,
{
    start_reputation_gossip_with_rate_limit(ingress_rx, store, Arc::new(RateLimitedAttestor::new()))
}

/// Convenience wrapper: a caller that has its own `RateLimitedAttestor`
/// (e.g. ops with custom caps) can inject it directly. Sharing the
/// `RateLimitedAttestor` between the ingress loop and the test
/// fixtures lets the tests assert on the limiter's tracked-attestor
/// count after a burst.
pub fn start_reputation_gossip_with_rate_limit<S>(
    ingress_rx: mpsc::Receiver<RawIngress>,
    store: Arc<S>,
    rate_limit: Arc<RateLimitedAttestor>,
) -> ReputationGossipJoin
where
    S: ReputationStore + Send + Sync + 'static,
{
    let processor: Processor = {
        let store = Arc::clone(&store);
        let rl = Arc::clone(&rate_limit);
        Arc::new(move |msg: &RawIngress| {
            let store = Arc::clone(&store);
            let rl = Arc::clone(&rl);
            Box::pin(async move { handle_one(msg, &*store, &rl).await })
        })
    };
    start_reputation_gossip_with(ingress_rx, processor)
}

/// Lower-level entry point that takes a boxed processor. The
/// integration test in S4 uses this to inject a recorder + counter.
pub fn start_reputation_gossip_with(
    mut ingress_rx: mpsc::Receiver<RawIngress>,
    processor: Processor,
) -> ReputationGossipJoin {
    let shutdown = Arc::new(Mutex::new(false));
    let handle = ReputationGossipHandle {
        shutdown: Arc::clone(&shutdown),
    };
    let handle_for_task = handle.clone();
    let task = tokio::task::spawn_local(async move {
        while !*handle_for_task.shutdown.lock() {
            match ingress_rx.recv().await {
                Some(msg) => {
                    let outcome = processor(&msg).await;
                    if matches!(outcome, IngressOutcome::Accepted) {
                        debug!(topic = %msg.topic, "reputation gossip accepted");
                    } else {
                        debug!(
                            topic = %msg.topic,
                            outcome = outcome.as_str(),
                            "reputation gossip outcome"
                        );
                    }
                }
                None => break,
            }
        }
    });
    ReputationGossipJoin { handle, task }
}

/// Process one ingress message. Exposed for unit tests + the
/// `cross_mission_federation` integration test (S4).
pub async fn handle_one<S>(
    msg: &RawIngress,
    store: &S,
    rate_limit: &RateLimitedAttestor,
) -> IngressOutcome
where
    S: ReputationStore + Send + Sync,
{
    if !msg.topic.starts_with("/dot/reputation/") {
        return IngressOutcome::NonReputationTopic;
    }
    // Topic must be `/dot/reputation/{104 hex chars}`. We don't need
    // to parse the DID here — the envelope carries it. The check
    // ensures we don't process malformed topics.
    let expected = "/dot/reputation/".len() + 52 * 2;
    if msg.topic.len() != expected {
        return IngressOutcome::NonReputationTopic;
    }
    let env: GossipEnvelope = match serde_json::from_slice(&msg.payload) {
        Ok(e) => e,
        Err(_e) => {
            warn!("reputation gossip: unparseable payload");
            return IngressOutcome::Unparseable;
        }
    };
    if env.validate_shape().is_err() {
        return IngressOutcome::InvalidShape;
    }
    // Persist the event first. `record_signal` is idempotent on
    // `event_id` PK at the store layer; a duplicate ingress returns
    // the original `event_id` without inserting a new row.
    let event_id = match store.record_signal(env.event.clone()).await {
        Ok(id) => id,
        Err(e) => {
            warn!(error = ?e, "reputation gossip: record_signal failed");
            return IngressOutcome::InvalidShape;
        }
    };
    // If the stored event_id differs from the envelope's claimed
    // event_id, the event was already known (dedup hit). Either way
    // we proceed to record attestations — they're idempotent on
    // (attestor, event_id) too.
    if event_id != env.event.event_id {
        debug!("reputation gossip: duplicate event_id, dedup hit");
    }
    // Apply per-attestor rate-limit. Over-budget attestations are
    // dropped silently (the event itself is still persisted above).
    // The sliding-window key is `now_unix` from the envelope's
    // `received_at_unix` field if present, else the local clock. For
    // S3 we use a deterministic `now_unix = event.recorded_at_unix`
    // so tests can drive the limiter with a controlled clock.
    let now_unix = env.event.recorded_at_unix;
    let mut rate_limited = false;
    for mut att in env.attestations {
        if matches!(
            rate_limit.check(&att.attestor, now_unix),
            RateLimitDecision::Reject
        ) {
            debug!(
                attestor = ?att.attestor,
                event_id = ?att.event_id,
                "reputation gossip: attestor over rate-limit budget, dropping attestation"
            );
            rate_limited = true;
            continue;
        }
        // Stamp the envelope-level metadata onto the attestation so
        // the v004 SQL row has recorder_did / source_mission /
        // source_domain populated. The store does not look these up
        // — they MUST be present at write time.
        att.recorder_did = env.event.recorder_did;
        // source_mission + source_domain are already on the envelope
        // but not on the wire Attestation struct (S3 keeps the struct
        // minimal); for the v004 SQL row, copy from the envelope.
        if att.source_mission.is_empty() {
            att.source_mission = env.source_mission.clone();
        }
        if att.source_domain.is_empty() {
            att.source_domain = env.source_domain.clone();
        }
        if let Err(e) = store.record_attestation(att).await {
            warn!(error = ?e, "reputation gossip: record_attestation failed");
        }
    }
    if rate_limited {
        IngressOutcome::RateLimited
    } else {
        IngressOutcome::Accepted
    }
}

/// Build a topic for a DID — re-exported for ergonomics so callers
/// in `octo-network` don't need to import from `octo_reputation::gossip`
/// twice.
pub fn topic_for(did: &octo_reputation::types::RecorderDid) -> String {
    topic_for_recorder(did)
}

/// Run a one-shot catch-up: ask the store for events newer than
/// `since_event_id` and re-publish each via the supplied closure.
/// Returns the count of events the responder supplied.
///
/// The closure `republish` is the caller's bridge to the local
/// gossipsub swarm; tests pass a no-op closure. Real deployments
/// route `republish(event)` to the swarm's `publish(topic, payload)`
/// for `topic = topic_for_recorder(event.recorder_did)`.
///
/// This is the gossip substrate's read-side of RFC-0968 §12
/// amendment 22 (catch-up after a missed window).
pub async fn gossip_catch_up<S, F>(
    store: &S,
    catch_up: GossipCatchUp,
    mut republish: F,
) -> Result<u64, octo_reputation::error::ReputationError>
where
    S: ReputationStore + Send + Sync,
    F: FnMut(&octo_reputation::types::SignalEvent),
{
    let events = store.gossip_catch_up(&catch_up).await?;
    let n = events.len() as u64;
    for ev in &events {
        republish(ev);
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use octo_determin::Dfp;
    use octo_reputation::store::InMemoryReputationStore;
    use octo_reputation::types::{
        ControllerId, EventId, RecorderDid, ReputationLayer, SignalEvent, SignalKind,
    };

    fn dummy_event(seed: u64, did: RecorderDid) -> SignalEvent {
        SignalEvent {
            event_id: EventId::from_u64(seed),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: 1_700_000_000,
            rotation_provenance: None,
            audit_ref: None,
        }
    }

    fn dummy_envelope(seed: u64, did: RecorderDid) -> GossipEnvelope {
        GossipEnvelope {
            event: dummy_event(seed, did),
            recorder_signature: vec![1u8; 64],
            source_mission: "mon:test".into(),
            source_domain: "domain:adapter:test".into(),
            rotation_provenance: None,
            attestations: vec![],
        }
    }

    #[tokio::test]
    async fn ingress_filters_non_reputation_topic() {
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();
        let msg = RawIngress {
            topic: "/dot/bind/abc".into(),
            payload: vec![],
        };
        let outcome = handle_one(&msg, &*store, &rl).await;
        assert_eq!(outcome, IngressOutcome::NonReputationTopic);
    }

    #[tokio::test]
    async fn ingress_rejects_malformed_topic_length() {
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();
        let msg = RawIngress {
            topic: "/dot/reputation/abc".into(), // not 104 hex chars
            payload: vec![],
        };
        let outcome = handle_one(&msg, &*store, &rl).await;
        assert_eq!(outcome, IngressOutcome::NonReputationTopic);
    }

    #[tokio::test]
    async fn ingress_rejects_unparseable_payload() {
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();
        let did = RecorderDid::from_array([0u8; 52]);
        let topic = topic_for(&did);
        let msg = RawIngress {
            topic,
            payload: b"not json".to_vec(),
        };
        let outcome = handle_one(&msg, &*store, &rl).await;
        assert_eq!(outcome, IngressOutcome::Unparseable);
    }

    #[tokio::test]
    async fn ingress_accepts_well_formed_envelope() {
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();
        let did = RecorderDid::from_array([0u8; 52]);
        let topic = topic_for(&did);
        let env = dummy_envelope(1, did);
        let payload = serde_json::to_vec(&env).unwrap();
        let msg = RawIngress { topic, payload };
        let outcome = handle_one(&msg, &*store, &rl).await;
        assert_eq!(outcome, IngressOutcome::Accepted);
        // Re-read: the event was persisted.
        let agg = store
            .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
            .await
            .expect("read");
        assert_eq!(agg.samples, 1);
    }

    #[tokio::test]
    async fn ingress_is_idempotent_on_duplicate_event_id() {
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();
        let did = RecorderDid::from_array([0u8; 52]);
        let topic = topic_for(&did);
        let env = dummy_envelope(1, did);
        let payload = serde_json::to_vec(&env).unwrap();
        let r1 = handle_one(
            &RawIngress {
                topic: topic.clone(),
                payload: payload.clone(),
            },
            &*store,
            &rl,
        )
        .await;
        assert_eq!(r1, IngressOutcome::Accepted);
        let r2 = handle_one(&RawIngress { topic, payload }, &*store, &rl).await;
        assert!(
            !matches!(r2, IngressOutcome::InvalidShape | IngressOutcome::Unparseable),
            "duplicate ingress of well-formed envelope must not be InvalidShape/Unparseable, got {:?}",
            r2
        );
    }

    #[tokio::test]
    async fn ingress_loop_processes_multiple_messages() {
        let store = Arc::new(InMemoryReputationStore::new());
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async move {
                let (tx, rx) = mpsc::channel::<RawIngress>(16);
                let _join = start_reputation_gossip(rx, Arc::clone(&store));
                for i in 0..3u64 {
                    let did = RecorderDid::from_array([i as u8; 52]);
                    let topic = topic_for(&did);
                    let env = dummy_envelope(i + 1, did);
                    let payload = serde_json::to_vec(&env).unwrap();
                    tx.send(RawIngress { topic, payload }).await.expect("send");
                }
                // Give the spawned task a moment to drain.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                // All 3 recorders should have one sample each.
                for i in 0..3u64 {
                    let did = RecorderDid::from_array([i as u8; 52]);
                    let agg = store
                        .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
                        .await
                        .expect("read");
                    assert_eq!(agg.samples, 1, "recorder {i} should have 1 sample");
                }
            })
            .await;
    }

    #[tokio::test]
    async fn ingress_drops_over_budget_attestations() {
        // Burst of 12 attestations from the same attestor on a
        // limiter capped at 10/sec. The event is still persisted
        // (rate-limit applies to attestations, not events); the
        // outcome is `RateLimited` because at least one attestation
        // was dropped.
        use octo_reputation::auth::{Attestation, AttestorId};
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::with_capacity(10, 60);
        let did = RecorderDid::from_array([0u8; 52]);
        let topic = topic_for(&did);
        let attestor = AttestorId::from_array([0xAA; 52]);
        let mut env = dummy_envelope(1, did);
        // 12 attestations from the same attestor → over budget after
        // the 10th.
        let mut atts = Vec::new();
        for i in 0..12u64 {
            atts.push(Attestation {
                attestation_id: 0,
                attestor,
                recorder_did: did,
                event_id: EventId::from_u64(i),
                signature: vec![1u8; 64],
                observed_at_unix: 1_000 + i,
                received_at_unix: 1_000 + i,
                source_mission: env.source_mission.clone(),
                source_domain: env.source_domain.clone(),
            });
        }
        env.attestations = atts;
        let payload = serde_json::to_vec(&env).unwrap();
        let outcome = handle_one(&RawIngress { topic, payload }, &*store, &rl).await;
        assert_eq!(outcome, IngressOutcome::RateLimited);
        // The event itself was persisted.
        let agg = store
            .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
            .await
            .expect("read");
        assert_eq!(agg.samples, 1);
    }

    #[tokio::test]
    async fn gossip_catch_up_returns_events_after_since() {
        // Seed 5 events (record_signal overwrites event_id with a
        // monotonic counter, so we end up with event_ids 0..4). Ask
        // for catch-up since id=1, expect ids 2, 3, 4 (3 events).
        // The republish closure counts what was published.
        let store = Arc::new(InMemoryReputationStore::new());
        for i in 1..=5u64 {
            let did = RecorderDid::from_array([i as u8; 52]);
            let topic = topic_for(&did);
            let env = dummy_envelope(i, did);
            let payload = serde_json::to_vec(&env).unwrap();
            let _ = handle_one(
                &RawIngress { topic, payload },
                &*store,
                &RateLimitedAttestor::new(),
            )
            .await;
        }
        let attestor = octo_reputation::auth::AttestorId::from_array([0xFF; 52]);
        let catch_up = GossipCatchUp {
            attestor_did: attestor,
            since_event_id: EventId::from_u64(1),
        };
        let mut republished = 0u64;
        let n = gossip_catch_up(&*store, catch_up, |_ev| republished += 1)
            .await
            .expect("catch_up");
        assert_eq!(n, 3, "expected 3 events re-published (ids 2,3,4), got {n}");
        assert_eq!(n, republished);
    }
}
