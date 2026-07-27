//! `ReputationStore` trait + in-memory + stoolap-fork impls (RFC-0968 §3).
//!
//! Storage abstraction — every method is async because the eventual stoolap
//! backend is async (tokio). The in-memory backend runs synchronously inside
//! `async fn` so tests stay deterministic. Session 4 adds the parity binary
//! which composes both backends.

mod memory;
mod stoolap;

pub use memory::InMemoryReputationStore;
pub use stoolap::StoolapReputationStore;

use crate::auth::{GovernanceProof, GovernanceSnapshot, SuspensionAuth};
use crate::error::ReputationError;
use crate::types::{
    EventId, ParityEvidence, RecorderDid, RecorderId, ReputationAggregate, ReputationLayer,
    RetirementEligibility, RotationProvenance, SignalEvent, SignalKind,
};

/// Result alias used by every `ReputationStore` method. The store never panics
/// on user input — every domain error maps to a `ReputationError` variant.
pub type StoreResult<T> = Result<T, ReputationError>;

/// The 12-method canonical reputation store contract (RFC-0968 §3).
///
/// `verify_governance_suspension` accepts `(auth, snapshot, now_unix)` — the
/// post-Round-11 canonical signature. `slash_recorder` carries a
/// `GovernanceProof` whose `slash_destination / slash_amount / slash_asset`
/// fields are validated against the chain-side state before any chain tx.
#[allow(async_fn_in_trait)]
pub trait ReputationStore: Send + Sync {
    /// Persist one signal event. Storage layer assigns `event_id`.
    async fn record_signal(&self, event: SignalEvent) -> StoreResult<EventId>;

    /// Read the aggregate for `(did, kind, layer)`.
    async fn read_aggregate(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layer: ReputationLayer,
    ) -> StoreResult<ReputationAggregate>;

    /// Read the aggregate across multiple layers in one call.
    async fn cross_layer_query(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layers: &[ReputationLayer],
    ) -> StoreResult<Vec<ReputationAggregate>>;

    /// Sliding-window aggregate over the last `window_secs`.
    async fn sliding_window(
        &self,
        did: &RecorderDid,
        kind: SignalKind,
        layer: ReputationLayer,
        window_secs: u64,
        now_unix: u64,
    ) -> StoreResult<ReputationAggregate>;

    /// Replay all events for a DID in `[since_unix, until_unix]`.
    async fn replay_for_audit(
        &self,
        did: &RecorderDid,
        since_unix: u64,
        until_unix: u64,
    ) -> StoreResult<Vec<SignalEvent>>;

    /// Delete events with `recorded_at_unix <= cutoff_unix`. Returns count.
    async fn retention_prune(&self, cutoff_unix: u64, now_unix: u64) -> StoreResult<u64>;

    /// Delete one event by id.
    async fn prune_event(&self, event_id: EventId) -> StoreResult<()>;

    /// Register a recorder via ChainRef. Performs 8-field chain verification
    /// + 3-guard stake check (octo / role / aggregate).
    async fn register_recorder(&self, chain_ref: crate::auth::ChainRef) -> StoreResult<RecorderId>;

    /// Verify a suspension authorisation against the on-chain governance
    /// snapshot. Real signature verification is deferred; this method
    /// validates shape, freshness, and quorum.
    async fn verify_governance_suspension(
        &self,
        auth: &SuspensionAuth,
        snapshot: &GovernanceSnapshot,
        now_unix: u64,
    ) -> StoreResult<()>;

    /// Suspend a recorder (read-side). Requires `verify_governance_suspension`
    /// to have returned `Ok` for the same auth.
    async fn suspend_recorder(
        &self,
        recorder_id: RecorderId,
        auth: SuspensionAuth,
        now_unix: u64,
    ) -> StoreResult<()>;

    /// Slash a recorder. Validates `slash_destination / slash_amount /
    /// slash_asset` field consistency before any chain tx.
    async fn slash_recorder(&self, proof: GovernanceProof) -> StoreResult<()>;

    /// Declare a recorder eligible for retirement. Stubbed governance proof
    /// check; returns `RetirementEligibility` with the recorded evidence hash.
    async fn declare_retirement_eligible(
        &self,
        adapter: u8,
        evidence: ParityEvidence,
        proof: GovernanceProof,
        now_unix: u64,
    ) -> StoreResult<RetirementEligibility>;
}

/// Helper used by every backend — convert a `RotationProvenance` into a
/// stable byte string for index keys. Tombstoned-DID replay tests rely on
/// this being deterministic.
pub fn rotation_key(rp: &RotationProvenance) -> Vec<u8> {
    let mut buf = Vec::with_capacity(52 + 8 + 8);
    buf.extend_from_slice(rp.new_did.as_bytes());
    buf.extend_from_slice(&rp.consumed_at_unix.to_be_bytes());
    buf.extend_from_slice(&rp.rotation_id.to_be_bytes());
    buf
}
