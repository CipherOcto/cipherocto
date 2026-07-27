//! `ReputationStore` trait + in-memory + stoolap-fork impls (RFC-0968 §3).
//!
//! Storage abstraction — every method is async because the eventual stoolap
//! backend is async (tokio). The in-memory backend runs synchronously inside
//! `async fn` so tests stay deterministic. Session 4 adds the parity binary
//! which composes both backends.
//!
//! The trait is 18 methods as of Session 8 (mission 0968 Phase 4): the
//! original 12 plus four federation methods
//! (`register_attestor`, `attestor_lookup_did`, `record_attestation`,
//! `query_attestations`) and two quorum / catch-up methods
//! (`attestor_quorum_reached`, `gossip_catch_up`) that own the gossip
//! substrate's read-side (RFC-0968 §12 + amendments 22, 28, 29).

mod memory;
mod stoolap;

pub use memory::InMemoryReputationStore;
pub use stoolap::StoolapReputationStore;

use crate::auth::{
    Attestation, AttestorId, AttestorRegistration, GovernanceProof, GovernanceSnapshot,
    SuspensionAuth,
};
use crate::error::ReputationError;
use crate::gossip::GossipCatchUp;
use crate::types::{
    EventId, ParityEvidence, RecorderDid, RecorderId, ReputationAggregate, ReputationLayer,
    RetirementEligibility, RotationProvenance, SignalEvent, SignalKind,
};

/// Result alias used by every `ReputationStore` method. The store never panics
/// on user input — every domain error maps to a `ReputationError` variant.
pub type StoreResult<T> = Result<T, ReputationError>;

/// The 16-method canonical reputation store contract (RFC-0968 §3 + §12).
///
/// `verify_governance_suspension` accepts `(auth, snapshot, now_unix)` — the
/// post-Round-11 canonical signature. `slash_recorder` carries a
/// `GovernanceProof` whose `slash_destination / slash_amount / slash_asset`
/// fields are validated against the chain-side state before any chain tx.
///
/// The four federation methods (`register_attestor`, `attestor_lookup_did`,
/// `record_attestation`, `query_attestations`) wire the gossip substrate's
/// read-side into the persisted store. Quorum is enforced via
/// `attestor_quorum_reached(event_id)` from the same trait; that method
/// will land in Session 8 alongside the stoolap backend's full federation
/// impl.
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

    // -- Federation: 4 methods added in Session 7 (mission 0968 Phase 4) --

    /// Register an attestor. Idempotent — re-registering the same DID
    /// updates the pubkey + peer_set_id without inserting a new row.
    /// Required before the attestor can sign `Attestation` records.
    async fn register_attestor(
        &self,
        registration: AttestorRegistration,
    ) -> StoreResult<AttestorId>;

    /// Look up an attestor by DID. Returns the registration record or
    /// `RecorderNotRegistered` (reused error variant for the federation
    /// namespace; the type system prevents mixing attestor DIDs into
    /// recorder-side state).
    async fn attestor_lookup_did(
        &self,
        attestor_did: &AttestorId,
    ) -> StoreResult<AttestorRegistration>;

    /// Record one attestation. The store is idempotent on
    /// `(attestor_did, event_id)` composite key: re-recording the same
    /// attestation is a no-op (returns the original `attestation_id`).
    async fn record_attestation(&self, attestation: Attestation) -> StoreResult<u64>;

    /// Query attestations observed for a recorder's events since
    /// `since_event_id` (exclusive). Used by the gossip substrate to
    /// answer `SlashReputationStoreCompat::global_slash_count` reads
    /// without re-deriving them from the events table.
    async fn query_attestations(
        &self,
        recorder_did: &RecorderDid,
        since_event_id: EventId,
    ) -> StoreResult<Vec<Attestation>>;

    /// True iff at least `MIN_ATTESTOR_QUORUM` distinct attestors have
    /// observed this `event_id`. Counts attestations persisted in
    /// `reputation_attestations` (session 3 / mission 0968 Phase 4).
    /// Absence of quorum fails-closed; the gossip substrate rejects
    /// the event from the confirmed-federation set.
    async fn attestor_quorum_reached(&self, event_id: EventId) -> StoreResult<bool>;

    /// Catch-up path for a late-joining attestor. Returns every
    /// `SignalEvent` with `event_id > since_event_id` so the caller
    /// can republish the missing envelopes to its local swarm. The
    /// `attestor_did` in the request identifies the asker (for
    /// rate-limiting on the responder side); the in-memory impl
    /// ignores it because there is no responder queue, but the
    /// stoolap impl records the catch-up in the `reputation_gossip_seen`
    /// ledger.
    async fn gossip_catch_up(&self, catch_up: &GossipCatchUp) -> StoreResult<Vec<SignalEvent>>;
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
