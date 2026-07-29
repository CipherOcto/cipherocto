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
        // R13 review: severity_total MUST sum per-event severities
        // over the window. SignalEvent has no per-event severity
        // field; derive from signal_kind (Slash contributes 1, others
        // contribute 0). Mirrors the stoolap backend's `severity_total`
        // aggregate semantics.
        let mut severity_total: u64 = 0;
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
            if matches!(ev.signal_kind, SignalKind::Slash) {
                severity_total = severity_total.saturating_add(1);
            }
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
        // R13 review (CRITICAL): `is_fresh(0)` / `age_secs(0)` always
        // returns true / 0 because `saturating_sub` of any
        // finalized_at_unix >= 0 from 0 is 0. Pass the real clock so
        // stale governance snapshots are rejected. Mirrors the
        // stoolap backend's use of `crate::migrations::now_unix()`.
        let now_unix = crate::migrations::now_unix();
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

    async fn anchor_pending(
        &self,
        batch_size: u32,
    ) -> StoreResult<Vec<(crate::types::EventId, [u8; 32])>> {
        let inner = self.inner.read().await;
        // Pending-anchor sweep: events with `anchor_tx_hash IS NULL`.
        // The in-memory backend has no real anchor tx yet, so the
        // returned `anchor_tx_hash` slot is filled with a 32-byte
        // placeholder of zeros. Live anchor jobs construct this list,
        // submit on-chain, then call `set_event_anchor_tx_hash` with
        // the real chain hash. Test fixture.
        let mut out: Vec<(crate::types::EventId, [u8; 32])> = inner
            .events
            .values()
            .filter(|e| e.anchor_tx_hash.is_none())
            .take(batch_size as usize)
            .map(|e| (e.event_id, [0u8; 32]))
            .collect();
        out.sort_by_key(|(id, _)| id.to_u64());
        Ok(out)
    }

    async fn set_event_anchor_tx_hash(
        &self,
        event_id: crate::types::EventId,
        anchor_tx_hash: [u8; 32],
    ) -> StoreResult<()> {
        let mut inner = self.inner.write().await;
        // R9 review (HIGH): event_ids are NOT globally unique across
        // recorders (the schema's composite PK is (recorder_did,
        // event_id) and event_ids reset per-recorder via the global
        // MAX counter). A linear scan over `events` keyed by event_id
        // alone can match multiple EventKey entries under distinct
        // did_hashs. We MUST count matches: 0 → not_found, 1 →
        // anchor, 2+ → ambiguous_event_id (refuse; the API contract
        // `set_event_anchor_tx_hash(event_id, hash)` cannot
        // disambiguate which recorder's row to mutate).
        //
        // This is the memory-backend analog of stoolap's R7-F2
        // composite-PK scope fix: the stoolap UPDATE is scoped to
        // (recorder_did, event_id) and the LIMIT-1 probe is followed
        // by a scoped write; for memory we explicitly reject
        // ambiguous matches.
        let mut matches: Vec<EventKey> = Vec::new();
        for k in inner.events.keys() {
            if k.event_id == event_id {
                matches.push(*k);
            }
        }
        match matches.len() {
            0 => Err(ReputationError::ChainRefInvalid(
                "set_event_anchor_tx_hash:event_not_found",
            )),
            1 => {
                let k = matches[0];
                let v = inner.events.get_mut(&k).expect("just-collected key");
                match v.anchor_tx_hash {
                    Some(existing) if existing != anchor_tx_hash => {
                        Err(ReputationError::ChainRefInvalid(
                            "set_event_anchor_tx_hash:anchor_already_set",
                        ))
                    }
                    Some(_) => Ok(()), // idempotent re-submit
                    None => {
                        v.anchor_tx_hash = Some(anchor_tx_hash);
                        Ok(())
                    }
                }
            }
            _ => Err(ReputationError::ChainRefInvalid(
                "set_event_anchor_tx_hash:ambiguous_event_id",
            )),
        }
    }

    async fn query_anchors_by_controller_id(
        &self,
        controller_id: crate::types::ControllerId,
    ) -> StoreResult<Vec<crate::store::AnchorRecord>> {
        let inner = self.inner.read().await;
        // Linear scan over the events map: filter by controller_id,
        // skip events without `anchor_tx_hash`, build AnchorRecord.
        let mut out: Vec<crate::store::AnchorRecord> = inner
            .events
            .values()
            .filter(|e| e.controller_id == controller_id)
            .filter_map(|e| {
                e.anchor_tx_hash.map(|h| crate::store::AnchorRecord {
                    event_id: e.event_id,
                    anchor_tx_hash: h,
                    recorded_at_unix: e.recorded_at_unix,
                })
            })
            .collect();
        // Round 4 review F2: tie-break by `event_id` ASC for
        // cross-backend determinism when timestamps tie.
        out.sort_by(|a, b| {
            a.recorded_at_unix
                .cmp(&b.recorded_at_unix)
                .then(a.event_id.cmp(&b.event_id))
        });
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
            anchor_tx_hash: None,
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
        // R13 review: snapshot must be fresh at now_unix for the
        // freshness gate to pass; use `crate::migrations::now_unix()`
        // as the finalized_at_unix so the snapshot's age = 0.
        let now = crate::migrations::now_unix();
        let snap = GovernanceSnapshot {
            finalized_at_unix: now,
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
        // R13 review: use a fresh snapshot so the is_fresh gate
        // passes (post-fix the gate reads the real clock).
        let now = crate::migrations::now_unix();
        let snap = GovernanceSnapshot {
            finalized_at_unix: now,
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

    /// R13 review (CRITICAL): stale governance snapshots MUST be
    /// rejected. Pre-fix, `is_fresh(0)` always returned true (since
    /// `0u64.saturating_sub(snap.finalized_at_unix) = 0` for any
    /// `finalized_at_unix >= 0`), allowing any old signed governance
    /// proof to authorize a slash. Post-fix the gate reads the real
    /// clock.
    #[tokio::test]
    async fn slash_recorder_rejects_stale_snapshot() {
        let store = InMemoryReputationStore::new();
        // Pick a timestamp well in the past so age_secs(now) >
        // MAX_GOVERNANCE_SNAPSHOT_AGE_SECS (600s by default).
        let stale = 1_700_000_000; // ~Jan 2024 (current epoch is 2026)
        let snap = GovernanceSnapshot {
            finalized_at_unix: stale,
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
        let err = store.slash_recorder(proof).await.unwrap_err();
        match err {
            ReputationError::GovernanceSnapshotStale { age_secs, .. } => {
                assert!(age_secs > 0, "stale snapshot must report positive age");
            }
            other => panic!("expected GovernanceSnapshotStale, got {other:?}"),
        }
    }

    /// R13 review (HIGH): sliding_window MUST accumulate
    /// severity_total from the per-event severities in the window.
    /// Pre-fix, the variable was hardcoded to 0 and never mutated
    /// inside the loop, silently disagreeing with the stoolap backend.
    #[tokio::test]
    async fn sliding_window_severity_total_counts_slash_events() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([0xD2u8; 52]);
        for i in 0..3u64 {
            store
                .record_signal(SignalEvent {
                    event_id: EventId::from_u64(i),
                    recorder_did: did,
                    controller_id: ControllerId::from_array([0u8; 32]),
                    signal_kind: SignalKind::Slash,
                    layer: ReputationLayer::Market,
                    score_delta: Dfp::from_f64(1.0),
                    recorded_at_unix: 1_000 + i,
                    rotation_provenance: None,
                    audit_ref: None,
                    anchor_tx_hash: None,
                })
                .await
                .expect("record");
        }
        let agg = store
            .sliding_window(&did, SignalKind::Slash, ReputationLayer::Market, 600, 1_500)
            .await
            .expect("sliding");
        assert_eq!(
            agg.severity_total, 3,
            "sliding_window must sum severities over the window; got {}",
            agg.severity_total
        );
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

    #[tokio::test]
    async fn query_anchors_by_controller_id_filters_and_orders() {
        use crate::store::AnchorRecord;
        let store = InMemoryReputationStore::new();
        let c1 = ControllerId::from_array([1u8; 32]);
        let c2 = ControllerId::from_array([2u8; 32]);
        let make = |cid: ControllerId, ts: u64| SignalEvent {
            event_id: EventId::from_u64(0), // storage assigns
            recorder_did: RecorderDid::from_array([0u8; 52]),
            controller_id: cid,
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: ts,
            rotation_provenance: None,
            audit_ref: None,
            anchor_tx_hash: None,
        };
        let e1 = store
            .record_signal(make(c1, 1_000))
            .await
            .expect("record 1");
        let e2 = store
            .record_signal(make(c1, 2_000))
            .await
            .expect("record 2");
        let _e3 = store
            .record_signal(make(c1, 3_000))
            .await
            .expect("record 3 (unanchored)");
        let e4 = store
            .record_signal(make(c2, 4_000))
            .await
            .expect("record 4");
        // Anchor e1, e2, e4 (e3 left unanchored).
        for ev_id in [e1, e2, e4] {
            store
                .set_event_anchor_tx_hash(ev_id, [0xAA; 32])
                .await
                .expect("set anchor");
        }
        let c1_out = store
            .query_anchors_by_controller_id(c1)
            .await
            .expect("query c1");
        assert_eq!(c1_out.len(), 2, "e1+e2 anchored under c1, e3 unanchored");
        // recorded_at_unix ASC ordering: e1 (ts=1000) before e2 (ts=2000).
        assert_eq!(c1_out[0].event_id.to_u64(), e1.to_u64());
        assert_eq!(c1_out[1].event_id.to_u64(), e2.to_u64());
        assert_eq!(c1_out[0].anchor_tx_hash, [0xAA; 32]);
        // Round 4 fix: field renamed `anchored_at_unix` →
        // `recorded_at_unix` (proxy field; round 4 F2).
        assert_eq!(c1_out[0].recorded_at_unix, 1_000);
        assert_eq!(c1_out[1].recorded_at_unix, 2_000);
        let c2_out = store
            .query_anchors_by_controller_id(c2)
            .await
            .expect("query c2");
        assert_eq!(c2_out.len(), 1);
        assert_eq!(c2_out[0].event_id.to_u64(), e4.to_u64());
        assert_eq!(c2_out[0].recorded_at_unix, 4_000);
        // Empty for an unknown controller.
        let unknown = store
            .query_anchors_by_controller_id(ControllerId::from_array([0xFF; 32]))
            .await
            .expect("query unknown");
        assert!(unknown.is_empty());
        // AnchorRecord is the new struct, not the legacy event.
        let _: AnchorRecord = c1_out[0].clone();
    }

    /// Round 2 review #6: set_event_anchor_tx_hash on a missing
    /// event_id must error with ChainRefInvalid (not silently Ok).
    #[tokio::test]
    async fn set_event_anchor_tx_hash_missing_event_id_errors() {
        let store = InMemoryReputationStore::new();
        // No record_signal: next_event_id is 0, no event exists.
        let err = store
            .set_event_anchor_tx_hash(EventId::from_u64(999), [0xAA; 32])
            .await
            .unwrap_err();
        match err {
            ReputationError::ChainRefInvalid("set_event_anchor_tx_hash:event_not_found") => {}
            other => panic!("expected ChainRefInvalid(event_not_found), got {other:?}"),
        }
    }

    /// R8 review: memory backend must mirror stoolap's three-way
    /// anchor semantics — same hash is idempotent, different hash
    /// is rejected (was silently overwritten before R8).
    #[tokio::test]
    async fn set_event_anchor_tx_hash_rejects_re_anchor_with_different_hash() {
        let store = InMemoryReputationStore::new();
        let did = RecorderDid::from_array([0xC8; 52]);
        let cid = ControllerId::from_array([0u8; 32]);
        let eid = store
            .record_signal(SignalEvent {
                event_id: EventId::from_u64(0), // storage assigns
                recorder_did: did,
                controller_id: cid,
                signal_kind: SignalKind::Outcome,
                layer: ReputationLayer::Market,
                score_delta: octo_determin::Dfp::from_f64(0.5),
                recorded_at_unix: 1_000,
                rotation_provenance: None,
                audit_ref: None,
                anchor_tx_hash: None,
            })
            .await
            .expect("record");
        // First anchor: succeeds.
        store
            .set_event_anchor_tx_hash(eid, [0xAA; 32])
            .await
            .expect("first anchor");
        // Same-hash re-submit: idempotent.
        store
            .set_event_anchor_tx_hash(eid, [0xAA; 32])
            .await
            .expect("idempotent same-hash");
        // Different-hash re-submit: MUST error (parity with stoolap).
        let err = store
            .set_event_anchor_tx_hash(eid, [0xBB; 32])
            .await
            .unwrap_err();
        match err {
            ReputationError::ChainRefInvalid("set_event_anchor_tx_hash:anchor_already_set") => {}
            other => panic!("expected anchor_already_set, got {other:?}"),
        }
    }

    /// R9 review (HIGH): cross-recorder event_id collision. event_ids
    /// are global-monotonic but stored under composite PK
    /// (recorder_did, event_id); two distinct recorders can each
    /// have an event with the same event_id (e.g., both their first
    /// events after R5-F4 if both have empty aggregates). Memory
    /// backend MUST refuse to anchor when the call is ambiguous
    /// (multiple did_hashs match); otherwise an attacker can race
    /// the iteration order to overwrite a victim's anchor (or
    /// non-deterministically land on the wrong row).
    #[tokio::test]
    async fn set_event_anchor_tx_hash_ambiguous_event_id_errors() {
        let store = InMemoryReputationStore::new();
        let r_a = RecorderDid::from_array([0xA1; 52]);
        let r_b = RecorderDid::from_array([0xA2; 52]);
        let cid = ControllerId::from_array([0u8; 32]);
        let mk = |did: RecorderDid| SignalEvent {
            event_id: EventId::from_u64(0),
            recorder_did: did,
            controller_id: cid,
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: octo_determin::Dfp::from_f64(0.5),
            recorded_at_unix: 1_000,
            rotation_provenance: None,
            audit_ref: None,
            anchor_tx_hash: None,
        };
        let e_a = store.record_signal(mk(r_a)).await.expect("r_a");
        let _e_b = store.record_signal(mk(r_b)).await.expect("r_b");
        // After R5-F4 with empty aggregates both first events share
        // event_id=1; r_b's record_signal bumps MAX to 2 → r_b=2?
        // No: r_a inserts first, last_event_id=1 in aggregates;
        // then r_b reads MAX=1, returns 2. So they DO NOT share id
        // unless we seed two events with the same id manually.
        // For this test, just verify the API rejects ambiguous
        // matches: forge the situation by inserting a row directly
        // with the same event_id under both recorders.
        let shared_id = EventId::from_u64(99);
        // r_a already has e_a=1. Insert another event for r_a with
        // event_id=99, and one for r_b with event_id=99.
        store.inner.write().await.events.insert(
            crate::store::memory::EventKey {
                did_hash: did_hash(&r_a),
                event_id: shared_id,
            },
            SignalEvent {
                event_id: shared_id,
                recorder_did: r_a,
                controller_id: cid,
                signal_kind: SignalKind::Outcome,
                layer: ReputationLayer::Market,
                score_delta: octo_determin::Dfp::from_f64(0.5),
                recorded_at_unix: 2_000,
                rotation_provenance: None,
                audit_ref: None,
                anchor_tx_hash: None,
            },
        );
        store.inner.write().await.events.insert(
            crate::store::memory::EventKey {
                did_hash: did_hash(&r_b),
                event_id: shared_id,
            },
            SignalEvent {
                event_id: shared_id,
                recorder_did: r_b,
                controller_id: cid,
                signal_kind: SignalKind::Outcome,
                layer: ReputationLayer::Market,
                score_delta: octo_determin::Dfp::from_f64(0.5),
                recorded_at_unix: 3_000,
                rotation_provenance: None,
                audit_ref: None,
                anchor_tx_hash: None,
            },
        );
        let err = store
            .set_event_anchor_tx_hash(shared_id, [0xAA; 32])
            .await
            .unwrap_err();
        match err {
            ReputationError::ChainRefInvalid("set_event_anchor_tx_hash:ambiguous_event_id") => {}
            other => panic!("expected ambiguous_event_id, got {other:?}"),
        }
        // Sanity: the original e_a anchor (unrelated) still works.
        store
            .set_event_anchor_tx_hash(e_a, [0xBB; 32])
            .await
            .expect("unique match anchors");
    }

    /// R10 review (MEDIUM): 3-way collision (r_a, r_b, r_c) sharing
    /// the same event_id. The `>= 2` branch in the production
    /// match arm must still trigger; this test pins the threshold
    /// at exactly 2 (not 3+) by extending with a third EventKey.
    #[tokio::test]
    async fn set_event_anchor_tx_hash_three_way_collision_errors() {
        let store = InMemoryReputationStore::new();
        let r_a = RecorderDid::from_array([0xB1; 52]);
        let r_b = RecorderDid::from_array([0xB2; 52]);
        let r_c = RecorderDid::from_array([0xB3; 52]);
        let cid = ControllerId::from_array([0u8; 32]);
        let shared_id = EventId::from_u64(7);
        let seed = |did: RecorderDid, ts: u64| {
            (
                EventKey {
                    did_hash: did_hash(&did),
                    event_id: shared_id,
                },
                SignalEvent {
                    event_id: shared_id,
                    recorder_did: did,
                    controller_id: cid,
                    signal_kind: SignalKind::Outcome,
                    layer: ReputationLayer::Market,
                    score_delta: octo_determin::Dfp::from_f64(0.5),
                    recorded_at_unix: ts,
                    rotation_provenance: None,
                    audit_ref: None,
                    anchor_tx_hash: None,
                },
            )
        };
        let mut inner = store.inner.write().await;
        for did in [r_a, r_b, r_c] {
            let (k, v) = seed(did, 1_000);
            inner.events.insert(k, v);
        }
        drop(inner);
        let err = store
            .set_event_anchor_tx_hash(shared_id, [0xCC; 32])
            .await
            .unwrap_err();
        match err {
            ReputationError::ChainRefInvalid("set_event_anchor_tx_hash:ambiguous_event_id") => {}
            other => panic!("expected ambiguous_event_id, got {other:?}"),
        }
        // No row anchored.
        let inner = store.inner.read().await;
        let anchored: Vec<_> = inner
            .events
            .values()
            .filter(|e| e.event_id == shared_id && e.anchor_tx_hash.is_some())
            .collect();
        assert_eq!(
            anchored.len(),
            0,
            "3-way collision must leave no rows anchored; got {}",
            anchored.len()
        );
    }

    /// Round 4 review #2: query_anchors_by_controller_id ordering
    /// breaks ties by event_id ASC for cross-backend determinism.
    #[tokio::test]
    async fn query_anchors_tie_breaks_by_event_id_asc() {
        let store = InMemoryReputationStore::new();
        let cid = ControllerId::from_array([7u8; 32]);
        // 3 events under one controller with the same recorded_at_unix.
        let make = |ts: u64| SignalEvent {
            event_id: EventId::from_u64(0), // storage assigns
            recorder_did: RecorderDid::from_array([0u8; 52]),
            controller_id: cid,
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(0.5),
            recorded_at_unix: ts,
            rotation_provenance: None,
            audit_ref: None,
            anchor_tx_hash: None,
        };
        let _e_a = store.record_signal(make(5_000)).await.expect("a");
        let _e_b = store.record_signal(make(5_000)).await.expect("b");
        let _e_c = store.record_signal(make(5_000)).await.expect("c");
        // Anchor all 3 — the storage-assigned event_ids are
        // monotonically increasing, so the tie-break should order
        // them ascending (whichever ids storage assigned).
        store
            .set_event_anchor_tx_hash(EventId::from_u64(0), [0xAA; 32])
            .await
            .expect("anchor 0");
        store
            .set_event_anchor_tx_hash(EventId::from_u64(1), [0xBB; 32])
            .await
            .expect("anchor 1");
        store
            .set_event_anchor_tx_hash(EventId::from_u64(2), [0xCC; 32])
            .await
            .expect("anchor 2");
        let out = store
            .query_anchors_by_controller_id(cid)
            .await
            .expect("query");
        assert_eq!(out.len(), 3);
        let ids: Vec<u64> = out.iter().map(|r| r.event_id.to_u64()).collect();
        assert_eq!(
            ids,
            vec![0, 1, 2],
            "tie-break must order by event_id ASC for identical timestamps"
        );
        assert_eq!(
            out.iter().map(|r| r.recorded_at_unix).collect::<Vec<_>>(),
            vec![5_000, 5_000, 5_000]
        );
    }
}
