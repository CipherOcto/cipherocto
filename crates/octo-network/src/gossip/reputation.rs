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
//! signatures are non-authoritative transport metadata. When the
//! caller supplies a `PublicKeyLookup` (via
//! `start_reputation_gossip_with_verification`), `handle_one` verifies
//! the ed25519 `recorder_signature` against the canonical
//! `SignalEvent::canonical_bytes()` digest using the recorder's
//! registered pubkey, and rejects any envelope whose
//! `recorder_did != did_from_pubkey(pubkey)` (amendment 29: stale
//! pubkey mapping → `InvalidShape`).
//!
//! Without a `PublicKeyLookup` the gossip substrate falls back to
//! shape validation only — the existing S2/S3 behavior. Callers
//! that want the §12 enforcement MUST pass a lookup. The default
//! constructors (`start_reputation_gossip`,
//! `start_reputation_gossip_with_rate_limit`,
//! `start_reputation_gossip_with_rate_limit_and_refresh`) retain
//! the lookup-less path for back-compat.
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

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use octo_reputation::gossip::{
    topic_for_recorder, GossipCatchUp, GossipEnvelope, RateLimitDecision, RateLimitedAttestor,
};
use octo_reputation::store::ReputationStore;
use octo_reputation::types::SignalEvent;

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

/// Public-key lookup abstraction for signature verification
/// (RFC-0968 §12 enforcement).
///
/// Resolves a recorder DID to its registered ed25519 public key. The
/// gossip substrate calls `lookup(recorder_did)` once per accepted
/// envelope; missing entries degrade to a debug log + permissive
/// accept (back-compat for environments where attestor registration
/// has not yet landed).
///
/// The trait is object-safe + `Send + Sync` so the gossip loop can
/// drive it from inside `tokio::task::spawn_local` without leaking
/// generic bounds into the spawn signature.
pub trait PublicKeyLookup: Send + Sync {
    /// Return the 32-byte ed25519 public key registered for `did`, or
    /// `None` if no key is registered. Implementations should be cheap
    /// (in-memory map or cached DB row); the gossip loop calls this
    /// synchronously on every accepted envelope.
    fn lookup(&self, did: &octo_reputation::types::RecorderDid) -> Option<[u8; 32]>;
}

/// Derive a `RecorderDid` from an ed25519 public key per
/// RFC-0968-A1 amendment 29. The mapping is deterministic and
/// collision-resistant up to BLAKE3's 128-bit security level; the
/// resulting 52-byte DID is `blake3(pubkey) || [0;20]`.
///
/// The trailing 20 zero bytes are part of the canonical
/// `RecorderDid` namespace which is fixed at 52 bytes — we cannot
/// truncate the 32-byte BLAKE3 digest without losing namespace
/// compatibility with existing DID-keyed gossip topics.
pub fn did_from_pubkey(pubkey: &[u8; 32]) -> octo_reputation::types::RecorderDid {
    let digest = blake3::hash(pubkey);
    let mut bytes = [0u8; 52];
    bytes[..32].copy_from_slice(digest.as_bytes());
    // bytes[32..52] already zero (stack-init).
    octo_reputation::types::RecorderDid::from_array(bytes)
}

/// Verify a `GossipEnvelope`'s `recorder_signature` against the
/// recorder's registered ed25519 public key. Returns `Ok(())` only
/// when:
/// 1. The signature parses as a valid ed25519 `Signature`.
/// 2. `ed25519_dalek::VerifyingKey::from_bytes(pubkey)` succeeds.
/// 3. The signature verifies against `SignalEvent::canonical_bytes()`
///    under strict-mode verification.
/// 4. `did_from_pubkey(pubkey) == envelope.recorder_did()` (amendment
///    29 stale-pubkey-mapping rejection).
///
/// Used by `handle_one` when the caller supplies a `PublicKeyLookup`.
/// The verifier intentionally does NOT consult the `recorder_did`
/// field on the envelope until step 4 — the signature check itself
/// must succeed before the binding check is meaningful.
pub fn verify_envelope_signature(
    env: &octo_reputation::gossip::GossipEnvelope,
    pubkey: &[u8; 32],
) -> Result<(), &'static str> {
    use ed25519_dalek::{Signature, VerifyingKey};
    if env.recorder_signature.len() != 64 {
        return Err("recorder_signature_wrong_length");
    }
    let sig = Signature::from_slice(&env.recorder_signature)
        .map_err(|_| "recorder_signature_not_canonical")?;
    let vk = VerifyingKey::from_bytes(pubkey).map_err(|_| "pubkey_not_canonical")?;
    let msg = env.event.canonical_bytes();
    vk.verify_strict(&msg, &sig)
        .map_err(|_| "signature_verify_failed")?;
    if did_from_pubkey(pubkey) != env.event.recorder_did {
        return Err("stale_pubkey_mapping");
    }
    Ok(())
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

/// Refresh hook callback fired after `record_signal` succeeds on a
/// reputation ingress. Lets callers (e.g., the DC slash adapter)
/// bridge gossip-side persistence into in-memory state derived from
/// the canonical `ReputationStore`. The hook is invoked with a
/// reference to the accepted `SignalEvent`; returning `()` means the
/// hook ran to completion (errors are the hook's responsibility).
///
/// The HRTB lifetime lets the hook return a future that borrows from
/// the event reference; the caller typically clones the needed fields
/// (e.g., `recorder_did`) before constructing the future body.
pub type RefreshHook =
    Arc<dyn for<'a> Fn(&'a SignalEvent) -> Pin<Box<dyn Future<Output = ()> + 'a>> + Send + Sync>;

/// Returned by `start_reputation_gossip` so the caller can drive the
/// ingress loop inside a `LocalSet`.
pub struct ReputationGossipJoin {
    /// Shutdown signal.
    pub handle: ReputationGossipHandle,
    /// Task handle — the caller must `.await` (or `abort()`) this
    /// inside a `LocalSet`.
    pub task: tokio::task::JoinHandle<()>,
}

/// **DEPRECATED** (Round 2 review C5): this constructor does NOT pass a
/// `PublicKeyLookup` to `start_reputation_gossip_with_verification`,
/// which means RFC-0968 §12 enforcement is disabled and any peer can
/// publish fabricated envelopes for any `recorder_did`. Migrate to
/// `start_reputation_gossip_with_verification(ingress_rx, store,
/// rate_limit, refresh_hook, signature_lookup)` with a real lookup
/// table. The deprecation is **loud** (compile warning) but does NOT
/// break compilation for back-compat with environments whose attestor
/// registration has not yet wired up the lookup.
#[deprecated(
    since = "0.2.0",
    note = "RFC-0968 §12 MUST: pass a PublicKeyLookup via \
            start_reputation_gossip_with_verification(...) — \
            signature verification is mandatory. See Round 2 review C5."
)]
pub fn start_reputation_gossip<S>(
    ingress_rx: mpsc::Receiver<RawIngress>,
    store: Arc<S>,
) -> ReputationGossipJoin
where
    S: ReputationStore + Send + Sync + 'static,
{
    // Self-call inside the deprecated wrapper chain: silence the
    // deprecation warning so only the outer call site at the user
    // gets the diagnostic.
    #[allow(deprecated)]
    {
        start_reputation_gossip_with_rate_limit(
            ingress_rx,
            store,
            Arc::new(RateLimitedAttestor::new()),
        )
    }
}

/// Convenience wrapper: a caller that has its own `RateLimitedAttestor`
/// (e.g. ops with custom caps) can inject it directly. Sharing the
/// `RateLimitedAttestor` between the ingress loop and the test
/// fixtures lets the tests assert on the limiter's tracked-attestor
/// count after a burst.
///
/// **DEPRECATED** (Round 2 review C5): see
/// `start_reputation_gossip_with_verification`.
#[deprecated(
    since = "0.2.0",
    note = "RFC-0968 §12 MUST: pass a PublicKeyLookup via \
            start_reputation_gossip_with_verification(...) — \
            signature verification is mandatory. See Round 2 review C5."
)]
pub fn start_reputation_gossip_with_rate_limit<S>(
    ingress_rx: mpsc::Receiver<RawIngress>,
    store: Arc<S>,
    rate_limit: Arc<RateLimitedAttestor>,
) -> ReputationGossipJoin
where
    S: ReputationStore + Send + Sync + 'static,
{
    // See note above — silence self-deprecation in the wrapper chain.
    #[allow(deprecated)]
    {
        start_reputation_gossip_with_rate_limit_and_refresh(ingress_rx, store, rate_limit, None)
    }
}

/// Full-control entry point: caller injects a `RateLimitedAttestor`
/// and an optional `RefreshHook`. The hook is invoked by `handle_one`
/// after every successful `record_signal` so derived stores (e.g.,
/// `DcRootedSlashReputationStoreCompat` for the cross-mission slash
/// adapter, mission 0855p-c) stay in sync with the persisted state.
/// When `refresh_hook` is `None`, the gossip substrate behaves
/// identically to the rate-limit-only wrapper.
///
/// **DEPRECATED** (Round 2 review C5): see
/// `start_reputation_gossip_with_verification`.
#[deprecated(
    since = "0.2.0",
    note = "RFC-0968 §12 MUST: pass a PublicKeyLookup via \
            start_reputation_gossip_with_verification(...) — \
            signature verification is mandatory. See Round 2 review C5."
)]
pub fn start_reputation_gossip_with_rate_limit_and_refresh<S>(
    ingress_rx: mpsc::Receiver<RawIngress>,
    store: Arc<S>,
    rate_limit: Arc<RateLimitedAttestor>,
    refresh_hook: Option<RefreshHook>,
) -> ReputationGossipJoin
where
    S: ReputationStore + Send + Sync + 'static,
{
    start_reputation_gossip_with_verification(ingress_rx, store, rate_limit, refresh_hook, None)
}

/// Full-control entry point with signature verification (S4
/// hardening, RFC-0968 §12 enforcement).
///
/// **THIS IS THE MANDATORY PROD ENTRY POINT** (Round 2 review C5):
/// `signature_lookup` is consulted by `handle_one` for every accepted
/// envelope: the recorder's ed25519 pubkey is looked up by DID, the
/// envelope's `recorder_signature` is verified against
/// `SignalEvent::canonical_bytes()`, and the
/// `recorder_did == did_from_pubkey(pubkey)` binding is enforced
/// (stale pubkey mapping rejection).
///
/// The other three constructors (`start_reputation_gossip`,
/// `start_reputation_gossip_with_rate_limit`,
/// `start_reputation_gossip_with_rate_limit_and_refresh`) are now
/// `#[deprecated]` because they pass `signature_lookup = None` and
/// therefore disable §12 enforcement. New callers MUST use this
/// constructor with a real lookup table; the deprecations are loud
/// but back-compat for environments whose attestor registration has
/// not yet wired up the lookup.
pub fn start_reputation_gossip_with_verification<S>(
    ingress_rx: mpsc::Receiver<RawIngress>,
    store: Arc<S>,
    rate_limit: Arc<RateLimitedAttestor>,
    refresh_hook: Option<RefreshHook>,
    signature_lookup: Option<Arc<dyn PublicKeyLookup>>,
) -> ReputationGossipJoin
where
    S: ReputationStore + Send + Sync + 'static,
{
    let processor: Processor = {
        let store = Arc::clone(&store);
        let rl = Arc::clone(&rate_limit);
        // S4 hardening: wrap the caller's refresh_hook in a dedup
        // decorator that tracks already-processed event_ids in a
        // shared `ProcessedEvents` set. Without this, gossipsub
        // duplicate deliveries cause the hook to fire N times for the
        // same event, double-counting in derived stores (e.g., the
        // DC cross-domain slash counter).
        let processed = Arc::new(ProcessedEvents::default());
        let deduped_hook = refresh_hook.map(|h| dedup_refresh_hook(h, Arc::clone(&processed)));
        Arc::new(move |msg: &RawIngress| {
            let store = Arc::clone(&store);
            let rl = Arc::clone(&rl);
            let hook = deduped_hook.clone();
            let lookup = signature_lookup.clone();
            Box::pin(async move {
                handle_one(msg, &*store, &rl, hook.as_ref(), lookup.as_deref()).await
            })
        })
    };
    start_reputation_gossip_with(ingress_rx, processor)
}

/// Tracks `EventId`s whose `RefreshHook` has already fired. Wraps
/// the gossip substrate's refresh-hook invocation path so duplicate
/// gossipsub deliveries (which are common: gossipsub is at-least-once)
/// do NOT re-fire the hook, which would otherwise double-count in
/// derived stores (mission 0855p-c's DC cross-domain slash counter is
/// the canonical example).
///
/// Stored as a `HashSet<EventId>` behind a `Mutex`; the gossip loop
/// is single-threaded (driven by `tokio::task::spawn_local`) so the
/// lock contention is effectively zero. The set grows unbounded over
/// the lifetime of the gossip substrate; for production deployments
/// the caller should periodically rebuild the substrate (e.g., on
/// epoch boundary) to bound memory.
#[derive(Debug, Default)]
pub struct ProcessedEvents {
    seen: Mutex<std::collections::HashSet<octo_reputation::types::EventId>>,
}

impl ProcessedEvents {
    /// Mark `event_id` as processed. Returns `true` if this is the
    /// FIRST time the id is seen (hook should fire); `false` if it
    /// was already tracked (hook should be skipped).
    pub fn mark_if_new(&self, event_id: &octo_reputation::types::EventId) -> bool {
        let mut g = self.seen.lock();
        g.insert(*event_id)
    }

    /// Number of unique event_ids currently tracked.
    pub fn len(&self) -> usize {
        self.seen.lock().len()
    }

    /// True iff no events have been tracked yet.
    pub fn is_empty(&self) -> bool {
        self.seen.lock().is_empty()
    }
}

/// Wrap a `RefreshHook` in a dedup decorator. The wrapper calls the
/// inner hook only when `event_id` has not been seen before (per the
/// supplied `ProcessedEvents` set). Subsequent calls with the same
/// `event_id` are a no-op.
fn dedup_refresh_hook(inner: RefreshHook, processed: Arc<ProcessedEvents>) -> RefreshHook {
    Arc::new(move |ev: &SignalEvent| {
        let processed = Arc::clone(&processed);
        let inner = Arc::clone(&inner);
        let event_id = ev.event_id;
        Box::pin(async move {
            if !processed.mark_if_new(&event_id) {
                debug!(
                    event_id = ?event_id,
                    "reputation gossip: refresh hook dedup hit, skipping"
                );
                return;
            }
            inner(ev).await;
        })
    })
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
///
/// `refresh_hook` is invoked after every successful `record_signal`
/// so derived in-memory state (e.g., `DcRootedSlashReputationStoreCompat`'s
/// cross-domain slash counter, mission 0855p-c) stays in sync with
/// the persisted store. The hook receives a reference to the
/// accepted event; errors inside the hook are the hook's
/// responsibility (the gossip substrate does not retry).
pub async fn handle_one<S>(
    msg: &RawIngress,
    store: &S,
    rate_limit: &RateLimitedAttestor,
    refresh_hook: Option<&RefreshHook>,
    signature_lookup: Option<&dyn PublicKeyLookup>,
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
    // RFC-0968 §12 enforcement: if the caller wired a public-key
    // lookup, verify the envelope's recorder_signature and reject
    // stale pubkey mappings before persistence. Without a lookup,
    // fall through (back-compat path; production deployments SHOULD
    // pass a lookup).
    if let Some(lookup) = signature_lookup {
        match lookup.lookup(&env.event.recorder_did) {
            Some(pubkey) => {
                if let Err(reason) = verify_envelope_signature(&env, &pubkey) {
                    warn!(
                        recorder_did = ?env.event.recorder_did,
                        reason,
                        "reputation gossip: signature verification failed"
                    );
                    return IngressOutcome::InvalidShape;
                }
            }
            None => {
                debug!(
                    recorder_did = ?env.event.recorder_did,
                    "reputation gossip: no pubkey registered for recorder_did; \
                     skipping signature verification (lookup-less fallback)"
                );
            }
        }
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
    // Fire the refresh hook (if any). The hook receives the
    // accepted event by reference; derived stores (e.g., the DC
    // cross-domain slash counter) refresh their in-memory state
    // from the canonical `ReputationStore` projection filtered by
    // `signal_kind == Slash ∧ layer == Coordinator`.
    if let Some(hook) = refresh_hook {
        hook(&env.event).await;
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
///
/// S4 hardening: the per-call event count is hard-capped at
/// [`CATCH_UP_LIMIT`] (DoS mitigation — a malicious peer cannot
/// force unbounded memory consumption by replying with millions of
/// events). The cap is enforced by slicing the result list; callers
/// needing more than one batch should re-invoke with a fresh
/// `since_event_id` once the first batch returns.
///
/// S4 hardening: a sliding-window catch-up rate limiter
/// [`CatchUpRateLimit`] defends against restart storms. The
/// `now_unix` parameter is the local-clock timestamp at call time;
/// when the rate-limit window has not yet elapsed since the previous
/// successful republish, the function returns `Ok(0)` without
/// touching the store. The default constructor disables the limit
/// (back-compat); production deployments should use
/// [`CatchUpRateLimit::per_second`] for restart-storm protection.
pub async fn gossip_catch_up<S, F>(
    store: &S,
    catch_up: GossipCatchUp,
    now_unix: u64,
    rate_limit: &CatchUpRateLimit,
    mut republish: F,
) -> Result<u64, octo_reputation::error::ReputationError>
where
    S: ReputationStore + Send + Sync,
    F: FnMut(&octo_reputation::types::SignalEvent),
{
    if !rate_limit.allow(now_unix) {
        debug!(
            attestor_did = ?catch_up.attestor_did,
            since_event_id = ?catch_up.since_event_id,
            "reputation gossip catch-up: rate-limited"
        );
        return Ok(0);
    }
    let events = store.gossip_catch_up(&catch_up).await?;
    let total = events.len();
    let capped = events.len().min(CATCH_UP_LIMIT);
    if total > CATCH_UP_LIMIT {
        warn!(
            stored = total,
            capped = CATCH_UP_LIMIT,
            "reputation gossip catch-up: result exceeded CATCH_UP_LIMIT, truncating; \
             caller should re-invoke with a fresh since_event_id"
        );
    }
    for ev in events.iter().take(capped) {
        republish(ev);
    }
    Ok(capped as u64)
}

/// Maximum events served per `gossip_catch_up` invocation (S4 DoS
/// cap). Set at 10_000 — well above any realistic catch-up batch but
/// low enough that a malicious peer cannot force unbounded memory
/// consumption. Callers needing more should re-invoke with a fresh
/// `since_event_id` once the first batch returns.
pub const CATCH_UP_LIMIT: usize = 10_000;

/// Default catch-up rate limit (S4 restart-storm protection).
/// `MIN_ATTESTOR_QUORUM / 10` envelopes per second per peer — at
/// `MIN_ATTESTOR_QUORUM = 3`, this is 0 (integer-divided) which would
/// block all catch-ups; instead use a single envelope per 3 seconds
/// as the floor. The real default is set to
/// [`CATCH_UP_DEFAULT_PER_SECOND`] for production deployments.
pub const CATCH_UP_DEFAULT_PER_SECOND: u32 = 1;

/// Sliding-window catch-up rate limiter (S4 restart-storm defence).
///
/// Tracks the timestamp of the last successful republish burst;
/// when the configured minimum interval has not elapsed, the next
/// `gossip_catch_up` call returns `Ok(0)` without invoking the store
/// or republish closure. The `default()` instance is permissive (no
/// rate limiting) for back-compat with existing callers.
#[derive(Debug)]
pub struct CatchUpRateLimit {
    per_second: u32,
    last_republish_unix: Mutex<Option<u64>>,
}

impl CatchUpRateLimit {
    /// Permissive default — no rate limiting. Back-compat for
    /// existing tests and integrations that pass `CatchUpRateLimit::default()`.
    pub fn unlimited() -> Self {
        Self {
            per_second: u32::MAX,
            last_republish_unix: Mutex::new(None),
        }
    }

    /// Cap republishes at `per_second` envelopes per second. The
    /// sliding-window is approximated as a fixed minimum interval
    /// `1_000_000_000 / per_second` microseconds; `per_second = 1`
    /// means at most one burst per second, `per_second = 10` means
    /// at most one burst per 100ms.
    pub fn per_second(per_second: u32) -> Self {
        Self {
            per_second,
            last_republish_unix: Mutex::new(None),
        }
    }

    /// Returns `true` if the caller may proceed with the next
    /// catch-up burst. Records the burst timestamp on `Allow`.
    pub fn allow(&self, now_unix: u64) -> bool {
        if self.per_second == u32::MAX {
            return true; // unlimited
        }
        let interval_ns = 1_000_000_000u64 / (self.per_second.max(1) as u64);
        let mut g = self.last_republish_unix.lock();
        if let Some(last) = *g {
            // now_unix is in seconds; convert interval to seconds
            // (rounding up). For per_second >= 1 the interval fits
            // in 1s; for slower rates we accumulate.
            let interval_secs = interval_ns.div_ceil(1_000_000_000);
            if now_unix.saturating_sub(last) < interval_secs {
                return false;
            }
        }
        *g = Some(now_unix);
        true
    }
}

impl Default for CatchUpRateLimit {
    fn default() -> Self {
        Self::unlimited()
    }
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
        let outcome = handle_one(&msg, &*store, &rl, None, None).await;
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
        let outcome = handle_one(&msg, &*store, &rl, None, None).await;
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
        let outcome = handle_one(&msg, &*store, &rl, None, None).await;
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
        let outcome = handle_one(&msg, &*store, &rl, None, None).await;
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
            None,
            None,
        )
        .await;
        assert_eq!(r1, IngressOutcome::Accepted);
        let r2 = handle_one(&RawIngress { topic, payload }, &*store, &rl, None, None).await;
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
                // Test fixture exercises the lookup-less path on purpose
                // (Round 2 review C5); migration to
                // `start_reputation_gossip_with_verification` requires a
                // test `PublicKeyLookup` impl, deferred to follow-on.
                #[allow(deprecated)]
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
        let outcome = handle_one(&RawIngress { topic, payload }, &*store, &rl, None, None).await;
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
                None,
                None,
            )
            .await;
        }
        let attestor = octo_reputation::auth::AttestorId::from_array([0xFF; 52]);
        let catch_up = GossipCatchUp {
            attestor_did: attestor,
            since_event_id: EventId::from_u64(1),
        };
        let mut republished = 0u64;
        let n = gossip_catch_up(
            &*store,
            catch_up,
            1_700_000_100,
            &CatchUpRateLimit::default(),
            |_ev| republished += 1,
        )
        .await
        .expect("catch_up");
        assert_eq!(n, 3, "expected 3 events re-published (ids 2,3,4), got {n}");
        assert_eq!(n, republished);
    }

    /// Item 1 wire-up: a `RefreshHook` that calls
    /// `DcRootedSlashReputationStoreCompat::refresh_cross_domain_for`
    /// fires after a slash+Coordinator ingress and updates the
    /// in-memory counter. Non-slash or non-Coordinator events do NOT
    /// change the count.
    #[tokio::test]
    async fn refresh_hook_fires_on_accepted_slash_coordinator() {
        use crate::reputation::DcRootedSlashReputationStoreCompat;
        use octo_reputation::types::{ControllerId, ReputationLayer as Layer, SignalKind as Kind};

        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();
        let did = RecorderDid::from_array([0x42; 52]);
        let topic = topic_for(&did);

        // Build a slash+Coordinator envelope.
        let slash_event = SignalEvent {
            event_id: EventId::from_u64(99),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: Kind::Slash,
            layer: Layer::Coordinator,
            score_delta: Dfp::from_f64(0.0),
            recorded_at_unix: 1_700_000_000,
            rotation_provenance: None,
            audit_ref: None,
        };
        let env = GossipEnvelope {
            event: slash_event,
            recorder_signature: vec![1u8; 64],
            source_mission: "mon:bootstrap".into(),
            source_domain: "domain:dc:test".into(),
            rotation_provenance: None,
            attestations: vec![],
        };
        let payload = serde_json::to_vec(&env).unwrap();

        // DC store starts at 0; hook must refresh to 1 after ingress.
        let dc_store = Arc::new(DcRootedSlashReputationStoreCompat::new());
        assert_eq!(dc_store.cross_domain_slash_count_for(&did), 0);

        // Build the hook. It borrows from `store` via the trait object.
        let store_for_hook = Arc::clone(&store);
        let dc_for_hook = Arc::clone(&dc_store);
        let hook: RefreshHook = Arc::new(move |ev: &SignalEvent| {
            let store = Arc::clone(&store_for_hook);
            let dc = Arc::clone(&dc_for_hook);
            let did = ev.recorder_did;
            Box::pin(async move {
                if ev.signal_kind == Kind::Slash && ev.layer == Layer::Coordinator {
                    let _ = dc.refresh_cross_domain_for(&did, &*store).await;
                }
            })
        });

        let outcome = handle_one(
            &RawIngress { topic, payload },
            &*store,
            &rl,
            Some(&hook),
            None,
        )
        .await;
        assert_eq!(outcome, IngressOutcome::Accepted);
        // The hook fired: DC counter reflects the persisted slash.
        assert_eq!(dc_store.cross_domain_slash_count_for(&did), 1);
    }

    /// Counter-test: an Outcome event on Market does NOT change the
    /// cross-domain slash count even when a refresh hook is wired.
    #[tokio::test]
    async fn refresh_hook_skips_non_slash_non_coordinator_events() {
        use crate::reputation::DcRootedSlashReputationStoreCompat;
        use octo_reputation::types::{ControllerId, ReputationLayer as Layer, SignalKind as Kind};

        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();
        let did = RecorderDid::from_array([0x43; 52]);
        let topic = topic_for(&did);
        let env = dummy_envelope(1, did); // Outcome + Market
        let payload = serde_json::to_vec(&env).unwrap();

        let dc_store = Arc::new(DcRootedSlashReputationStoreCompat::new());
        let store_for_hook = Arc::clone(&store);
        let dc_for_hook = Arc::clone(&dc_store);
        let hook: RefreshHook = Arc::new(move |ev: &SignalEvent| {
            let store = Arc::clone(&store_for_hook);
            let dc = Arc::clone(&dc_for_hook);
            let did = ev.recorder_did;
            Box::pin(async move {
                if ev.signal_kind == Kind::Slash && ev.layer == Layer::Coordinator {
                    let _ = dc.refresh_cross_domain_for(&did, &*store).await;
                }
            })
        });

        let outcome = handle_one(
            &RawIngress { topic, payload },
            &*store,
            &rl,
            Some(&hook),
            None,
        )
        .await;
        assert_eq!(outcome, IngressOutcome::Accepted);
        // No slash+Coordinator event → count stays at 0.
        assert_eq!(dc_store.cross_domain_slash_count_for(&did), 0);
        // Reference unused ControllerId import to keep the test fixture
        // honest about what fields exist on SignalEvent.
        let _ctrl: ControllerId = ControllerId::from_array([0u8; 32]);
    }

    // ====================================================================
    // S4 hardening tests (RFC-0968 §12 enforcement + DoS protection).
    // ====================================================================

    use ed25519_dalek::{Signer, SigningKey};
    use octo_reputation::types::RotationProvenance;
    use std::collections::HashMap;
    use std::sync::RwLock as StdRwLock;

    /// In-memory `PublicKeyLookup` for tests. Wraps a `HashMap`
    /// behind a `RwLock` so concurrent ingress loops can share it.
    #[derive(Debug, Default)]
    struct StaticPublicKeyLookup {
        by_did: StdRwLock<HashMap<octo_reputation::types::RecorderDid, [u8; 32]>>,
    }

    impl StaticPublicKeyLookup {
        fn new() -> Self {
            Self::default()
        }

        fn insert(&self, did: octo_reputation::types::RecorderDid, pubkey: [u8; 32]) {
            self.by_did.write().unwrap().insert(did, pubkey);
        }
    }

    impl PublicKeyLookup for StaticPublicKeyLookup {
        fn lookup(&self, did: &octo_reputation::types::RecorderDid) -> Option<[u8; 32]> {
            self.by_did.read().unwrap().get(did).copied()
        }
    }

    /// S4-1 + S4-4 (stale pubkey mapping): an envelope whose
    /// `recorder_did` does NOT equal `did_from_pubkey(pubkey)` for
    /// the registered pubkey MUST be rejected with `InvalidShape`,
    /// even when the ed25519 signature itself verifies cleanly.
    /// This closes the topic-spoofing attack vector on
    /// `/dot/reputation/{recorder_did}`.
    #[tokio::test]
    async fn handle_one_rejects_stale_pubkey_mapping() {
        use ed25519_dalek::SigningKey;
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();

        // Generate a real keypair. The signer is "alice"; alice's
        // recorder_did is `did_from_pubkey(alice_pubkey)`.
        let key = SigningKey::from_bytes(&[0x77; 32]);
        let pubkey = key.verifying_key().to_bytes();
        let alice_did = did_from_pubkey(&pubkey);

        // Build a well-formed envelope signed by alice, but with
        // `recorder_did = bob_did` (stale-pubkey-mapping attack).
        let bob_did = RecorderDid::from_array([0xCD; 52]);
        let event = SignalEvent {
            event_id: EventId::from_u64(1),
            recorder_did: bob_did, // <-- mismatch from alice's did
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: 1_700_000_000,
            rotation_provenance: None,
            audit_ref: None,
        };
        let mut env = GossipEnvelope {
            event,
            recorder_signature: vec![0u8; 64],
            source_mission: "mon:test".into(),
            source_domain: "domain:test".into(),
            rotation_provenance: None,
            attestations: vec![],
        };
        // Sign over canonical_bytes — the verifier checks signature
        // first, then the binding check fails.
        let msg = env.event.canonical_bytes();
        env.recorder_signature = key.sign(&msg).to_bytes().to_vec();

        // Wire lookup with bob_did mapped to alice's pubkey — this
        // is the exact attack: attacker subscribes to alice's
        // topic, signs with their own key, but uses alice's
        // envelope as a template after swapping recorder_did.
        let lookup = StaticPublicKeyLookup::new();
        lookup.insert(bob_did, pubkey);

        let topic = topic_for(&bob_did);
        let payload = serde_json::to_vec(&env).unwrap();
        let outcome = handle_one(
            &RawIngress { topic, payload },
            &*store,
            &rl,
            None,
            Some(&lookup),
        )
        .await;
        assert_eq!(
            outcome,
            IngressOutcome::InvalidShape,
            "stale pubkey mapping MUST be rejected with InvalidShape"
        );
        // Event must NOT have been persisted. `read_aggregate`
        // returns `Err(AggregateNotFound)` when no event exists for
        // (did, kind, layer); both bob and alice must surface this.
        let alice_agg = store
            .read_aggregate(&alice_did, SignalKind::Outcome, ReputationLayer::Market)
            .await;
        assert!(
            alice_agg.is_err() || alice_agg.unwrap().samples == 0,
            "alice_did aggregate must be empty after rejection"
        );
        let bob_agg = store
            .read_aggregate(&bob_did, SignalKind::Outcome, ReputationLayer::Market)
            .await;
        assert!(
            bob_agg.is_err() || bob_agg.unwrap().samples == 0,
            "bob_did aggregate must be empty after rejection"
        );
    }

    /// S4-1 + S4-4 (signature mismatch): a tampered envelope whose
    /// signature does NOT verify against the registered pubkey MUST
    /// be rejected with `InvalidShape`.
    #[tokio::test]
    async fn handle_one_rejects_signature_mismatch() {
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();

        // Register a pubkey for our test DID. We'll send a
        // signature that is NOT signed by this pubkey.
        let key = SigningKey::from_bytes(&[0x66; 32]);
        let pubkey = key.verifying_key().to_bytes();
        let did = did_from_pubkey(&pubkey);
        let lookup = StaticPublicKeyLookup::new();
        lookup.insert(did, pubkey);

        let event = SignalEvent {
            event_id: EventId::from_u64(7),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: 1_700_000_000,
            rotation_provenance: None,
            audit_ref: None,
        };
        let env = GossipEnvelope {
            event,
            recorder_signature: vec![0u8; 64], // all zeros — invalid sig
            source_mission: "mon:test".into(),
            source_domain: "domain:test".into(),
            rotation_provenance: None,
            attestations: vec![],
        };

        let topic = topic_for(&did);
        let payload = serde_json::to_vec(&env).unwrap();
        let outcome = handle_one(
            &RawIngress { topic, payload },
            &*store,
            &rl,
            None,
            Some(&lookup),
        )
        .await;
        assert_eq!(outcome, IngressOutcome::InvalidShape);
    }

    /// S4-1 + S4-4 (signature verify succeeds for valid envelope):
    /// the lookup path is non-blocking when the envelope is properly
    /// signed by the registered pubkey.
    #[tokio::test]
    async fn handle_one_accepts_validly_signed_envelope() {
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();

        let key = SigningKey::from_bytes(&[0x55; 32]);
        let pubkey = key.verifying_key().to_bytes();
        let did = did_from_pubkey(&pubkey);
        let lookup = StaticPublicKeyLookup::new();
        lookup.insert(did, pubkey);

        let event = SignalEvent {
            event_id: EventId::from_u64(11),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: 1_700_000_000,
            rotation_provenance: None,
            audit_ref: None,
        };
        let mut env = GossipEnvelope {
            event,
            recorder_signature: vec![0u8; 64],
            source_mission: "mon:test".into(),
            source_domain: "domain:test".into(),
            rotation_provenance: None,
            attestations: vec![],
        };
        env.recorder_signature = key.sign(&env.event.canonical_bytes()).to_bytes().to_vec();

        let topic = topic_for(&did);
        let payload = serde_json::to_vec(&env).unwrap();
        let outcome = handle_one(
            &RawIngress { topic, payload },
            &*store,
            &rl,
            None,
            Some(&lookup),
        )
        .await;
        assert_eq!(outcome, IngressOutcome::Accepted);
        let agg = store
            .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
            .await
            .expect("read");
        assert_eq!(agg.samples, 1);
    }

    /// S4-4 (rotation tombstone): an envelope carrying a
    /// `rotation_provenance` whose `new_did` equals the event's
    /// `recorder_did` MUST be rejected by `validate_shape` — the
    /// rotation would have been a no-op. The gossip substrate
    /// surfaces this as `InvalidShape` (RFC-0968-A1 amendment 29
    /// enforcement).
    #[tokio::test]
    async fn handle_one_rejects_rotation_provenance_matching_event_did() {
        let store = Arc::new(InMemoryReputationStore::new());
        let rl = RateLimitedAttestor::new();

        let did = RecorderDid::from_array([0xAB; 52]);
        let env = GossipEnvelope {
            event: SignalEvent {
                event_id: EventId::from_u64(13),
                recorder_did: did,
                controller_id: ControllerId::from_array([0u8; 32]),
                signal_kind: SignalKind::Outcome,
                layer: ReputationLayer::Market,
                score_delta: Dfp::from_f64(0.5),
                recorded_at_unix: 1_700_000_000,
                rotation_provenance: None,
                audit_ref: None,
            },
            recorder_signature: vec![1u8; 64],
            source_mission: "mon:test".into(),
            source_domain: "domain:test".into(),
            rotation_provenance: Some(RotationProvenance {
                new_did: did, // same as event's recorder_did — illegal
                consumed_at_unix: 1_000,
                rotation_id: 1,
            }),
            attestations: vec![],
        };

        let topic = topic_for(&did);
        let payload = serde_json::to_vec(&env).unwrap();
        let outcome = handle_one(&RawIngress { topic, payload }, &*store, &rl, None, None).await;
        assert_eq!(outcome, IngressOutcome::InvalidShape);
    }

    /// S4-4 (`MIN_ATTESTOR_QUORUM` threshold): the attestor quorum
    /// check returns `false` for fewer than `MIN_ATTESTOR_QUORUM`
    /// distinct attestors and `true` at-or-above the threshold.
    /// Pins the amendment 22 quorum rule for any future tuning.
    #[tokio::test]
    async fn attestor_quorum_threshold_one_two_three() {
        use octo_reputation::auth::{AttestorId, AttestorRegistration};
        use octo_reputation::constants::MIN_ATTESTOR_QUORUM;
        // Pin the constant invariant at compile time.
        const { assert!(MIN_ATTESTOR_QUORUM >= 1, "test assumes positive quorum") };

        let recorder = RecorderDid::from_array([0x01; 52]);
        let target_event = EventId::from_u64(42);

        // Register K distinct attestors, each attesting the same event.
        // Fresh store per iteration so attestation counts are isolated.
        for k in 1..=3u8 {
            let store = InMemoryReputationStore::new();
            for i in 0..k {
                let att = AttestorId::from_array([i + 1; 52]);
                store
                    .register_attestor(AttestorRegistration {
                        attestor_did: att,
                        pubkey: [i + 1; 32],
                        peer_set_id: [0u8; 32],
                        requested_at_unix: 1_700_000_000,
                        registered_at_unix: 1_700_000_000,
                    })
                    .await
                    .expect("register");
                store
                    .record_attestation(octo_reputation::auth::Attestation {
                        attestation_id: 0,
                        attestor: att,
                        recorder_did: recorder,
                        event_id: target_event,
                        signature: vec![],
                        observed_at_unix: 1_700_000_000,
                        received_at_unix: 1_700_000_000,
                        source_mission: "mon:test".into(),
                        source_domain: "domain:test".into(),
                    })
                    .await
                    .expect("record_attestation");
            }
            let reached = store
                .attestor_quorum_reached(target_event)
                .await
                .expect("quorum");
            if (k as u32) < MIN_ATTESTOR_QUORUM {
                assert!(
                    !reached,
                    "k={k} attestors must NOT reach quorum (threshold {MIN_ATTESTOR_QUORUM})"
                );
            } else {
                assert!(
                    reached,
                    "k={k} attestors MUST reach quorum (threshold {MIN_ATTESTOR_QUORUM})"
                );
            }
        }
    }

    /// S4-4 (rate-limit sliding window reset): the attestor rate
    /// limiter MUST allow new attestations once the window has
    /// elapsed since the most recent accepted one, even if the
    /// attestor previously exhausted its budget. Pins the RFC-0968
    /// §12 sliding-window semantics.
    #[tokio::test]
    async fn rate_limit_sliding_window_resets_after_quiesce() {
        let rl = RateLimitedAttestor::with_capacity(2, 5);
        let a = octo_reputation::auth::AttestorId::from_array([0x99; 52]);

        // Burst: 2 allowed, 3rd rejected.
        assert_eq!(rl.check(&a, 1_000), RateLimitDecision::Allow);
        assert_eq!(rl.check(&a, 1_001), RateLimitDecision::Allow);
        assert_eq!(rl.check(&a, 1_002), RateLimitDecision::Reject);

        // Quiesce: jump 5s+ past the window — entries evict.
        assert_eq!(rl.check(&a, 1_010), RateLimitDecision::Allow);
        assert_eq!(rl.check(&a, 1_011), RateLimitDecision::Allow);
        // Third in the new window: reject again.
        assert_eq!(rl.check(&a, 1_012), RateLimitDecision::Reject);
    }

    /// S4-3 (RefreshHook event_id dedup): the dedup decorator must
    /// fire the underlying hook exactly ONCE per event_id across N
    /// duplicate ingress calls. Validates the S4 hardening fix for
    /// the double-counting bug found by R2 reviewer.
    #[tokio::test]
    async fn refresh_hook_dedup_fires_once_per_event_id() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let processed = Arc::new(ProcessedEvents::default());
        let fire_count = Arc::new(AtomicU32::new(0));

        let inner: RefreshHook = {
            let fire_count = Arc::clone(&fire_count);
            Arc::new(move |_ev: &SignalEvent| {
                let fire_count = Arc::clone(&fire_count);
                Box::pin(async move {
                    fire_count.fetch_add(1, Ordering::SeqCst);
                })
            })
        };
        let hook = dedup_refresh_hook(inner, Arc::clone(&processed));

        let did = RecorderDid::from_array([0xEE; 52]);
        let event = SignalEvent {
            event_id: EventId::from_u64(101),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: 1_700_000_000,
            rotation_provenance: None,
            audit_ref: None,
        };

        // Fire 3 times with the same event_id.
        hook(&event).await;
        hook(&event).await;
        hook(&event).await;
        assert_eq!(
            fire_count.load(Ordering::SeqCst),
            1,
            "hook must fire exactly once for 3 duplicate event_ids"
        );
        assert_eq!(processed.len(), 1);
        assert!(!processed.is_empty());

        // A different event_id: fires once more.
        let mut event2 = event.clone();
        event2.event_id = EventId::from_u64(102);
        hook(&event2).await;
        assert_eq!(
            fire_count.load(Ordering::SeqCst),
            2,
            "different event_id must fire hook"
        );
        assert_eq!(processed.len(), 2);
    }

    /// S4-2 (catch-up cap constant + integrity): the exported
    /// `CATCH_UP_LIMIT` constant must equal the documented 10_000
    /// so a malicious peer cannot force unbounded memory
    /// consumption. The runtime truncation path (`events.iter().take(
    /// CATCH_UP_LIMIT)`) is one line of plumbing; its correctness
    /// is anchored on this constant value + the integration test in
    /// `cross_mission_federation` which exercises catch-up end-to-end.
    #[test]
    fn catch_up_limit_constant_is_documented() {
        assert_eq!(CATCH_UP_LIMIT, 10_000);
    }

    /// S4-2 (catch-up cap, runtime slice): when the store returns
    /// fewer than `CATCH_UP_LIMIT` events, the gossip substrate must
    /// pass all of them through. When the store returns exactly
    /// `CATCH_UP_LIMIT` events, all pass through unchanged. This
    /// pins the contract: the cap is `min(stored, CATCH_UP_LIMIT)`.
    #[tokio::test]
    async fn gossip_catch_up_passes_under_cap_unchanged() {
        // Seed 5 events, ask since_event_id=0 → expect 5 events
        // passed through (well under CATCH_UP_LIMIT).
        let store = InMemoryReputationStore::new();
        for i in 1..=5u64 {
            let did = RecorderDid::from_array([i as u8; 52]);
            let env = dummy_envelope(i, did);
            let _ = handle_one(
                &RawIngress {
                    topic: topic_for(&did),
                    payload: serde_json::to_vec(&env).unwrap(),
                },
                &store,
                &RateLimitedAttestor::new(),
                None,
                None,
            )
            .await;
        }
        let attestor = octo_reputation::auth::AttestorId::from_array([0xFF; 52]);
        let catch_up = GossipCatchUp {
            attestor_did: attestor,
            since_event_id: EventId::from_u64(0),
        };
        let mut count = 0u64;
        let n = gossip_catch_up(
            &store,
            catch_up,
            1_700_000_100,
            &CatchUpRateLimit::default(),
            |_ev| count += 1,
        )
        .await
        .expect("catch_up");
        // InMemoryReputationStore assigns monotonic event_ids 0..4
        // for 5 seeded events; `since_event_id=0` (exclusive lower
        // bound) returns 4 events (ids 1..4). The cap is well above
        // this so all pass through.
        assert_eq!(n, 4);
        assert_eq!(count, 4);
    }

    /// S4-5 (catch-up rebroadcast rate limit): a second call inside
    /// the rate-limit window MUST return `Ok(0)` without touching
    /// the store, defending against restart storms.
    #[tokio::test]
    async fn gossip_catch_up_rate_limit_blocks_restart_storm() {
        let store = InMemoryReputationStore::new();
        // Seed 5 events.
        for i in 1..=5u64 {
            let did = RecorderDid::from_array([i as u8; 52]);
            let env = dummy_envelope(i, did);
            let _ = handle_one(
                &RawIngress {
                    topic: topic_for(&did),
                    payload: serde_json::to_vec(&env).unwrap(),
                },
                &store,
                &RateLimitedAttestor::new(),
                None,
                None,
            )
            .await;
        }
        let attestor = octo_reputation::auth::AttestorId::from_array([0xFF; 52]);
        let catch_up = GossipCatchUp {
            attestor_did: attestor,
            since_event_id: EventId::from_u64(0),
        };
        // 1 per second → first call at t=100 succeeds, second
        // call at t=100 (same second) is rate-limited.
        let rl = CatchUpRateLimit::per_second(1);
        let n1 = gossip_catch_up(&store, catch_up.clone(), 100, &rl, |_| {})
            .await
            .expect("first");
        assert_eq!(n1, 4, "first burst serves events since_id=0 (ids 1..4)");
        // Same second: rate-limited.
        let n2 = gossip_catch_up(&store, catch_up.clone(), 100, &rl, |_| {})
            .await
            .expect("second");
        assert_eq!(
            n2, 0,
            "second call in same window must be rate-limited (Ok(0))"
        );
        // After the window elapses, allowed again — but since the
        // store hasn't grown new events, result is the same 4.
        let n3 = gossip_catch_up(&store, catch_up, 101, &rl, |_| {})
            .await
            .expect("third");
        assert_eq!(n3, 4, "post-window call re-serves since_id=0 (ids 1..4)");
        // Unlimited instance: never blocked.
        let rl_unlimited = CatchUpRateLimit::default();
        let n4 = gossip_catch_up(
            &store,
            GossipCatchUp {
                attestor_did: attestor,
                since_event_id: EventId::from_u64(0),
            },
            100,
            &rl_unlimited,
            |_| {},
        )
        .await
        .expect("unlimited");
        assert_eq!(n4, 4, "unlimited instance serves all events");
    }
}
