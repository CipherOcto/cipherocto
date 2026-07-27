//! In-memory `ReputationStore` for unit + integration tests.
//!
//! Backed by `tokio::sync::RwLock<HashMap<…>>`. Operations are deterministic
//! and side-effect free beyond the in-process map.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use octo_determin::Dfp;
use tokio::sync::RwLock;

use crate::auth::{GovernanceProof, GovernanceSnapshot, SuspensionAuth};
use crate::constants::{BLAKE3_REPUTATION_RETIREMENT_DOMAIN, GOVERNANCE_QUORUM};
use crate::error::ReputationError;
use crate::recorder::verify_registration;
use crate::store::{ReputationStore, StoreResult};
use crate::types::{
    EventId, ParityEvidence, RecorderDid, RecorderId, ReputationAggregate, ReputationLayer,
    RetirementEligibility, SignalEvent, SignalKind,
};

/// Composite key for the events table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct EventKey {
    did_hash: [u8; 32],
    event_id: EventId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AggregateKey {
    did_hash: [u8; 32],
    kind: SignalKind,
    layer: ReputationLayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RecorderKey {
    did_hash: [u8; 32],
}

fn did_hash(d: &RecorderDid) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(d.as_bytes());
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(out.as_bytes());
    arr
}

#[derive(Default)]
struct Inner {
    events: HashMap<EventKey, SignalEvent>,
    aggregates: HashMap<AggregateKey, ReputationAggregate>,
    recorders: HashMap<RecorderKey, (RecorderId, bool, bool)>, // (id, suspended, slashed)
}

#[derive(Clone, Default)]
pub struct InMemoryReputationStore {
    inner: Arc<RwLock<Inner>>,
    next_event_id: Arc<AtomicU64>,
    next_recorder_id: Arc<AtomicU64>,
}

impl InMemoryReputationStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ReputationStore for InMemoryReputationStore {
    async fn record_signal(&self, mut event: SignalEvent) -> StoreResult<EventId> {
        let mut inner = self.inner.write().await;
        let id = self.next_event_id.fetch_add(1, Ordering::SeqCst);
        let eid = EventId::from_u64(id);
        event.event_id = eid;
        // Update aggregate
        let key = AggregateKey {
            did_hash: did_hash(&event.recorder_did),
            kind: event.signal_kind,
            layer: event.layer,
        };
        let agg = inner
            .aggregates
            .entry(key)
            .or_insert_with(|| ReputationAggregate {
                recorder_did: event.recorder_did,
                signal_kind: event.signal_kind,
                layer: event.layer,
                score_ewma: Dfp::zero(),
                samples: 0,
                severity_total: 0,
                last_event_id: eid,
                last_event_unix: event.recorded_at_unix,
                updated_at_unix: event.recorded_at_unix,
            });
        let n = agg.samples as f64;
        let alpha = 1.0 / (n + 1.0);
        // Deterministic f64 → Dfp arithmetic via `from_f64`.
        let delta_f = event.score_delta.to_f64();
        let cur_f = agg.score_ewma.to_f64();
        let new_f = cur_f * (1.0 - alpha) + delta_f * alpha;
        agg.score_ewma = Dfp::from_f64(new_f);
        agg.samples += 1;
        agg.last_event_id = eid;
        agg.last_event_unix = event.recorded_at_unix;
        agg.updated_at_unix = event.recorded_at_unix;

        inner.events.insert(
            EventKey {
                did_hash: key.did_hash,
                event_id: eid,
            },
            event,
        );
        Ok(eid)
    }

    async fn read_aggregate(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layer: ReputationLayer,
    ) -> StoreResult<ReputationAggregate> {
        let inner = self.inner.read().await;
        let key = AggregateKey {
            did_hash: did_hash(did),
            kind,
            layer,
        };
        inner
            .aggregates
            .get(&key)
            .cloned()
            .ok_or(ReputationError::AggregateNotFound {
                did: 0,
                kind: kind.discriminant(),
                layer: layer.discriminant(),
            })
    }

    async fn cross_layer_query(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layers: &[ReputationLayer],
    ) -> StoreResult<Vec<ReputationAggregate>> {
        if layers.is_empty() {
            return Err(ReputationError::CrossLayerEmpty);
        }
        let inner = self.inner.read().await;
        let h = did_hash(did);
        let mut out = Vec::with_capacity(layers.len());
        for &l in layers {
            let key = AggregateKey {
                did_hash: h,
                kind,
                layer: l,
            };
            if let Some(agg) = inner.aggregates.get(&key) {
                out.push(agg.clone());
            }
        }
        Ok(out)
    }

    async fn sliding_window(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layer: ReputationLayer,
        window_secs: u64,
        now_unix: u64,
    ) -> StoreResult<ReputationAggregate> {
        if window_secs == 0 {
            return Err(ReputationError::SlidingWindowZero);
        }
        let inner = self.inner.read().await;
        let h = did_hash(did);
        let cutoff = now_unix.saturating_sub(window_secs);
        let mut score = Dfp::zero();
        let mut samples: u64 = 0;
        let severity_total: u64 = 0;
        let mut last_event_id = EventId::from_u64(0);
        let mut last_event_unix: u64 = 0;
        let mut updated_at_unix: u64 = 0;
        for (k, ev) in &inner.events {
            if k.did_hash != h
                || ev.signal_kind != kind
                || ev.layer != layer
                || ev.recorded_at_unix < cutoff
                || ev.recorded_at_unix > now_unix
            {
                continue;
            }
            let n = samples as f64;
            let alpha = 1.0 / (n + 1.0);
            let d = ev.score_delta.to_f64();
            let c = score.to_f64();
            score = Dfp::from_f64(c * (1.0 - alpha) + d * alpha);
            samples += 1;
            if ev.recorded_at_unix >= last_event_unix {
                last_event_unix = ev.recorded_at_unix;
                last_event_id = ev.event_id;
            }
            updated_at_unix = ev.recorded_at_unix;
        }
        Ok(ReputationAggregate {
            recorder_did: *did,
            signal_kind: kind,
            layer,
            score_ewma: score,
            samples,
            severity_total,
            last_event_id,
            last_event_unix,
            updated_at_unix,
        })
    }

    async fn replay_for_audit(
        &self,
        did: &RecorderDid,
        since_unix: u64,
        until_unix: u64,
    ) -> StoreResult<Vec<SignalEvent>> {
        if since_unix > until_unix {
            return Err(ReputationError::ReplayWindowInverted);
        }
        let inner = self.inner.read().await;
        let h = did_hash(did);
        let mut out: Vec<SignalEvent> = inner
            .events
            .iter()
            .filter(|(k, ev)| {
                k.did_hash == h
                    && ev.recorded_at_unix >= since_unix
                    && ev.recorded_at_unix <= until_unix
            })
            .map(|(_, v)| v.clone())
            .collect();
        out.sort_by_key(|e| e.recorded_at_unix);
        Ok(out)
    }

    async fn retention_prune(&self, cutoff_unix: u64, now_unix: u64) -> StoreResult<u64> {
        if cutoff_unix > now_unix {
            return Err(ReputationError::RetentionCutoffFuture);
        }
        let mut inner = self.inner.write().await;
        let before = inner.events.len();
        inner
            .events
            .retain(|_, ev| ev.recorded_at_unix > cutoff_unix);
        Ok((before - inner.events.len()) as u64)
    }

    async fn prune_event(&self, event_id: EventId) -> StoreResult<()> {
        let mut inner = self.inner.write().await;
        let to_remove: Vec<EventKey> = inner
            .events
            .keys()
            .filter(|k| k.event_id == event_id)
            .copied()
            .collect();
        for k in to_remove {
            inner.events.remove(&k);
        }
        Ok(())
    }

    async fn register_recorder(&self, chain_ref: crate::auth::ChainRef) -> StoreResult<RecorderId> {
        verify_registration(&chain_ref)?;
        let mut inner = self.inner.write().await;
        let id = self.next_recorder_id.fetch_add(1, Ordering::SeqCst);
        let rid = RecorderId::from_u64(id);
        inner.recorders.insert(
            RecorderKey {
                did_hash: did_hash(&chain_ref.recorder_did),
            },
            (rid, false, false),
        );
        Ok(rid)
    }

    async fn verify_governance_suspension(
        &self,
        auth: &SuspensionAuth,
        snapshot: &GovernanceSnapshot,
        now_unix: u64,
    ) -> StoreResult<()> {
        if !snapshot.is_fresh(now_unix) {
            return Err(ReputationError::GovernanceSnapshotStale {
                age_secs: snapshot.age_secs(now_unix),
                max: crate::constants::MAX_GOVERNANCE_SNAPSHOT_AGE_SECS,
            });
        }
        if snapshot.governance_set_hash != auth.governance_set_hash {
            return Err(ReputationError::GovernanceSetHashMismatch);
        }
        if snapshot.quorum_count() < GOVERNANCE_QUORUM {
            return Err(ReputationError::GovernanceQuorumNotMet {
                signatures: snapshot.quorum_count(),
                quorum: GOVERNANCE_QUORUM,
            });
        }
        if auth.governance_pubkey == [0u8; 32] {
            return Err(ReputationError::GovernanceSignatureInvalid);
        }
        Ok(())
    }

    async fn suspend_recorder(
        &self,
        recorder_id: RecorderId,
        auth: SuspensionAuth,
        now_unix: u64,
    ) -> StoreResult<()> {
        // Caller must have invoked verify_governance_suspension first.
        let snapshot = auth.snapshot.clone();
        self.verify_governance_suspension(&auth, &snapshot, now_unix)
            .await?;
        let mut inner = self.inner.write().await;
        for (_k, v) in inner.recorders.iter_mut() {
            if v.0 == recorder_id {
                v.1 = true;
                return Ok(());
            }
        }
        Err(ReputationError::RecorderNotRegistered(recorder_id.to_u64()))
    }

    async fn slash_recorder(&self, proof: GovernanceProof) -> StoreResult<()> {
        let snap = proof.snapshot.clone();
        if !snap.is_fresh(0) {
            return Err(ReputationError::GovernanceSnapshotStale {
                age_secs: snap.age_secs(0),
                max: crate::constants::MAX_GOVERNANCE_SNAPSHOT_AGE_SECS,
            });
        }
        if snap.governance_set_hash != proof.governance_set_hash {
            return Err(ReputationError::GovernanceSetHashMismatch);
        }
        if snap.quorum_count() < GOVERNANCE_QUORUM {
            return Err(ReputationError::GovernanceQuorumNotMet {
                signatures: snap.quorum_count(),
                quorum: GOVERNANCE_QUORUM,
            });
        }
        let dest = proof
            .slash_destination
            .ok_or(ReputationError::SlashDestinationMismatch {
                expected: 0,
                actual: 0,
            })?;
        if proof.slash_asset == crate::auth::AssetTag::None {
            return Err(ReputationError::ChainRefInvalid("slash_asset"));
        }
        if proof.slash_amount == 0 {
            return Err(ReputationError::ChainRefInvalid("slash_amount"));
        }
        let _ = dest;
        Ok(())
    }

    async fn declare_retirement_eligible(
        &self,
        adapter: u8,
        evidence: ParityEvidence,
        proof: GovernanceProof,
        now_unix: u64,
    ) -> StoreResult<RetirementEligibility> {
        let snap = proof.snapshot.clone();
        if !snap.is_fresh(now_unix) {
            return Err(ReputationError::GovernanceSnapshotStale {
                age_secs: snap.age_secs(now_unix),
                max: crate::constants::MAX_GOVERNANCE_SNAPSHOT_AGE_SECS,
            });
        }
        if snap.governance_set_hash != proof.governance_set_hash {
            return Err(ReputationError::GovernanceSetHashMismatch);
        }
        if snap.quorum_count() < GOVERNANCE_QUORUM {
            return Err(ReputationError::GovernanceQuorumNotMet {
                signatures: snap.quorum_count(),
                quorum: GOVERNANCE_QUORUM,
            });
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(BLAKE3_REPUTATION_RETIREMENT_DOMAIN);
        hasher.update(&evidence.evidence_hash);
        hasher.update(&evidence.last_bucket_unix.to_be_bytes());
        hasher.update(&[adapter]);
        let out = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(out.as_bytes());
        Ok(RetirementEligibility {
            eligible: true,
            since_unix: now_unix,
            evidence_hash: arr,
            adapter,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AssetTag, ChainRef, SlashDestination};
    use crate::store::rotation_key;
    use crate::types::ControllerId;
    use crate::{ReputationError, StakeComponent};
    use octo_determin::Dfp;

    fn good_chain_ref() -> ChainRef {
        ChainRef {
            chain_id: 7,
            block_height: 100,
            tx_hash: [1u8; 32],
            recorder_did: RecorderDid::from_array([0u8; 52]),
            octo_stake: 4_000,
            role_stake: 1_000,
            role_token_kind: 1,
            lock_until_unix: 9_999_999_999,
        }
    }

    #[tokio::test]
    async fn register_recorder_happy_path() {
        let store = InMemoryReputationStore::new();
        let id = store.register_recorder(good_chain_ref()).await.unwrap();
        assert_eq!(id.to_u64(), 0);
    }

    #[tokio::test]
    async fn register_recorder_rejects_octo_below_minimum() {
        let store = InMemoryReputationStore::new();
        let mut cr = good_chain_ref();
        cr.octo_stake = 3_999;
        let err = store.register_recorder(cr).await.unwrap_err();
        assert_eq!(
            err,
            ReputationError::StakeBelowMinimum {
                component: StakeComponent::Octo,
            }
        );
    }

    #[tokio::test]
    async fn record_signal_increments_samples_and_ewma() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([0u8; 52]);
        let cid = ControllerId::from_array([0u8; 32]);
        let ev = SignalEvent {
            event_id: EventId::from_u64(0),
            recorder_did: did,
            controller_id: cid,
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(1.0),
            recorded_at_unix: 1_000,
            rotation_provenance: None,
            audit_ref: None,
        };
        let _id = store.record_signal(ev).await.unwrap();
        let agg = store
            .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
            .await
            .unwrap();
        assert_eq!(agg.samples, 1);
        // First sample: EWMA = 1.0 / 1 = 1.0
        assert!((agg.score_ewma.to_f64() - 1.0).abs() < 1e-12);
    }

    #[tokio::test]
    async fn cross_layer_empty_returns_err() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([0u8; 52]);
        let err = store
            .cross_layer_query(&did, SignalKind::Outcome, &[])
            .await
            .unwrap_err();
        assert_eq!(err, ReputationError::CrossLayerEmpty);
    }

    #[tokio::test]
    async fn retention_prune_future_cutoff_rejected() {
        let store = InMemoryReputationStore::new();
        let err = store.retention_prune(2000, 1000).await.unwrap_err();
        assert_eq!(err, ReputationError::RetentionCutoffFuture);
    }

    #[tokio::test]
    async fn slash_recorder_requires_destination() {
        let store = InMemoryReputationStore::new();
        let snap = GovernanceSnapshot {
            finalized_at_unix: 0,
            governance_set_hash: [0u8; 32],
            members: vec![[1u8; 32]; 3],
        };
        let proof = GovernanceProof {
            governance_pubkey: [1u8; 32],
            recorder_id: RecorderId::from_u64(0),
            reason_hash: [0u8; 32],
            signature: vec![],
            snapshot: snap,
            governance_set_hash: [0u8; 32],
            slash_destination: None,
            slash_amount: 100,
            slash_asset: AssetTag::Octo,
        };
        let err = store.slash_recorder(proof).await.unwrap_err();
        assert_eq!(err.discriminant(), 0x16);
    }

    #[tokio::test]
    async fn slash_recorder_accepts_full_proof() {
        let store = InMemoryReputationStore::new();
        let snap = GovernanceSnapshot {
            finalized_at_unix: 0,
            governance_set_hash: [0u8; 32],
            members: vec![[1u8; 32]; 3],
        };
        let proof = GovernanceProof {
            governance_pubkey: [1u8; 32],
            recorder_id: RecorderId::from_u64(0),
            reason_hash: [0u8; 32],
            signature: vec![],
            snapshot: snap,
            governance_set_hash: [0u8; 32],
            slash_destination: Some(SlashDestination::Treasury),
            slash_amount: 100,
            slash_asset: AssetTag::Octo,
        };
        store.slash_recorder(proof).await.unwrap();
    }

    #[tokio::test]
    async fn retirement_eligibility_returns_envelope_hash() {
        let store = InMemoryReputationStore::new();
        let snap = GovernanceSnapshot {
            finalized_at_unix: 1_000,
            governance_set_hash: [0u8; 32],
            members: vec![[1u8; 32]; 3],
        };
        let proof = GovernanceProof {
            governance_pubkey: [1u8; 32],
            recorder_id: RecorderId::from_u64(0),
            reason_hash: [0u8; 32],
            signature: vec![],
            snapshot: snap,
            governance_set_hash: [0u8; 32],
            slash_destination: None,
            slash_amount: 0,
            slash_asset: AssetTag::None,
        };
        let evidence = ParityEvidence {
            adapter: 0x03, // Marketplace
            parity_score: 9999,
            bucket_count: 100,
            first_bucket_unix: 0,
            last_bucket_unix: 86_400,
            evidence_hash: [1u8; 32],
        };
        let r = store
            .declare_retirement_eligible(0x03, evidence, proof, 1_000)
            .await
            .unwrap();
        assert!(r.eligible);
        assert_eq!(r.adapter, 0x03);
        assert_ne!(r.evidence_hash, [0u8; 32]);
    }

    #[test]
    fn rotation_key_is_deterministic() {
        use crate::types::RotationProvenance;
        let rp = RotationProvenance {
            new_did: RecorderDid::from_array([0u8; 52]),
            consumed_at_unix: 100,
            rotation_id: 7,
        };
        let k1 = rotation_key(&rp);
        let k2 = rotation_key(&rp);
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 52 + 8 + 8);
    }
}
