//! In-memory `ReputationStore` for unit + integration tests.
//!
//! Backed by `tokio::sync::RwLock<HashMap<…>>`. Operations are deterministic
//! and side-effect free beyond the in-process map.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use octo_determin::Dfp;
use tokio::sync::RwLock;

use crate::auth::{
    Attestation, AttestorId, AttestorRegistration, GovernanceProof, GovernanceSnapshot,
    SuspensionAuth,
};
use crate::constants::GOVERNANCE_QUORUM;
use crate::error::ReputationError;
use crate::gossip::GossipCatchUp;
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
    attestors: HashMap<AttestorId, AttestorRegistration>,
    attestations: HashMap<(AttestorId, EventId), Attestation>,
    next_attestation_id: u64,
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
        // Stubbed governance + evidence validation per [[deferred-vs-unspecified]].
        crate::retirement::stub_verify_proof_shape(&proof, now_unix)?;
        crate::retirement::validate_evidence(&evidence)?;
        let arr = crate::retirement::retirement_envelope_hash(
            &evidence.evidence_hash,
            evidence.last_bucket_unix,
            adapter,
        );
        Ok(RetirementEligibility {
            eligible: true,
            since_unix: now_unix,
            evidence_hash: arr,
            adapter,
        })
    }

    // -- Federation (Session 7 / mission 0968 Phase 4) --

    async fn register_attestor(
        &self,
        registration: AttestorRegistration,
    ) -> StoreResult<AttestorId> {
        if registration.attestor_did.as_bytes() == &[0u8; 52] {
            return Err(ReputationError::RecorderDidMalformed(
                "attestor did must not be all-zero",
            ));
        }
        if registration.pubkey == [0u8; 32] {
            return Err(ReputationError::GossipEnvelopeInvalid(
                "attestor_pubkey_zero",
            ));
        }
        let mut inner = self.inner.write().await;
        // Idempotent: re-registering updates the row.
        inner
            .attestors
            .insert(registration.attestor_did, registration.clone());
        Ok(registration.attestor_did)
    }

    async fn attestor_lookup_did(
        &self,
        attestor_did: &AttestorId,
    ) -> StoreResult<AttestorRegistration> {
        let inner = self.inner.read().await;
        inner
            .attestors
            .get(attestor_did)
            .cloned()
            .ok_or(ReputationError::RecorderNotRegistered(0))
    }

    async fn record_attestation(&self, attestation: Attestation) -> StoreResult<u64> {
        let key = (attestation.attestor, attestation.event_id);
        let mut inner = self.inner.write().await;
        if let Some(existing) = inner.attestations.get(&key) {
            return Ok(existing.attestation_id);
        }
        let id = inner.next_attestation_id;
        inner.next_attestation_id += 1;
        let mut att = attestation;
        att.attestation_id = id;
        inner.attestations.insert(key, att);
        Ok(id)
    }

    async fn query_attestations(
        &self,
        recorder_did: &RecorderDid,
        since_event_id: EventId,
    ) -> StoreResult<Vec<Attestation>> {
        let inner = self.inner.read().await;
        let since = since_event_id.to_u64();
        let mut out: Vec<Attestation> = inner
            .attestations
            .values()
            .filter(|a| a.recorder_did == *recorder_did && a.event_id.to_u64() > since)
            .cloned()
            .collect();
        out.sort_by_key(|a| a.observed_at_unix);
        Ok(out)
    }

    async fn attestor_quorum_reached(&self, event_id: EventId) -> StoreResult<bool> {
        let inner = self.inner.read().await;
        let distinct: std::collections::HashSet<AttestorId> = inner
            .attestations
            .values()
            .filter(|a| a.event_id == event_id)
            .map(|a| a.attestor)
            .collect();
        Ok(distinct.len() as u32 >= crate::constants::MIN_ATTESTOR_QUORUM)
    }

    async fn gossip_catch_up(
        &self,
        catch_up: &GossipCatchUp,
    ) -> StoreResult<Vec<crate::types::SignalEvent>> {
        let inner = self.inner.read().await;
        let since = catch_up.since_event_id.to_u64();
        // The catch-up path returns the recorder's events with id >
        // since_event_id, joined to attestations from `attestor_did`.
        // We stream every matching event for the (catch-up) attestor's
        // subscribed recorders; in-memory impl returns ALL events with
        // event_id > since (the gossip substrate filters per-topic).
        let mut out: Vec<crate::types::SignalEvent> = inner
            .events
            .values()
            .filter(|e| e.event_id.to_u64() > since)
            .cloned()
            .collect();
        out.sort_by_key(|e| e.event_id.to_u64());
        Ok(out)
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let snap = GovernanceSnapshot {
            finalized_at_unix: now,
            governance_set_hash: [1u8; 32],
            members: vec![[1u8; 32], [2u8; 32], [3u8; 32]],
        };
        let proof = GovernanceProof {
            governance_pubkey: [1u8; 32],
            recorder_id: RecorderId::from_u64(0),
            reason_hash: [0u8; 32],
            signature: vec![0u8; 96], // 3 × 32-byte stub sigs
            snapshot: snap,
            governance_set_hash: [1u8; 32],
            slash_destination: None,
            slash_amount: 0,
            slash_asset: AssetTag::None,
        };
        let evidence = ParityEvidence {
            adapter: 0x03, // Marketplace
            parity_score: 9_999,
            bucket_count: 24,
            first_bucket_unix: 0,
            last_bucket_unix: 86_400,
            evidence_hash: [1u8; 32],
        };
        let r = store
            .declare_retirement_eligible(0x03, evidence, proof, now)
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

    // -- Session 7 / mission 0968 Phase 4 federation --

    #[tokio::test]
    async fn register_attestor_then_lookup_round_trips() {
        use crate::auth::{AttestorId, AttestorRegistration};
        let store = InMemoryReputationStore::new();
        let attestor = AttestorRegistration {
            attestor_did: AttestorId::from_array([0xAA; 52]),
            pubkey: [0xBB; 32],
            peer_set_id: [0xCC; 32],
            requested_at_unix: 1_000,
            registered_at_unix: 1_500,
        };
        let did = store
            .register_attestor(attestor.clone())
            .await
            .expect("reg");
        assert_eq!(did, attestor.attestor_did);
        let back = store
            .attestor_lookup_did(&attestor.attestor_did)
            .await
            .expect("lookup");
        assert_eq!(back, attestor);
    }

    #[tokio::test]
    async fn record_attestation_is_idempotent_on_composite_key() {
        use crate::auth::{Attestation, AttestorId};
        let store = InMemoryReputationStore::new();
        let attestor = AttestorId::from_array([0xAA; 52]);
        let recorder = RecorderDid::from_array([0x11; 52]);
        let eid = EventId::from_u64(42);
        let att = Attestation {
            attestation_id: 0,
            attestor,
            recorder_did: recorder,
            event_id: eid,
            signature: vec![1u8; 64],
            observed_at_unix: 1_000,
            received_at_unix: 1_500,
            source_mission: "mon:test".into(),
            source_domain: "domain:adapter:test".into(),
        };
        let id1 = store.record_attestation(att.clone()).await.expect("first");
        let id2 = store.record_attestation(att).await.expect("second");
        assert_eq!(id1, id2, "duplicate attestation returns same id");
    }

    #[tokio::test]
    async fn query_attestations_filters_by_since_event_id() {
        use crate::auth::{Attestation, AttestorId};
        let store = InMemoryReputationStore::new();
        let recorder = RecorderDid::from_array([0u8; 52]);
        for i in 0..5u64 {
            store
                .record_attestation(Attestation {
                    attestation_id: 0,
                    attestor: AttestorId::from_array([i as u8; 52]),
                    recorder_did: recorder,
                    event_id: EventId::from_u64(i + 1),
                    signature: vec![1u8; 64],
                    observed_at_unix: 1_000,
                    received_at_unix: 1_500,
                    source_mission: "mon:test".into(),
                    source_domain: "domain:adapter:test".into(),
                })
                .await
                .expect("record");
        }
        let out = store
            .query_attestations(&recorder, EventId::from_u64(2)) // > 2
            .await
            .expect("query");
        assert_eq!(out.len(), 3, "events 3, 4, 5 should match");
        let mut ids: Vec<u64> = out.iter().map(|a| a.event_id.to_u64()).collect();
        ids.sort();
        assert_eq!(ids, vec![3, 4, 5]);
    }
}
