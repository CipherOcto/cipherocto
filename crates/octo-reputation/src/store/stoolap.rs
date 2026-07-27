//! `StoolapReputationStore` — backend impl over the CipherOcto stoolap fork.
//!
//! **Status (Session 2):** stub. Construction and schema land in Session 4
//! when the parity binary composes both backends. The stub impls below keep
//! the trait satisfied so downstream code can name the type. Each method
//! returns `ChainRefInvalid("stoolap_backend_unimplemented:<name>")`.

use crate::auth::{GovernanceProof, GovernanceSnapshot, SuspensionAuth};
use crate::error::ReputationError;
use crate::store::{ReputationStore, StoreResult};
use crate::types::{
    EventId, ParityEvidence, RecorderDid, ReputationAggregate, ReputationLayer,
    RetirementEligibility, SignalEvent, SignalKind,
};

/// Marker type. Construction is gated on Session 4.
pub struct StoolapReputationStore;

fn stub<T>(name: &str) -> StoreResult<T> {
    Err(ReputationError::ChainRefInvalid(match name {
        "record_signal" => "stoolap_backend_unimplemented:record_signal",
        "read_aggregate" => "stoolap_backend_unimplemented:read_aggregate",
        "cross_layer_query" => "stoolap_backend_unimplemented:cross_layer_query",
        "sliding_window" => "stoolap_backend_unimplemented:sliding_window",
        "replay_for_audit" => "stoolap_backend_unimplemented:replay_for_audit",
        "retention_prune" => "stoolap_backend_unimplemented:retention_prune",
        "prune_event" => "stoolap_backend_unimplemented:prune_event",
        "register_recorder" => "stoolap_backend_unimplemented:register_recorder",
        "verify_governance_suspension" => {
            "stoolap_backend_unimplemented:verify_governance_suspension"
        }
        "suspend_recorder" => "stoolap_backend_unimplemented:suspend_recorder",
        "slash_recorder" => "stoolap_backend_unimplemented:slash_recorder",
        "declare_retirement_eligible" => {
            "stoolap_backend_unimplemented:declare_retirement_eligible"
        }
        _ => "stoolap_backend_unimplemented",
    }))
}

impl ReputationStore for StoolapReputationStore {
    async fn record_signal(&self, _event: SignalEvent) -> StoreResult<EventId> {
        stub("record_signal")
    }

    async fn read_aggregate(
        &self,
        _did: &RecorderDid,
        _kind: SignalKind,
        _layer: ReputationLayer,
    ) -> StoreResult<ReputationAggregate> {
        stub("read_aggregate")
    }

    async fn cross_layer_query(
        &self,
        _did: &RecorderDid,
        _kind: SignalKind,
        _layers: &[ReputationLayer],
    ) -> StoreResult<Vec<ReputationAggregate>> {
        stub("cross_layer_query")
    }

    async fn sliding_window(
        &self,
        _did: &RecorderDid,
        _kind: SignalKind,
        _layer: ReputationLayer,
        _window_secs: u64,
        _now_unix: u64,
    ) -> StoreResult<ReputationAggregate> {
        stub("sliding_window")
    }

    async fn replay_for_audit(
        &self,
        _did: &RecorderDid,
        _since_unix: u64,
        _until_unix: u64,
    ) -> StoreResult<Vec<SignalEvent>> {
        stub("replay_for_audit")
    }

    async fn retention_prune(&self, _cutoff_unix: u64, _now_unix: u64) -> StoreResult<u64> {
        stub("retention_prune")
    }

    async fn prune_event(&self, _event_id: EventId) -> StoreResult<()> {
        stub("prune_event")
    }

    async fn register_recorder(
        &self,
        _chain_ref: crate::auth::ChainRef,
    ) -> StoreResult<crate::types::RecorderId> {
        stub("register_recorder")
    }

    async fn verify_governance_suspension(
        &self,
        _auth: &SuspensionAuth,
        _snapshot: &GovernanceSnapshot,
        _now_unix: u64,
    ) -> StoreResult<()> {
        stub("verify_governance_suspension")
    }

    async fn suspend_recorder(
        &self,
        _recorder_id: crate::types::RecorderId,
        _auth: SuspensionAuth,
        _now_unix: u64,
    ) -> StoreResult<()> {
        stub("suspend_recorder")
    }

    async fn slash_recorder(&self, _proof: GovernanceProof) -> StoreResult<()> {
        stub("slash_recorder")
    }

    async fn declare_retirement_eligible(
        &self,
        _adapter: u8,
        _evidence: ParityEvidence,
        _proof: GovernanceProof,
        _now_unix: u64,
    ) -> StoreResult<RetirementEligibility> {
        stub("declare_retirement_eligible")
    }
}
