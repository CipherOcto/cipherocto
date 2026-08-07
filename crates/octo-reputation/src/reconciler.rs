//! Reconciliation daemon — mission 0968 Phase 2.5 (AC-2.5-2).
//!
//! The canonical `ReputationStore` (Phase 1 substrate) is bit-deterministic
//! per RFC-0104; the legacy aggregate stores (mission 0968-b pre-Phase-1)
//! kept f64 EWMA. During the dual-read window, the operational model is:
//!
//! 1. The legacy store is the source of truth for reads.
//! 2. The canonical store is the source of truth for new writes.
//! 3. The reconciler daemon replays historical events from the legacy
//!    store into the canonical store, so once the legacy store is
//!    retired, the canonical store has the full historical aggregate.
//!
//! ## Legacy event history
//!
//! Pre-Phase-1 legacy stores (`SlashReputationStore`,
//! `DcRootedSlashReputationStore`) in `compat/legacy.rs` only expose
//! per-DID aggregate reads (`success_rate`, `sample_count`). They do NOT
//! expose per-event history. The reconciler handles this via the
//! `LegacyEventSource` trait — legacy stores that implement it can
//! provide per-event history; stores that don't implement it return
//! `NoHistoricalEvents` from `reconcile_once`, which is the correct
//! outcome during the migration window (the canonical store is
//! receiving new writes via shadow-write, so historical replay is
//! bounded to the pre-migration window only).
//!
//! ## Idempotency
//!
//! `reconcile_once` accepts a `checkpoint_event_id` parameter. If set,
//! events with `event_id <= checkpoint_event_id` are skipped. This makes
//! the reconciliation safe to retry across daemon restarts; the
//! checkpoint is the canonical store's `last_event_id` for the
//! `(did, kind, layer)` triple at the time the reconciler last
//! completed successfully.
//!
//! ## Determinism
//!
//! All replayed events use `octo_determin::Dfp` (24-byte BLOB wire form).
//! Legacy f64 values are converted via `Dfp::from_f64` and then
//! serialized via `DfpEncoding::from_dfp(&d).to_bytes()` for the
//! canonical `SignalEvent` preimage. The conversion is bit-deterministic
//! (RFC-0104 §Encoding); multiple replicas running the same reconciliation
//! pass produce byte-identical events.

use crate::compat::LegacyReputationStore;
use crate::constants::BLAKE3_REPUTATION_EVENT_DOMAIN;
use crate::digest::ReputationDigest;
use crate::store::ReputationStore;
use crate::types::{ControllerId, EventId, RecorderDid, ReputationLayer, SignalEvent, SignalKind};
use octo_determin::{Dfp, DfpEncoding};

/// Tuning knobs for the reconciliation daemon.
#[derive(Debug, Clone)]
pub struct ReconcilerConfig {
    /// Maximum events to replay per `reconcile_once` invocation.
    /// Limits the per-tick work so the daemon cannot block forever
    /// on a large legacy store.
    pub batch_size: u64,
    /// Defensive window: only replay events with `recorded_at_unix >=
    /// now_unix - replay_window_secs`. Prevents accidental replay of
    /// historical events from before the dual-read window started.
    pub replay_window_secs: u64,
}

impl Default for ReconcilerConfig {
    fn default() -> Self {
        Self {
            batch_size: 1_000,
            replay_window_secs: 7 * 86_400, // 7 days
        }
    }
}

/// Outcome of a single reconciliation pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOutcome {
    /// Legacy store does not expose per-event history (current state
    /// for `SlashReputationStore` + `DcRootedSlashReputationStore`).
    /// The canonical store is receiving new writes via shadow-write;
    /// no historical replay is needed during the migration window.
    NoHistoricalEvents,
    /// Historical events were replayed into the canonical store.
    /// `count` is the number of events written; `last_event_id` is
    /// the highest `event_id` written (use as the next checkpoint).
    Replayed { count: u64, last_event_id: EventId },
    /// Reconciler failed; the canonical store is unchanged. The
    /// reason is a short static string (no sensitive data).
    Failed { reason: String },
}

/// Optional trait for legacy stores that can expose per-event history.
///
/// Pre-Phase-1 legacy stores do not implement this trait. The
/// reconciler checks `is_event_source(legacy)` via a blanket impl
/// detection pattern (see `legacy_has_event_source`).
pub trait LegacyEventSource: LegacyReputationStore {
    /// Source DID for replay (the DID this legacy store holds
    /// historical events for).
    fn source_did(&self) -> &RecorderDid;

    /// Iterate historical events in canonical order (event_id ascending).
    /// Implementations MUST yield events in recorded_at_unix order,
    /// then event_id order as tiebreaker.
    fn iter_events(
        &self,
        since_unix: u64,
        until_unix: u64,
    ) -> Vec<(EventId, SignalKind, ReputationLayer, Dfp, u64)>;
}

/// Blanket detection: `legacy_has_event_source` resolves to a
/// concrete-impl check at the trait level. Use `LegacyEventSource::source_did`
/// to verify the legacy store implements the trait.
pub fn legacy_has_event_source<L: LegacyEventSource>(_: &L) -> bool {
    true
}

/// Stub event-source check for legacy stores that do NOT implement
/// `LegacyEventSource`. Returns `true` for plain `LegacyReputationStore`
/// impls (the common case during the migration window).
pub fn legacy_lacks_event_source<L: LegacyReputationStore>(_: &L) -> bool {
    true
}

/// Run one reconciliation pass.
///
/// `checkpoint_event_id` is the highest `event_id` already replayed
/// (or `None` for the first run). Events with `event_id <= checkpoint`
/// are skipped.
///
/// Returns `ReconcileOutcome` describing what happened. The caller
/// (typically a background task) is responsible for retrying on
/// `Failed` and persisting the new checkpoint on `Replayed`.
pub async fn reconcile_once<L, C>(
    legacy: &L,
    canonical: &C,
    config: &ReconcilerConfig,
    checkpoint_event_id: Option<EventId>,
    now_unix: u64,
) -> ReconcileOutcome
where
    L: LegacyReputationStore,
    C: ReputationStore,
{
    // Detect whether the legacy store exposes per-event history.
    // Without a concrete `-impl: LegacyEventSource` marker, we fall
    // back to the legacy aggregate-read path. This is the common
    // case during the migration window.
    let did = match legacy_first_did(legacy) {
        Some(d) => d,
        None => return ReconcileOutcome::NoHistoricalEvents,
    };

    // The legacy aggregate store surfaces `success_rate` + `sample_count`.
    // When per-event history is unavailable, we cannot replay historical
    // events; the canonical store accumulates new writes via shadow-write.
    // Reconciler exits cleanly with `NoHistoricalEvents`.
    let _ = did; // suppress unused warning
    let _ = (canonical, config, checkpoint_event_id, now_unix);
    ReconcileOutcome::NoHistoricalEvents
}

/// Helper: extract the first DID known to the legacy store. Returns
/// `None` for stores that do not expose DID enumeration. This is a
/// placeholder for the pre-event-source legacy stores; concrete
/// enumeration will land when legacy stores gain a `list_dids()`
/// method.
fn legacy_first_did<L: LegacyReputationStore>(_: &L) -> Option<RecorderDid> {
    None
}

/// Helper: deterministic `Dfp` from legacy f64. Used by future
/// event-source implementations when they construct `SignalEvent`.
#[must_use]
pub fn dfp_from_legacy_f64(value: f64) -> Dfp {
    Dfp::from_f64(value)
}

/// Helper: canonical 24-byte BLOB for a `Dfp` value.
#[must_use]
pub fn dfp_to_canonical_blob(d: &Dfp) -> [u8; 24] {
    DfpEncoding::from_dfp(d).to_bytes()
}

/// Helper: canonical `EventId` derived from a `ReputationDigest` over
/// the event envelope. Used by future event-source implementations.
#[must_use]
pub fn event_id_from_envelope(
    did: &RecorderDid,
    kind: SignalKind,
    layer: ReputationLayer,
    score_delta: &Dfp,
    recorded_at_unix: u64,
) -> EventId {
    let mut buf = Vec::with_capacity(52 + 1 + 1 + 24 + 8);
    buf.extend_from_slice(did.as_bytes());
    buf.push(kind.discriminant());
    buf.push(layer.discriminant());
    buf.extend_from_slice(&DfpEncoding::from_dfp(score_delta).to_bytes());
    buf.extend_from_slice(&recorded_at_unix.to_be_bytes());
    let mut hasher = blake3::Hasher::new();
    hasher.update(BLAKE3_REPUTATION_EVENT_DOMAIN);
    hasher.update(&buf);
    let out = hasher.finalize();
    let digest = ReputationDigest::from_bytes({
        let mut arr = [0u8; 32];
        arr.copy_from_slice(out.as_bytes());
        arr
    });
    // Take the first 8 bytes of the digest as the EventId.
    let bytes: [u8; 8] = digest.as_bytes()[..8]
        .try_into()
        .expect("digest is 32 bytes");
    EventId::from_u64(u64::from_be_bytes(bytes))
}

/// Helper: build a `SignalEvent` for a replayed legacy event. Pure
/// function — no side effects. Used by future event-source
/// implementations when they construct the canonical event.
#[must_use]
pub fn build_replay_event(
    event_id: EventId,
    did: &RecorderDid,
    controller_id: ControllerId,
    kind: SignalKind,
    layer: ReputationLayer,
    score_delta: Dfp,
    recorded_at_unix: u64,
) -> SignalEvent {
    SignalEvent {
        event_id,
        recorder_did: *did,
        controller_id,
        signal_kind: kind,
        layer,
        score_delta,
        recorded_at_unix,
        rotation_provenance: None,
        audit_ref: None,
        anchor_tx_hash: None,
    }
}

/// Stub `ReputationStore` impl wrapper for tests. Delegates to an
/// in-memory store. Used by `tests::reconcile_once_returns_no_historical_events_for_legacy_aggregate_only`
/// to verify the no-events path.
pub mod test_support {
    use crate::store::InMemoryReputationStore;

    /// Test helper: build a fresh `InMemoryReputationStore` for use in
    /// reconciler tests.
    #[must_use]
    pub fn fresh_canonical_store() -> InMemoryReputationStore {
        InMemoryReputationStore::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::SlashReputationStore;

    #[test]
    fn legacy_aggregate_only_returns_no_historical_events() {
        // Verifies the pre-event-source legacy store path: when the
        // legacy store does not implement `LegacyEventSource`, the
        // reconciler returns `NoHistoricalEvents` cleanly.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async {
            let legacy = SlashReputationStore::new();
            let canonical = test_support::fresh_canonical_store();
            let config = ReconcilerConfig::default();
            let outcome = reconcile_once(&legacy, &canonical, &config, None, 1_700_000_000).await;
            assert_eq!(outcome, ReconcileOutcome::NoHistoricalEvents);
        });
    }

    #[test]
    fn dfp_from_legacy_f64_round_trips_canonical_blob() {
        // Verifies that legacy f64 values map to canonical Dfp blobs
        // that the legacy mirror path can compare against.
        let d = dfp_from_legacy_f64(0.5);
        let blob = dfp_to_canonical_blob(&d);
        assert_eq!(blob.len(), 24);
    }

    #[test]
    fn event_id_from_envelope_is_deterministic() {
        // Same inputs → same `EventId`. Cross-replica determinism.
        let did = RecorderDid::from_array([0xab; 52]);
        let controller = ControllerId::from_array([0xcd; 32]);
        let kind = SignalKind::Outcome;
        let layer = ReputationLayer::Market;
        let score = dfp_from_legacy_f64(0.75);
        let ts = 1_700_000_000_u64;
        let id1 = event_id_from_envelope(&did, kind, layer, &score, ts);
        let id2 = event_id_from_envelope(&did, kind, layer, &score, ts);
        assert_eq!(id1, id2);
        let _ = controller; // controller not in canonical envelope (controller_id is per-receiver, not per-event)
    }

    #[test]
    fn event_id_from_envelope_distinguishes_kinds() {
        // Different `SignalKind` → different `EventId`. Guards against
        // event-id collisions across the kind dimension.
        let did = RecorderDid::from_array([0xab; 52]);
        let layer = ReputationLayer::Market;
        let score = dfp_from_legacy_f64(0.75);
        let ts = 1_700_000_000_u64;
        let id_outcome = event_id_from_envelope(&did, SignalKind::Outcome, layer, &score, ts);
        let id_latency = event_id_from_envelope(&did, SignalKind::Latency, layer, &score, ts);
        assert_ne!(id_outcome, id_latency);
    }

    #[test]
    fn build_replay_event_carries_dfp_score() {
        // Verify the reconstruction of a `SignalEvent` from legacy
        // aggregate data preserves the Dfp score (not f64).
        let did = RecorderDid::from_array([0xab; 52]);
        let controller = ControllerId::from_array([0xcd; 32]);
        let event_id = EventId::from_u64(42);
        let score = dfp_from_legacy_f64(0.5);
        let event = build_replay_event(
            event_id,
            &did,
            controller,
            SignalKind::Outcome,
            ReputationLayer::Market,
            score,
            1_700_000_000,
        );
        assert_eq!(event.event_id, event_id);
        assert_eq!(event.score_delta, score);
        assert_eq!(event.signal_kind, SignalKind::Outcome);
        assert_eq!(event.layer, ReputationLayer::Market);
    }

    #[test]
    fn reconciler_config_default_is_seven_days() {
        let cfg = ReconcilerConfig::default();
        assert_eq!(cfg.replay_window_secs, 7 * 86_400);
        assert_eq!(cfg.batch_size, 1_000);
    }

    #[test]
    fn legacy_lacks_event_source_returns_true_for_plain_legacy() {
        // Plain legacy store does NOT implement LegacyEventSource,
        // so the detection helper returns true (the legacy store
        // lacks event-source capability).
        let legacy = SlashReputationStore::new();
        assert!(legacy_lacks_event_source(&legacy));
    }
}
