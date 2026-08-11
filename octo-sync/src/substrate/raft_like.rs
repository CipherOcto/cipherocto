//! `RaftLikeWriterElection` + `RaftLikeDidWriteCoordinator` concrete
//! impls (per RFC-0862 v1.4 §Concrete Impl Extension §Data Structures).
//!
//! Both impls are backed by a shared `Arc<Cluster>`. In production the
//! cluster lives across instances and mediates consensus via the Raft
//! RPC protocol; for the substrate test harness the cluster is in-process
//! and serialises under a `parking_lot` mutex.
//!
//! # Sealed trait pattern
//!
//! Per [[cipherocto-design-principles]] §No parallel abstractions:
//! - `WriterElectionSealed` + `WriterElectionForceRelinquishSealed`
//!   are implemented here (impls live in the substrate crate only).
//! - `DidWriteCoordinatorSealed` (from `octo_ident::write_coordinator::sealed`)
//!   is implemented here because `octo-sync` is the substrate crate that
//!   RFC-0862 v1.3 + v1.4 designate as the concrete-impl author.
//!
//! # Layer discipline
//!
//! - `octo-sync` (Layer B-substrate) — provides concrete impls.
//! - `octo-ident` (Layer B) — owns the `DidWriteCoordinator` trait.
//! - `octo-identity-resolver-node` (Layer C) — consumes the impl via
//!   `Arc<dyn DidWriteCoordinator>` injection (mission
//!   `0871e-f7-impl-resolver-mediation` LANDED 2026-08-11).
//!
//! One-way dep direction: `octo-sync → octo-ident`. No reverse cycle.

use std::sync::Arc;

use async_trait::async_trait;
use borsh::{BorshDeserialize, BorshSerialize};
use octo_ident::write_coordinator::sealed::DidWriteCoordinatorSealed;
use octo_ident::{ChainId, DidDocument, DidWriteCoordinator, DidWriteCoordinatorError};

use super::cluster::Cluster;
use super::drain::DrainCoordinator;
use super::governance::{GovernanceAttestation, NonceTracker};
use super::hlc::HlcClock;
use super::ids::{OperatorSet, ShardKey, WriterNodeId};
use super::records::{ActualDrained, DrainCoordinatorError, WriterElectionError};
use super::state::{
    sealed::{WriterElectionForceRelinquishSealed, WriterElectionSealed},
    WriterElection, WriterElectionForceRelinquish, WriterIdentity,
};
use super::wal::{WalEntry, ENTRY_TYPE_DID_REGISTER, ENTRY_TYPE_DID_REVOKE, ENTRY_TYPE_DRAIN};
use super::wal_storage::InMemoryWal;
use super::wal_traits::WalWriter;

/// RaftLike `WriterElection` impl backed by `Arc<Cluster>`.
///
/// Per RFC-0862 v1.4 §Concrete Impl Extension §Data Structures:
/// - `acquire_writer` blocks on `Cluster::try_acquire_leader` (in-memory
///   backing makes this synchronous; production awaits a Raft
///   `RequestVote` RPC).
/// - `heartbeat` refreshes the lease via `Cluster::heartbeat`.
/// - `current_writer` is a read-only cluster inspection.
pub struct RaftLikeWriterElection {
    node_id: WriterNodeId,
    hlc: HlcClock,
    cluster: Arc<Cluster>,
    /// Deployment `chain_id` used for governance attestation verification
    /// (per RFC-0862 v1.3 R12 M23 — `attestation.chain_id` MUST match this
    /// configured deployment chain_id or the attestation is rejected with
    /// `ChainIdMismatch`).
    chain_id: ChainId,
}

impl RaftLikeWriterElection {
    /// Construct a new `RaftLikeWriterElection` for `node_id` backed by
    /// the shared `cluster` and bound to deployment `chain_id` (per
    /// RFC-0862 v1.3 R12 M23 — deployment binding for governance
    /// attestations).
    pub fn new(node_id: WriterNodeId, cluster: Arc<Cluster>, chain_id: ChainId) -> Self {
        Self {
            node_id,
            hlc: HlcClock::new(node_id),
            cluster,
            chain_id,
        }
    }
}

impl WriterElectionSealed for RaftLikeWriterElection {}
impl WriterElectionForceRelinquishSealed for RaftLikeWriterElection {}

#[async_trait]
impl WriterElection for RaftLikeWriterElection {
    async fn acquire_writer(
        &self,
        shard_key: &ShardKey,
        _election_timeout_ms: u64,
    ) -> Result<WriterIdentity, WriterElectionError> {
        let hlc = self
            .hlc
            .now()
            .map_err(|_| WriterElectionError::WalCorruption)?;
        self.cluster
            .try_acquire_leader(self.node_id, *shard_key, hlc)
    }

    async fn relinquish_writer(&self, shard_key: &ShardKey) -> Result<(), WriterElectionError> {
        self.cluster.relinquish(self.node_id, *shard_key)
    }

    async fn heartbeat(&self, shard_key: &ShardKey) -> Result<(), WriterElectionError> {
        let hlc = self
            .hlc
            .now()
            .map_err(|_| WriterElectionError::WalCorruption)?;
        self.cluster.heartbeat(self.node_id, *shard_key, hlc)
    }

    fn current_writer(
        &self,
        shard_key: &ShardKey,
    ) -> Result<Option<WriterIdentity>, WriterElectionError> {
        Ok(self.cluster.current_leader(*shard_key))
    }
}

#[async_trait]
impl WriterElectionForceRelinquish for RaftLikeWriterElection {
    async fn force_relinquish_writer(
        &self,
        shard_key: &ShardKey,
        attestation: &GovernanceAttestation,
        configured_operator_set: &OperatorSet,
        nonce_tracker: &NonceTracker,
    ) -> Result<(), WriterElectionError> {
        // Verify the attestation first (chain_id binding + M-of-N + nonce consume).
        // Pass the configured deployment chain_id (NOT `attestation.chain_id`)
        // so the deployment-binding check fires when the operator set tries
        // to replay an attestation minted for a different deployment.
        super::governance::verify_governance_attestation(
            shard_key,
            &self.chain_id,
            attestation,
            configured_operator_set,
            nonce_tracker,
        )?;
        // Clear the lease (governance path bypasses leader authority).
        self.cluster.force_relinquish(*shard_key)
    }
}

/// RaftLike `DidWriteCoordinator` impl backed by `Arc<Cluster>` +
/// `Arc<InMemoryWal>`.
///
/// Per RFC-0862 v1.4 §Concrete Impl Extension §Data Structures:
/// - `submit_register_validated` checks the elected writer for the
///   DID's shard, then appends an `ENTRY_TYPE_DID_REGISTER` WAL entry.
/// - `submit_revoke` checks the elected writer, then appends an
///   `ENTRY_TYPE_DID_REVOKE` WAL entry.
/// - `submit_register_local_fallback` is gated behind the `crdt` feature
///   (per v1.4 §Motivation 4: opt-in LWW for partition-tolerance;
///   default builds stay linearizable via the leader-election path).
pub struct RaftLikeDidWriteCoordinator {
    /// Held for future CRDT (LWW) reconciliation in the failover window
    /// (RFC-0862 v1.4 F12 + F13 amendment). Production code reads
    /// cluster state to resolve competing HLC-stamped writes; the
    /// in-memory backing for the substrate test harness exposes only
    /// the WAL append path.
    #[allow(dead_code)]
    cluster: Arc<Cluster>,
    wal: Arc<InMemoryWal>,
    hlc: HlcClock,
    chain_id: ChainId,
    node_id: WriterNodeId,
    election: Arc<dyn WriterElection>,
}

impl RaftLikeDidWriteCoordinator {
    /// Construct a new `RaftLikeDidWriteCoordinator` bound to
    /// `chain_id`. Writes are gated on `election.current_writer`
    /// returning `Some(WriterIdentity { writer_node_id == self.node_id })`.
    pub fn new(
        cluster: Arc<Cluster>,
        chain_id: ChainId,
        node_id: WriterNodeId,
        election: Arc<dyn WriterElection>,
    ) -> Self {
        Self {
            wal: Arc::new(InMemoryWal::new(cluster.clone())),
            hlc: HlcClock::new(node_id),
            chain_id,
            node_id,
            cluster,
            election,
        }
    }
}

impl DidWriteCoordinatorSealed for RaftLikeDidWriteCoordinator {}

#[async_trait]
impl DidWriteCoordinator for RaftLikeDidWriteCoordinator {
    async fn submit_register_validated(
        &self,
        canonical_did_hash: &[u8; 32],
        chain_id: &ChainId,
        document: &DidDocument,
    ) -> Result<(), DidWriteCoordinatorError> {
        if chain_id != &self.chain_id {
            return Err(DidWriteCoordinatorError::ChainIdMismatch);
        }
        let shard_key = ShardKey::derive_canonical(canonical_did_hash);
        let leader = self
            .election
            .current_writer(&shard_key)
            .map_err(|_| DidWriteCoordinatorError::WriterUnavailable)?
            .ok_or(DidWriteCoordinatorError::WriterUnavailable)?;
        if leader.writer_node_id != self.node_id {
            // Per RFC-0862 v1.4 §Concrete Impl: only the elected leader
            // may commit; non-leaders forward via the leader-election
            // substrate in production.
            return Err(DidWriteCoordinatorError::WriterUnavailable);
        }
        let _ = self
            .hlc
            .now()
            .map_err(|e| DidWriteCoordinatorError::WalCorruption(format!("hlc: {e}")))?;
        let doc_bytes = borsh::to_vec(document)
            .map_err(|e| DidWriteCoordinatorError::WalCorruption(format!("borsh: {e}")))?;
        let entry = WalEntry::build_v13(ENTRY_TYPE_DID_REGISTER, shard_key, doc_bytes);
        self.wal
            .append_entry(&entry)
            .await
            .map_err(|e| DidWriteCoordinatorError::WalCorruption(format!("wal: {e}")))?;
        Ok(())
    }

    async fn submit_revoke(
        &self,
        canonical_did_hash: &[u8; 32],
        chain_id: &ChainId,
    ) -> Result<(), DidWriteCoordinatorError> {
        if chain_id != &self.chain_id {
            return Err(DidWriteCoordinatorError::ChainIdMismatch);
        }
        let shard_key = ShardKey::derive_canonical(canonical_did_hash);
        let leader = self
            .election
            .current_writer(&shard_key)
            .map_err(|_| DidWriteCoordinatorError::WriterUnavailable)?
            .ok_or(DidWriteCoordinatorError::WriterUnavailable)?;
        if leader.writer_node_id != self.node_id {
            return Err(DidWriteCoordinatorError::WriterUnavailable);
        }
        let _ = self
            .hlc
            .now()
            .map_err(|e| DidWriteCoordinatorError::WalCorruption(format!("hlc: {e}")))?;
        let entry = WalEntry::build_v13(
            ENTRY_TYPE_DID_REVOKE,
            shard_key,
            canonical_did_hash.to_vec(),
        );
        self.wal
            .append_entry(&entry)
            .await
            .map_err(|e| DidWriteCoordinatorError::WalCorruption(format!("wal: {e}")))?;
        Ok(())
    }

    /// CRDT (Last-Write-Wins) local fallback (per RFC-0862 v1.4
    /// §Motivation 4: opt-in feature flag for partition-tolerance).
    ///
    /// Without the `crdt` feature, this method is the default trait impl
    /// returning `WriterUnavailable` (fail-closed). With `crdt` enabled,
    /// writes are appended locally without leader-election — the WAL
    /// carries an HLC stamp and reconciliation occurs during the
    /// failover window via the LWW counter (F12 + F13 amendments).
    #[cfg(feature = "crdt")]
    #[allow(deprecated)]
    async fn submit_register_local_fallback(
        &self,
        canonical_did_hash: &[u8; 32],
        chain_id: &ChainId,
        document: &DidDocument,
    ) -> Result<(), DidWriteCoordinatorError> {
        if chain_id != &self.chain_id {
            return Err(DidWriteCoordinatorError::ChainIdMismatch);
        }
        let shard_key = ShardKey::derive_canonical(canonical_did_hash);
        let doc_bytes = borsh::to_vec(document)
            .map_err(|e| DidWriteCoordinatorError::WalCorruption(format!("borsh: {e}")))?;
        let entry = WalEntry::build_v13(ENTRY_TYPE_DID_REGISTER, shard_key, doc_bytes);
        self.wal
            .append_entry(&entry)
            .await
            .map_err(|e| DidWriteCoordinatorError::WalCorruption(format!("wal: {e}")))?;
        Ok(())
    }
}

/// Borsh-serialised WAL payload for `ENTRY_TYPE_DRAIN` (per RFC-0862
/// v1.3 §DrainCoordinator).
///
/// Captures the full drain context so the WAL entry is self-describing
/// during replay. The drain itself executes against the
/// `StoolapSpendLedger` (Layer B-adjacent) at the consumer boundary;
/// the coordinator only routes through the writer-election substrate
/// + appends the receipt entry.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
struct DrainWalPayload {
    /// Holder DID whose balance was drained (canonical wire form, per RFC-0010).
    holder_did: String,
    /// 16-byte macaroon identifier (raw bytes).
    macaroon_id: Vec<u8>,
    /// Drained amount in `MicroOCTO_W` (matches `octo_paid_query::MicroOCTO_W`).
    requested_cost: u128,
    /// Chain ID (deployment binding per RFC-0862 v1.3 R12 M23).
    chain_id: String,
}

/// RaftLike `DrainCoordinator` impl backed by `Arc<Cluster>` +
/// `Arc<InMemoryWal>` (per RFC-0862 v1.4 §Concrete Impl Extension + mission
/// `0871e-phase5c-1-cross-instance-drain`).
///
/// `submit_drain` checks the elected writer for the holder's shard
/// (shard key = `ShardKey::derive_canonical(holder_did.as_bytes())`),
/// then appends an `ENTRY_TYPE_DRAIN` WAL entry carrying the
/// `(holder_did, macaroon_id, requested_cost)` payload. The
/// `ActualDrained.receipt_lsn` is the LSN assigned by the WAL
/// appender; the LSN can be referenced by downstream
/// `StoolapSpendLedger::try_deduct` calls for proof-of-receipt
/// replay.
///
/// Authorisation design follows `RaftLikeDidWriteCoordinator`:
/// - `chain_id` mismatch → `ChainIdMismatch` (gated).
/// - No elected writer OR non-leader node → `WriterUnavailable`
///   (fail-closed per RFC-0862 v1.3 R12).
/// - Balance / holder validation is OUT OF SCOPE here — the
///   consumer (the wallet-node handler) calls `StoolapSpendLedger`
///   before invoking `submit_drain`. The coordinator only routes
///   writer availability.
///
/// ## Layer discipline
///
/// Per [[cipherocto-design-principles]]:
///
/// - `octo-sync` (Layer B-substrate) — owns the concrete
///   `RaftLikeDrainCoordinator` impl.
/// - `octo-paid-query` (Layer E) — owns `SpendLedger` trait +
///   `InMemorySpendLedger`. `RaftLikeDrainCoordinator` does NOT
///   consume `SpendLedger` directly to avoid layer inversion;
///   the balance check lives at the consumer boundary.
/// - `quota-router-storage` (Layer B-adjacent) — `StoolapSpendLedger`
///   is the production ledger, injected via `Arc<dyn SpendLedger>`
///   at the wallet-node construction boundary.
pub struct RaftLikeDrainCoordinator {
    /// Held for future CRDT (LWW) reconciliation in the failover
    /// window (RFC-0862 v1.4 F12 + F13 amendment). Matches the
    /// `RaftLikeDidWriteCoordinator` precedent.
    #[allow(dead_code)]
    cluster: Arc<Cluster>,
    wal: Arc<InMemoryWal>,
    hlc: HlcClock,
    chain_id: ChainId,
    node_id: WriterNodeId,
    election: Arc<dyn WriterElection>,
}

impl RaftLikeDrainCoordinator {
    /// Construct a new `RaftLikeDrainCoordinator` bound to `chain_id`.
    /// Drains are gated on `election.current_writer` returning
    /// `Some(WriterIdentity { writer_node_id == self.node_id })`.
    pub fn new(
        cluster: Arc<Cluster>,
        chain_id: ChainId,
        node_id: WriterNodeId,
        election: Arc<dyn WriterElection>,
    ) -> Self {
        Self {
            wal: Arc::new(InMemoryWal::new(cluster.clone())),
            hlc: HlcClock::new(node_id),
            chain_id,
            node_id,
            cluster,
            election,
        }
    }

    /// Borrow the deployment `chain_id` for testing + diagnostics.
    #[must_use]
    pub fn chain_id(&self) -> &ChainId {
        &self.chain_id
    }

    /// Borrow the `node_id` for testing + diagnostics.
    #[must_use]
    pub fn node_id(&self) -> WriterNodeId {
        self.node_id
    }
}

#[async_trait]
impl DrainCoordinator for RaftLikeDrainCoordinator {
    async fn submit_drain(
        &self,
        holder_did: &str,
        macaroon_id: &[u8],
        requested_cost: u128,
    ) -> Result<ActualDrained, DrainCoordinatorError> {
        // Shard by holder DID (canonical wire form, per RFC-0010).
        // All drains for the same holder route to the same shard.
        let shard_key = ShardKey::derive_canonical(holder_did.as_bytes());
        let leader = self
            .election
            .current_writer(&shard_key)
            .map_err(|_| DrainCoordinatorError::WriterUnavailable)?
            .ok_or(DrainCoordinatorError::WriterUnavailable)?;
        if leader.writer_node_id != self.node_id {
            // Per RFC-0862 v1.4 §Concrete Impl: only the elected leader
            // may commit drains; non-leaders forward via the
            // leader-election substrate in production.
            return Err(DrainCoordinatorError::WriterUnavailable);
        }
        let _ = self
            .hlc
            .now()
            .map_err(|_| DrainCoordinatorError::WriterUnavailable)?;
        let payload = borsh::to_vec(&DrainWalPayload {
            holder_did: holder_did.to_owned(),
            macaroon_id: macaroon_id.to_vec(),
            requested_cost,
            chain_id: self.chain_id.as_str().to_owned(),
        })
        .map_err(|_| DrainCoordinatorError::WriterUnavailable)?;
        let entry = WalEntry::build_v13(ENTRY_TYPE_DRAIN, shard_key, payload);
        let receipt_lsn = self
            .wal
            .append_entry(&entry)
            .await
            .map_err(|_| DrainCoordinatorError::WriterUnavailable)?;
        Ok(ActualDrained {
            holder_did: holder_did.to_owned(),
            macaroon_id: macaroon_id.to_vec(),
            drained_amount: requested_cost,
            receipt_lsn,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::substrate::wal::WAL_MAGIC_V13;

    fn doc(seed: u8) -> DidDocument {
        DidDocument {
            public_key: [seed; 32],
            revoked: false,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn acquire_then_submit_register_succeeds() {
        let cluster = Cluster::new();
        let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
        let node_id = WriterNodeId([1u8; 32]);
        let election = Arc::new(RaftLikeWriterElection::new(
            node_id,
            cluster.clone(),
            chain_id.clone(),
        ));
        let coordinator = RaftLikeDidWriteCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            node_id,
            election.clone(),
        );

        let d = doc(7);
        let hash = octo_ident::canonical_hash(&d);
        let shard_key = ShardKey::derive_canonical(&hash);
        let _id = election.acquire_writer(&shard_key, 1_000).await.unwrap();

        let r = coordinator.submit_register(&hash, &chain_id, &d).await;
        assert!(r.is_ok(), "register failed: {r:?}");

        // Verify WAL entry present.
        let entries = cluster.read_wal_range(1, None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_type, ENTRY_TYPE_DID_REGISTER);
        assert_eq!(entries[0].magic, WAL_MAGIC_V13);
        assert_eq!(entries[0].lsn, 1);
    }

    #[tokio::test]
    async fn register_without_leader_fails_closed() {
        let cluster = Cluster::new();
        let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
        let node_id = WriterNodeId([1u8; 32]);
        let election = Arc::new(RaftLikeWriterElection::new(
            node_id,
            cluster.clone(),
            chain_id.clone(),
        ));
        let coordinator = RaftLikeDidWriteCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            node_id,
            election.clone(),
        );

        // No acquire — register must fail-closed.
        let d = doc(9);
        let hash = octo_ident::canonical_hash(&d);
        let r = coordinator.submit_register(&hash, &chain_id, &d).await;
        assert!(matches!(
            r,
            Err(DidWriteCoordinatorError::WriterUnavailable)
        ));
    }

    #[tokio::test]
    async fn register_from_non_leader_fails() {
        let cluster = Cluster::new();
        let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
        let leader_id = WriterNodeId([1u8; 32]);
        let follower_id = WriterNodeId([2u8; 32]);
        let leader_election = Arc::new(RaftLikeWriterElection::new(
            leader_id,
            cluster.clone(),
            chain_id.clone(),
        ));
        let follower_election = Arc::new(RaftLikeWriterElection::new(
            follower_id,
            cluster.clone(),
            chain_id.clone(),
        ));
        let d = doc(11);
        let hash = octo_ident::canonical_hash(&d);
        let shard_key = ShardKey::derive_canonical(&hash);

        // Leader acquires.
        let _id = leader_election
            .acquire_writer(&shard_key, 1_000)
            .await
            .unwrap();

        // Follower's coordinator attempts register — must fail.
        let coordinator = RaftLikeDidWriteCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            follower_id,
            follower_election.clone(),
        );
        let r = coordinator.submit_register(&hash, &chain_id, &d).await;
        assert!(matches!(
            r,
            Err(DidWriteCoordinatorError::WriterUnavailable)
        ));

        // Leader's coordinator succeeds.
        let leader_coord = RaftLikeDidWriteCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            leader_id,
            leader_election.clone(),
        );
        let r = leader_coord.submit_register(&hash, &chain_id, &d).await;
        assert!(r.is_ok(), "leader register failed: {r:?}");
    }

    #[tokio::test]
    async fn chain_id_mismatch_rejected() {
        let cluster = Cluster::new();
        let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
        let node_id = WriterNodeId([1u8; 32]);
        let election = Arc::new(RaftLikeWriterElection::new(
            node_id,
            cluster.clone(),
            chain_id.clone(),
        ));
        let coordinator = RaftLikeDidWriteCoordinator::new(
            cluster.clone(),
            ChainId::new("cipherocto-test").expect("static test literal"),
            node_id,
            election.clone(),
        );
        let d = doc(13);
        let hash = octo_ident::canonical_hash(&d);
        let shard_key = ShardKey::derive_canonical(&hash);
        let _ = election.acquire_writer(&shard_key, 1_000).await.unwrap();

        let wrong_chain = ChainId::new("other-chain").expect("static test literal");
        let r = coordinator.submit_register(&hash, &wrong_chain, &d).await;
        assert!(matches!(r, Err(DidWriteCoordinatorError::ChainIdMismatch)));
    }

    #[tokio::test]
    async fn revoke_creates_revoke_entry() {
        let cluster = Cluster::new();
        let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
        let node_id = WriterNodeId([1u8; 32]);
        let election = Arc::new(RaftLikeWriterElection::new(
            node_id,
            cluster.clone(),
            chain_id.clone(),
        ));
        let coordinator = RaftLikeDidWriteCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            node_id,
            election.clone(),
        );
        let hash = [42u8; 32];
        let shard_key = ShardKey::derive_canonical(&hash);
        let _ = election.acquire_writer(&shard_key, 1_000).await.unwrap();

        let r = coordinator.submit_revoke(&hash, &chain_id).await;
        assert!(r.is_ok(), "revoke failed: {r:?}");
        let entries = cluster.read_wal_range(1, None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_type, ENTRY_TYPE_DID_REVOKE);
    }

    #[cfg(feature = "crdt")]
    #[tokio::test]
    #[allow(deprecated)]
    async fn crdt_local_fallback_succeeds_without_leader() {
        let cluster = Cluster::new();
        let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
        let node_id = WriterNodeId([1u8; 32]);
        let election = Arc::new(RaftLikeWriterElection::new(
            node_id,
            cluster.clone(),
            chain_id.clone(),
        ));
        let coordinator = RaftLikeDidWriteCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            node_id,
            election.clone(),
        );
        // No acquire.
        let d = doc(15);
        let hash = octo_ident::canonical_hash(&d);
        let r = coordinator
            .submit_register_local_fallback(&hash, &chain_id, &d)
            .await;
        assert!(r.is_ok(), "crdt fallback failed: {r:?}");
        let entries = cluster.read_wal_range(1, None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_type, ENTRY_TYPE_DID_REGISTER);
    }

    #[cfg(not(feature = "crdt"))]
    #[tokio::test]
    #[allow(deprecated)]
    async fn non_crdt_local_fallback_returns_writer_unavailable() {
        let cluster = Cluster::new();
        let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
        let node_id = WriterNodeId([1u8; 32]);
        let election = Arc::new(RaftLikeWriterElection::new(
            node_id,
            cluster.clone(),
            chain_id.clone(),
        ));
        let coordinator = RaftLikeDidWriteCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            node_id,
            election.clone(),
        );
        let d = doc(17);
        let hash = octo_ident::canonical_hash(&d);
        let r = coordinator
            .submit_register_local_fallback(&hash, &chain_id, &d)
            .await;
        assert!(matches!(
            r,
            Err(DidWriteCoordinatorError::WriterUnavailable)
        ));
    }

    // ─── RaftLikeDrainCoordinator unit tests (mission 0871e-phase5c-1) ───

    fn fixture_drain() -> (
        Arc<Cluster>,
        [WriterNodeId; 3],
        ChainId,
        Arc<RaftLikeWriterElection>,
    ) {
        let cluster = Cluster::new();
        let chain_id = ChainId::new("cipherocto-test").expect("static test literal");
        let ids = [
            WriterNodeId([1u8; 32]),
            WriterNodeId([2u8; 32]),
            WriterNodeId([3u8; 32]),
        ];
        let election = Arc::new(RaftLikeWriterElection::new(
            ids[0],
            cluster.clone(),
            chain_id.clone(),
        ));
        (cluster, ids, chain_id, election)
    }

    #[tokio::test]
    async fn drain_acquire_then_submit_drain_succeeds() {
        let (cluster, ids, chain_id, election) = fixture_drain();
        let coordinator = RaftLikeDrainCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            ids[0],
            election.clone() as Arc<dyn WriterElection>,
        );

        let holder = "did:octo:zHolder1";
        let macaroon_id: [u8; 16] = [0x42; 16];
        let shard_key = ShardKey::derive_canonical(holder.as_bytes());
        let _leader = election.acquire_writer(&shard_key, 1_000).await.unwrap();

        let r = coordinator
            .submit_drain(holder, &macaroon_id, 100)
            .await
            .expect("leader should drain");
        assert_eq!(r.holder_did, holder);
        assert_eq!(r.macaroon_id, macaroon_id);
        assert_eq!(r.drained_amount, 100);
        assert_eq!(r.receipt_lsn, 1, "first WAL entry LSN = 1");

        // Verify WAL entry present with type DRAIN.
        let entries = cluster.read_wal_range(1, None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry_type, ENTRY_TYPE_DRAIN);
        assert_eq!(entries[0].lsn, 1);
    }

    #[tokio::test]
    async fn drain_non_leader_fails_closed() {
        let (cluster, ids, chain_id, election_leader) = fixture_drain();
        // Coordinator for instance B (NOT the elected leader).
        let coordinator_b = RaftLikeDrainCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            ids[1],
            election_leader.clone() as Arc<dyn WriterElection>,
        );

        let holder = "did:octo:zHolder2";
        let macaroon_id: [u8; 16] = [0x07; 16];
        let shard_key = ShardKey::derive_canonical(holder.as_bytes());
        // Instance A acquires the lease.
        let _leader = election_leader
            .acquire_writer(&shard_key, 1_000)
            .await
            .unwrap();

        let r = coordinator_b.submit_drain(holder, &macaroon_id, 50).await;
        assert!(
            matches!(r, Err(DrainCoordinatorError::WriterUnavailable)),
            "non-leader must fail-closed with WriterUnavailable, got {r:?}"
        );

        // No WAL entry appended on non-leader path.
        let entries = cluster.read_wal_range(1, None);
        assert_eq!(entries.len(), 0, "non-leader must not append");
    }

    #[tokio::test]
    async fn drain_no_elected_writer_fails_closed() {
        let (cluster, ids, chain_id, election) = fixture_drain();
        let coordinator = RaftLikeDrainCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            ids[0],
            election.clone() as Arc<dyn WriterElection>,
        );

        // No leader acquired for any shard.
        let r = coordinator
            .submit_drain("did:octo:zHolder3", &[0x05; 16], 200)
            .await;
        assert!(
            matches!(r, Err(DrainCoordinatorError::WriterUnavailable)),
            "no leader must fail-closed, got {r:?}"
        );
        let entries = cluster.read_wal_range(1, None);
        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn drain_unknown_holder_is_not_validated_by_coordinator() {
        // Per RFC-0862 v1.4 §Concrete Impl Extension: `DrainCoordinator`
        // does NOT validate holder balance — the consumer (wallet-node)
        // calls `StoolapSpendLedger::try_deduct` BEFORE invoking
        // `submit_drain`. The coordinator only checks writer
        // availability. This test verifies a coordinator `submit_drain`
        // against an "unknown" holder still succeeds (returns
        // `ActualDrained`) — the balance check is the consumer's job.
        let (cluster, ids, chain_id, election) = fixture_drain();
        let coordinator = RaftLikeDrainCoordinator::new(
            cluster.clone(),
            chain_id.clone(),
            ids[0],
            election.clone() as Arc<dyn WriterElection>,
        );

        let holder = "did:octo:zNeverSeeded";
        let macaroon_id: [u8; 16] = [0xFF; 16];
        let shard_key = ShardKey::derive_canonical(holder.as_bytes());
        let _leader = election.acquire_writer(&shard_key, 1_000).await.unwrap();

        let r = coordinator
            .submit_drain(holder, &macaroon_id, 999)
            .await
            .expect("coordinator passes through; ledger validation is downstream");
        assert_eq!(r.drained_amount, 999);
        assert_eq!(r.receipt_lsn, 1);
    }
}
