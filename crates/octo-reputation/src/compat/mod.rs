//! Phase 2 compat adapter — shadow-write every `record_signal` to a legacy
//! `LegacyReputationStore` while the canonical Dfp store remains the source
//! of truth. Per `missions/claimed/0968-reputation-persistence.md` Phase 2
//! acceptance criteria + mission 0968-b Phase A compatibility adapter.

mod determinism;
mod keymap;
mod legacy;

pub use determinism::{deterministic_f64_mirror, F64MirrorPolicy};
pub use keymap::{CompatKeymap, CompatMapping};
pub use legacy::{DcRootedSlashReputationStore, LegacyReputationStore, SlashReputationStore};

use crate::auth::{
    Attestation, AttestorId, AttestorRegistration, GovernanceProof, GovernanceSnapshot,
    SuspensionAuth,
};
use crate::error::ReputationError;
use crate::store::{ReputationStore, StoreResult};
use crate::types::{
    EventId, ParityEvidence, RecorderDid, ReputationAggregate, ReputationLayer,
    RetirementEligibility, SignalEvent, SignalKind,
};

/// Shadow-write wrapper. Every write to the inner store also writes to a
/// legacy store via the per-event score mirror. Read methods go to the inner
/// store; the legacy store is consulted only for the dual-read parity
/// reconciliation report (Phase 2.5 binary).
pub struct ReputationStoreCompat<S: ReputationStore, L: LegacyReputationStore> {
    inner: S,
    legacy: L,
    mirror: F64MirrorPolicy,
}

impl<S: ReputationStore, L: LegacyReputationStore> ReputationStoreCompat<S, L> {
    pub fn new(inner: S, legacy: L) -> Self {
        Self {
            inner,
            legacy,
            mirror: F64MirrorPolicy::default(),
        }
    }

    pub fn with_mirror(inner: S, legacy: L, mirror: F64MirrorPolicy) -> Self {
        Self {
            inner,
            legacy,
            mirror,
        }
    }

    pub fn inner(&self) -> &S {
        &self.inner
    }

    pub fn legacy(&self) -> &L {
        &self.legacy
    }
}

impl<S: ReputationStore, L: LegacyReputationStore> ReputationStore for ReputationStoreCompat<S, L> {
    async fn record_signal(&self, event: SignalEvent) -> StoreResult<EventId> {
        // Mirror first so we record exactly the same delta the inner store sees.
        let delta = self.mirror.mirror_dfp(event.score_delta);
        let recorder_did = event.recorder_did;
        let signal_kind = event.signal_kind;
        let layer = event.layer;
        let ts = event.recorded_at_unix;

        let id = self.inner.record_signal(event).await?;

        // Shadow write to the legacy store. Failures are non-fatal for the
        // canonical write but are logged in the compat layer for the parity
        // reconciliation report.
        if let Err(e) = self
            .legacy
            .shadow_record(&recorder_did, signal_kind, layer, delta, ts)
        {
            // Surface as `ChainRefInvalid("legacy_shadow_failed")` so the
            // parity report can classify the failure without crashing.
            return Err(ReputationError::ChainRefInvalid(match e {
                LegacyShadowError::UnsupportedKind => "legacy_shadow_failed:unsupported_kind",
                LegacyShadowError::UnsupportedLayer => "legacy_shadow_failed:unsupported_layer",
            }));
        }

        Ok(id)
    }

    async fn read_aggregate(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layer: ReputationLayer,
    ) -> StoreResult<ReputationAggregate> {
        self.inner.read_aggregate(did, kind, layer).await
    }

    async fn cross_layer_query(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layers: &[ReputationLayer],
    ) -> StoreResult<Vec<ReputationAggregate>> {
        self.inner.cross_layer_query(did, kind, layers).await
    }

    async fn sliding_window(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layer: ReputationLayer,
        window_secs: u64,
        now_unix: u64,
    ) -> StoreResult<ReputationAggregate> {
        self.inner
            .sliding_window(did, kind, layer, window_secs, now_unix)
            .await
    }

    async fn replay_for_audit(
        &self,
        did: &RecorderDid,
        since_unix: u64,
        until_unix: u64,
    ) -> StoreResult<Vec<SignalEvent>> {
        self.inner
            .replay_for_audit(did, since_unix, until_unix)
            .await
    }

    async fn retention_prune(&self, cutoff_unix: u64, now_unix: u64) -> StoreResult<u64> {
        self.inner.retention_prune(cutoff_unix, now_unix).await
    }

    async fn prune_event(&self, event_id: EventId) -> StoreResult<()> {
        self.inner.prune_event(event_id).await
    }

    async fn register_recorder(
        &self,
        chain_ref: crate::auth::ChainRef,
    ) -> StoreResult<crate::types::RecorderId> {
        self.inner.register_recorder(chain_ref).await
    }

    async fn verify_governance_suspension(
        &self,
        auth: &SuspensionAuth,
        snapshot: &GovernanceSnapshot,
        now_unix: u64,
    ) -> StoreResult<()> {
        self.inner
            .verify_governance_suspension(auth, snapshot, now_unix)
            .await
    }

    async fn suspend_recorder(
        &self,
        recorder_id: crate::types::RecorderId,
        auth: SuspensionAuth,
        now_unix: u64,
    ) -> StoreResult<()> {
        self.inner
            .suspend_recorder(recorder_id, auth, now_unix)
            .await
    }

    async fn slash_recorder(&self, proof: GovernanceProof) -> StoreResult<()> {
        self.inner.slash_recorder(proof).await
    }

    async fn declare_retirement_eligible(
        &self,
        adapter: u8,
        evidence: ParityEvidence,
        proof: GovernanceProof,
        now_unix: u64,
    ) -> StoreResult<RetirementEligibility> {
        self.inner
            .declare_retirement_eligible(adapter, evidence, proof, now_unix)
            .await
    }

    // -- Federation (Session 7 / mission 0968 Phase 4) --
    //
    // These four methods forward to the inner store only — the legacy
    // `LegacyReputationStore` shim does not have an attestor concept
    // (the pre-RFC-0968 pubkey-keyed model pre-dates the federation
    // substrate). Federation reads are a pure canonical-store
    // operation; the legacy store is not consulted.

    async fn register_attestor(
        &self,
        registration: AttestorRegistration,
    ) -> StoreResult<AttestorId> {
        self.inner.register_attestor(registration).await
    }

    async fn attestor_lookup_did(
        &self,
        attestor_did: &AttestorId,
    ) -> StoreResult<AttestorRegistration> {
        self.inner.attestor_lookup_did(attestor_did).await
    }

    async fn record_attestation(&self, attestation: Attestation) -> StoreResult<u64> {
        self.inner.record_attestation(attestation).await
    }

    async fn query_attestations(
        &self,
        recorder_did: &RecorderDid,
        since_event_id: crate::types::EventId,
    ) -> StoreResult<Vec<Attestation>> {
        self.inner
            .query_attestations(recorder_did, since_event_id)
            .await
    }

    async fn attestor_quorum_reached(&self, event_id: EventId) -> StoreResult<bool> {
        self.inner.attestor_quorum_reached(event_id).await
    }

    async fn gossip_catch_up(
        &self,
        catch_up: &crate::gossip::GossipCatchUp,
    ) -> StoreResult<Vec<crate::types::SignalEvent>> {
        self.inner.gossip_catch_up(catch_up).await
    }
}

/// Reasons a legacy shadow write can fail. The compat layer maps each to a
/// distinct `ChainRefInvalid` sub-string so the parity binary can classify
/// failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyShadowError {
    UnsupportedKind,
    UnsupportedLayer,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryReputationStore;
    use octo_determin::Dfp;

    fn dummy_event(seed: u64, score: f64, ts: u64) -> SignalEvent {
        SignalEvent {
            event_id: EventId::from_u64(seed),
            recorder_did: RecorderDid::from_array([seed as u8; 52]),
            controller_id: crate::ControllerId::from_array([0u8; 32]),
            signal_kind: SignalKind::Outcome,
            layer: ReputationLayer::Market,
            score_delta: Dfp::from_f64(score),
            recorded_at_unix: ts,
            rotation_provenance: None,
            audit_ref: None,
        }
    }

    #[tokio::test]
    async fn compat_shadow_writes_both_stores() {
        let inner = InMemoryReputationStore::new();
        let legacy = SlashReputationStore::new();
        let compat = ReputationStoreCompat::new(inner, legacy);

        let _id = compat
            .record_signal(dummy_event(1, 1.0, 1000))
            .await
            .unwrap();

        // Inner store sees the event.
        let did = RecorderDid::from_array([1u8; 52]);
        let agg = compat
            .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
            .await
            .unwrap();
        assert_eq!(agg.samples, 1);

        // Legacy store sees the mirrored event via the same compat instance.
        let legacy_rate = compat.legacy().success_rate(&did);
        assert!(
            (legacy_rate - 1.0).abs() < 1e-9,
            "legacy f64 mirror must equal 1.0, got {legacy_rate}"
        );
    }

    #[tokio::test]
    async fn compat_determinism_byte_identical_across_replays() {
        let mk = || {
            let inner = InMemoryReputationStore::new();
            let legacy = SlashReputationStore::new();
            ReputationStoreCompat::new(inner, legacy)
        };
        let a = mk();
        let b = mk();
        for i in 0..50u64 {
            let ev = dummy_event(i + 1, 0.5, 1000 + i);
            a.record_signal(ev.clone()).await.unwrap();
            b.record_signal(ev).await.unwrap();
        }
        let did = RecorderDid::from_array([1u8; 52]);
        let agg_a = a
            .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
            .await
            .unwrap();
        let agg_b = b
            .read_aggregate(&did, SignalKind::Outcome, ReputationLayer::Market)
            .await
            .unwrap();
        assert_eq!(agg_a.samples, agg_b.samples);
        assert_eq!(
            agg_a.score_ewma.to_f64().to_bits(),
            agg_b.score_ewma.to_f64().to_bits(),
            "score_ewma must be byte-identical between two compat stores"
        );
    }
}
