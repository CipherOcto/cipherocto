//! Sliding-window helper (RFC-0968 §3, mission 0968 Phase 3).
//!
//! Wraps `ReputationStore::sliding_window` with a bounded window. Caps the
//! requested window to `MAX_SLIDING_WINDOW_SECS` so a buggy client cannot
//! crash the producer by asking for a full-history window.

use crate::error::ReputationError;
use crate::store::{ReputationStore, StoreResult};
use crate::types::{RecorderDid, ReputationAggregate, ReputationLayer, SignalKind};

/// Maximum window a single query may request. 30 days.
pub const MAX_SLIDING_WINDOW_SECS: u64 = 30 * 86_400;

/// Effective window = `min(window_secs, MAX_SLIDING_WINDOW_SECS)`.
/// A requested `0` still rejects so callers can't accidentally pass an
/// uninitialised parameter.
pub fn effective_window(window_secs: u64) -> u64 {
    if window_secs == 0 {
        return 0;
    }
    window_secs.min(MAX_SLIDING_WINDOW_SECS)
}

/// Run a capped sliding-window query.
pub async fn sliding_window<S: ReputationStore + ?Sized>(
    store: &S,
    did: &RecorderDid,
    kind: SignalKind,
    layer: ReputationLayer,
    window_secs: u64,
    now_unix: u64,
) -> StoreResult<ReputationAggregate> {
    if window_secs == 0 {
        return Err(ReputationError::SlidingWindowZero);
    }
    let w = effective_window(window_secs);
    store.sliding_window(did, kind, layer, w, now_unix).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryReputationStore;
    use crate::types::{ControllerId, EventId, SignalEvent};
    use octo_determin::Dfp;

    #[test]
    fn effective_window_caps_to_max() {
        assert_eq!(effective_window(0), 0);
        assert_eq!(effective_window(86_400), 86_400);
        assert_eq!(
            effective_window(MAX_SLIDING_WINDOW_SECS),
            MAX_SLIDING_WINDOW_SECS
        );
        assert_eq!(
            effective_window(MAX_SLIDING_WINDOW_SECS + 1),
            MAX_SLIDING_WINDOW_SECS
        );
    }

    #[tokio::test]
    async fn sliding_window_zero_rejected() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([1u8; 52]);
        let err = sliding_window(
            &store,
            &did,
            SignalKind::Outcome,
            ReputationLayer::Market,
            0,
            1_000,
        )
        .await
        .unwrap_err();
        assert_eq!(err, ReputationError::SlidingWindowZero);
    }

    #[tokio::test]
    async fn sliding_window_30d_capped() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([1u8; 52]);
        // Drop a few events in the window.
        for i in 0..5u64 {
            let ev = SignalEvent {
                event_id: EventId::from_u64(i),
                recorder_did: did,
                controller_id: ControllerId::from_array([0u8; 32]),
                signal_kind: SignalKind::Outcome,
                layer: ReputationLayer::Market,
                score_delta: Dfp::from_f64(0.5),
                recorded_at_unix: 1_000 + i * 100,
                rotation_provenance: None,
                audit_ref: None,
            };
            store.record_signal(ev).await.unwrap();
        }
        // Request a window larger than MAX — must not crash, must be capped.
        let agg = sliding_window(
            &store,
            &did,
            SignalKind::Outcome,
            ReputationLayer::Market,
            MAX_SLIDING_WINDOW_SECS + 1,
            1_000 + 10_000,
        )
        .await
        .unwrap();
        // Capped to MAX, events at ts=1000..1500 are within (now=11000).
        assert!(agg.samples >= 1);
    }
}
