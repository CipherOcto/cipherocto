//! Audit replay wrapper (RFC-0968 §3, mission 0968 Phase 3).
//!
//! `replay_for_audit` returns events in `[since_unix, until_unix]` for a DID,
//! sorted by `recorded_at_unix`. The wrapper here composes two layers:
//!
//! 1. **Tombstone filter** — events whose `recorder_did` has been rotated and
//!    whose `rotation_provenance.rotation_id` is below the most recent
//!    consumed rotation are excluded from the audit set. This prevents
//!    re-use of tombstoned DIDs post-rotation (RFC-0968 amendment 47).
//! 2. **BLAKE3 commitment** — the canonical envelope over the returned
//!    events is computed under
//!    `BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN` so the auditor can pin a single
//!    digest in a nonce table.
//!
//! Both behaviours are optional — direct callers can use the underlying
//! `ReputationStore::replay_for_audit` and skip the tombstone filter.

use serde::{Deserialize, Serialize};

use crate::constants::BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN;
use crate::store::{ReputationStore, StoreResult};
use crate::types::{RecorderDid, SignalEvent};

/// Audit replay with optional tombstone filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditReplay {
    pub recorder_did: RecorderDid,
    pub since_unix: u64,
    pub until_unix: u64,
    /// Returned events (sorted by `recorded_at_unix`).
    pub events: Vec<SignalEvent>,
    /// `BLAKE3(BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN || canonical_bytes_per_event)`.
    pub commitment: [u8; 32],
}

/// The most recent `rotation_id` observed in `events`. `None` means no
/// rotations are present in the window.
pub fn max_rotation_id(events: &[SignalEvent]) -> Option<u64> {
    events
        .iter()
        .filter_map(|e| e.rotation_provenance.as_ref())
        .map(|rp| rp.rotation_id)
        .max()
}

/// Strip events whose `rotation_provenance.rotation_id` is below `threshold`.
/// Used to enforce tombstone semantics when the auditor replays events for
/// a rotated DID.
pub fn drop_pre_rotation_events(events: Vec<SignalEvent>, threshold: u64) -> Vec<SignalEvent> {
    events
        .into_iter()
        .filter(|e| match &e.rotation_provenance {
            Some(rp) => rp.rotation_id >= threshold,
            None => true,
        })
        .collect()
}

/// Compute the audit-set commitment.
pub fn audit_commitment(events: &[SignalEvent]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN);
    for e in events {
        hasher.update(&e.canonical_bytes());
    }
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(out.as_bytes());
    arr
}

/// One-shot helper — replay + tombstone-filter + commitment.
pub async fn replay<S: ReputationStore + ?Sized>(
    store: &S,
    did: &RecorderDid,
    since_unix: u64,
    until_unix: u64,
    rotation_threshold: Option<u64>,
) -> StoreResult<AuditReplay> {
    let mut events = store.replay_for_audit(did, since_unix, until_unix).await?;
    if let Some(t) = rotation_threshold {
        events = drop_pre_rotation_events(events, t);
    }
    if let Some(max_id) = max_rotation_id(&events) {
        events = drop_pre_rotation_events(events, max_id);
    }
    let commitment = audit_commitment(&events);
    Ok(AuditReplay {
        recorder_did: *did,
        since_unix,
        until_unix,
        events,
        commitment,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ReputationError;
    use crate::store::InMemoryReputationStore;
    use crate::types::{ControllerId, EventId, ReputationLayer, RotationProvenance, SignalKind};
    use octo_determin::Dfp;

    fn mk_ev(seed: u64, did: RecorderDid, ts: u64, rotation_id: Option<u64>) -> SignalEvent {
        let mut e = SignalEvent {
            event_id: EventId::from_u64(seed),
            recorder_did: did,
            controller_id: ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: ts,
            rotation_provenance: None,
            audit_ref: None,
            anchor_tx_hash: None,
        };
        if let Some(rid) = rotation_id {
            e.rotation_provenance = Some(RotationProvenance {
                new_did: did,
                consumed_at_unix: ts,
                rotation_id: rid,
            });
        }
        e
    }

    #[tokio::test]
    async fn replay_returns_sorted_events() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([9u8; 52]);
        for i in 0..5u64 {
            store
                .record_signal(mk_ev(i, did, 1_000 + i * 10, None))
                .await
                .unwrap();
        }
        let ar = replay(&store, &did, 0, 10_000, None).await.unwrap();
        assert_eq!(ar.events.len(), 5);
        let ts: Vec<u64> = ar.events.iter().map(|e| e.recorded_at_unix).collect();
        let mut sorted = ts.clone();
        sorted.sort();
        assert_eq!(ts, sorted);
        assert_ne!(ar.commitment, [0u8; 32]);
    }

    #[tokio::test]
    async fn replay_inverted_window_rejected() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([9u8; 52]);
        let err = replay(&store, &did, 2_000, 1_000, None).await.unwrap_err();
        assert_eq!(err, ReputationError::ReplayWindowInverted);
    }

    #[test]
    fn drop_pre_rotation_events_excludes_old_rotations() {
        let did = RecorderDid::from_array([9u8; 52]);
        let events = vec![
            mk_ev(1, did, 1_000, Some(5)),
            mk_ev(2, did, 2_000, Some(10)),
            mk_ev(3, did, 3_000, None),
        ];
        let kept = drop_pre_rotation_events(events.clone(), 10);
        // First event (rotation_id=5) dropped; second && third kept.
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].event_id.to_u64(), 2);
        assert_eq!(kept[1].event_id.to_u64(), 3);
    }

    #[test]
    fn max_rotation_id_returns_max_seen() {
        let did = RecorderDid::from_array([9u8; 52]);
        let events = vec![
            mk_ev(1, did, 1_000, Some(5)),
            mk_ev(2, did, 2_000, Some(10)),
            mk_ev(3, did, 3_000, Some(7)),
        ];
        assert_eq!(max_rotation_id(&events), Some(10));
    }

    #[test]
    fn audit_commitment_is_deterministic() {
        let did = RecorderDid::from_array([9u8; 52]);
        let events = vec![mk_ev(1, did, 1_000, None), mk_ev(2, did, 2_000, None)];
        assert_eq!(audit_commitment(&events), audit_commitment(&events));
        // Different event set → different commitment.
        let one = vec![mk_ev(1, did, 1_000, None)];
        assert_ne!(audit_commitment(&one), audit_commitment(&events));
    }
}
