//! Cross-instance cluster state substrate (per RFC-0862 v1.3 §Cluster
//! substrate + v1.4 §Concrete Impl Extension).
//!
//! In production, `RaftLikeWriterElection` + `RaftLikeDidWriteCoordinator`
//! coordinate via the cluster consensus protocol (one network round-trip
//! per Raft append / election RPC). For the substrate test harness
//! (mission `0871e-f7-coordinator-impl` task #122 + cross-instance TV),
//! a single `Arc<Cluster>` is shared across all instances and provides
//! the same ordering guarantees synchronously under a `parking_lot` mutex.
//!
//! # Why a cluster substrate (not parallel consensus code)
//!
//! Per [[cipherocto-design-principles]] §Stable Abstractions Principle:
//! the writer-election + DID-write coordinator surfaces are the
//! abstractions; the consensus mechanism is the implementation detail.
//! Adding CRDT (LWW) per v1.4 F12 + F13 amendment swaps the consensus
//! layer without changing the trait surface — the substrate trait objects
//! stay identical.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use super::hlc::HlcTimestamp;
use super::ids::{ShardKey, ShardMissionId, WriterNodeId};
use super::records::{NonceRecord, WriterElectionError};
use super::state::WriterIdentity;
use super::wal::WalEntry;

/// Default lease duration in milliseconds (per RFC-0862 v1.3 §WriterElection
/// Protocol §Lease TTL). 5 seconds balances failover latency vs spurious
/// lease expiry under transient packet loss.
pub const DEFAULT_LEASE_DURATION_MS: u64 = 5_000;

/// Shared cluster state backing `RaftLikeWriterElection` +
/// `RaftLikeDidWriteCoordinator` in the test harness.
///
/// In production this state lives across instances; here it lives in a
/// single `Arc<Cluster>` for synchronous test access.
#[derive(Debug)]
pub struct ClusterState {
    /// Current leader per shard (None = no leader).
    leaders: HashMap<ShardKey, WriterNodeId>,
    /// Election term per shard (monotonic per shard_key).
    terms: HashMap<ShardKey, u64>,
    /// Last heartbeat physical_ms per shard (used for lease-expiry check).
    last_heartbeat_ms: HashMap<ShardKey, u64>,
    /// Lease duration in milliseconds (test-overridable).
    lease_duration_ms: u64,
    /// Nodes marked dead (acquire_writer fails fast on these).
    dead_writers: Vec<WriterNodeId>,
    /// Append-only WAL (shared across all instances).
    wal: Vec<WalEntry>,
    /// Nonce records (entry type 0x10) — used by `NonceTracker` replay.
    nonces: Vec<NonceRecord>,
}

impl Default for ClusterState {
    fn default() -> Self {
        Self {
            leaders: HashMap::new(),
            terms: HashMap::new(),
            last_heartbeat_ms: HashMap::new(),
            lease_duration_ms: DEFAULT_LEASE_DURATION_MS,
            dead_writers: Vec::new(),
            wal: Vec::new(),
            nonces: Vec::new(),
        }
    }
}

/// Cross-instance cluster substrate (per RFC-0862 v1.4 §Concrete Impl
/// Extension §Data Structures).
///
/// `Arc<Cluster>` is shared across all instances. All methods take
/// `&self`; the internal `parking_lot::Mutex<ClusterState>` serialises
/// access. Methods never block on async I/O — the in-memory backing
/// makes them all O(1) (modulo WAL append).
#[derive(Debug, Default)]
pub struct Cluster {
    state: Mutex<ClusterState>,
}

impl Cluster {
    /// Construct a new cluster with default lease duration.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Override the lease duration (used by tests to exercise failover).
    pub fn set_lease_duration_ms(&self, ms: u64) {
        self.state.lock().lease_duration_ms = ms;
    }

    /// Mark a writer as dead. `acquire_writer` and `heartbeat` from this
    /// node fail fast with `WriterUnavailable` until `revive` is called.
    pub fn kill(&self, node_id: WriterNodeId) {
        self.state.lock().dead_writers.push(node_id);
    }

    /// Revive a previously-killed writer.
    pub fn revive(&self, node_id: WriterNodeId) {
        self.state.lock().dead_writers.retain(|n| n != &node_id);
    }

    /// Try to acquire the writer lease for `shard_key` on behalf of
    /// `node_id`. Returns `WriterIdentity` on success; `WriterUnavailable`
    /// if the lease is held by another node and has not yet expired.
    pub fn try_acquire_leader(
        &self,
        node_id: WriterNodeId,
        shard_key: ShardKey,
        hlc: HlcTimestamp,
    ) -> Result<WriterIdentity, WriterElectionError> {
        let mut state = self.state.lock();
        if state.dead_writers.contains(&node_id) {
            return Err(WriterElectionError::WriterUnavailable);
        }
        let current_term = state.terms.get(&shard_key).copied().unwrap_or(0);
        let new_term = current_term + 1;
        if let Some(leader) = state.leaders.get(&shard_key).copied() {
            if leader != node_id {
                let now_ms = hlc.physical_ms;
                let last_hb = state
                    .last_heartbeat_ms
                    .get(&shard_key)
                    .copied()
                    .unwrap_or(0);
                if now_ms.saturating_sub(last_hb) < state.lease_duration_ms {
                    return Err(WriterElectionError::WriterUnavailable);
                }
            }
        }
        state.leaders.insert(shard_key, node_id);
        state.terms.insert(shard_key, new_term);
        state.last_heartbeat_ms.insert(shard_key, hlc.physical_ms);
        Ok(WriterIdentity {
            writer_node_id: node_id,
            mission_id: ShardMissionId([0u8; 32]),
            term: new_term,
            elected_at_hlc: hlc,
            shard_key,
        })
    }

    /// Relinquish the writer lease for `shard_key`. Idempotent; returns
    /// `Ok(())` if no lease was held.
    pub fn relinquish(
        &self,
        node_id: WriterNodeId,
        shard_key: ShardKey,
    ) -> Result<(), WriterElectionError> {
        let mut state = self.state.lock();
        if state.leaders.get(&shard_key) == Some(&node_id) {
            state.leaders.remove(&shard_key);
            state.last_heartbeat_ms.remove(&shard_key);
        }
        Ok(())
    }

    /// Refresh the lease for `shard_key`. Returns `LeaseExpired` if the
    /// lease is no longer held by `node_id` (or has been force-relinquished).
    pub fn heartbeat(
        &self,
        node_id: WriterNodeId,
        shard_key: ShardKey,
        hlc: HlcTimestamp,
    ) -> Result<(), WriterElectionError> {
        let mut state = self.state.lock();
        if state.dead_writers.contains(&node_id) {
            return Err(WriterElectionError::WriterUnavailable);
        }
        if state.leaders.get(&shard_key) != Some(&node_id) {
            return Err(WriterElectionError::LeaseExpired);
        }
        state.last_heartbeat_ms.insert(shard_key, hlc.physical_ms);
        Ok(())
    }

    /// Read the current leader for `shard_key` without acquiring.
    pub fn current_leader(&self, shard_key: ShardKey) -> Option<WriterIdentity> {
        let state = self.state.lock();
        let wid = state.leaders.get(&shard_key).copied()?;
        let term = state.terms.get(&shard_key).copied()?;
        let physical_ms = state.last_heartbeat_ms.get(&shard_key).copied()?;
        Some(WriterIdentity {
            writer_node_id: wid,
            mission_id: ShardMissionId([0u8; 32]),
            term,
            elected_at_hlc: HlcTimestamp {
                physical_ms,
                logical: 0,
                writer_node_id: wid,
            },
            shard_key,
        })
    }

    /// Force-relinquish the lease for `shard_key`. Used by the
    /// governance attestation path after M-of-N threshold verification.
    pub fn force_relinquish(&self, shard_key: ShardKey) -> Result<(), WriterElectionError> {
        let mut state = self.state.lock();
        state.leaders.remove(&shard_key);
        state.last_heartbeat_ms.remove(&shard_key);
        Ok(())
    }

    /// Append a v1.3 WAL entry. Sets the LSN chain fields + recomputes
    /// the checksum on the caller's entry, then stores it. Returns the
    /// assigned LSN.
    pub fn append_wal_entry(&self, mut entry: WalEntry) -> Result<u64, WriterElectionError> {
        let mut state = self.state.lock();
        let lsn = state.wal.len() as u64 + 1;
        let previous_lsn = state.wal.last().map(|e| e.lsn).unwrap_or(0);
        entry.finalize_with_lsns(lsn, previous_lsn);
        state.wal.push(entry);
        Ok(lsn)
    }

    /// Append a `NonceRecord` to the nonce record list (used by
    /// `NonceTracker` and the cluster's local WAL).
    pub fn append_nonce_record(&self, record: NonceRecord) -> Result<(), WriterElectionError> {
        let mut state = self.state.lock();
        state.nonces.push(record);
        Ok(())
    }

    /// Read a range of WAL entries. `from_lsn` is inclusive; `to_lsn`
    /// is exclusive. `to_lsn = None` means "to the current tip + 1".
    pub fn read_wal_range(&self, from_lsn: u64, to_lsn: Option<u64>) -> Vec<WalEntry> {
        let state = self.state.lock();
        let to = to_lsn.unwrap_or_else(|| state.wal.len() as u64 + 1);
        state
            .wal
            .iter()
            .filter(|e| e.lsn >= from_lsn && e.lsn < to)
            .cloned()
            .collect()
    }

    /// Snapshot all nonce records. Used by `NonceTracker::replay_from_wal`.
    pub fn scan_nonce_records(&self) -> Vec<NonceRecord> {
        self.state.lock().nonces.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::substrate::wal::WAL_MAGIC_V13;

    fn hlc_now(node: WriterNodeId) -> HlcTimestamp {
        HlcTimestamp {
            physical_ms: 1_000,
            logical: 0,
            writer_node_id: node,
        }
    }

    #[test]
    fn first_acquire_wins() {
        let c = Cluster::new();
        let a = WriterNodeId([1u8; 32]);
        let b = WriterNodeId([2u8; 32]);
        let sk = ShardKey([7u8; 32]);
        let id_a = c.try_acquire_leader(a, sk, hlc_now(a)).unwrap();
        let r_b = c.try_acquire_leader(b, sk, hlc_now(b));
        assert!(matches!(r_b, Err(WriterElectionError::WriterUnavailable)));
        assert_eq!(id_a.writer_node_id, a);
        assert_eq!(id_a.term, 1);
    }

    #[test]
    fn failover_after_lease_expiry() {
        let c = Cluster::new();
        c.set_lease_duration_ms(0);
        let a = WriterNodeId([1u8; 32]);
        let b = WriterNodeId([2u8; 32]);
        let sk = ShardKey([7u8; 32]);
        let _id_a = c.try_acquire_leader(a, sk, hlc_now(a)).unwrap();
        // Lease 0 → any later call from b takes over.
        let id_b = c.try_acquire_leader(b, sk, hlc_now(b)).unwrap();
        assert_eq!(id_b.writer_node_id, b);
        assert!(id_b.term > 1);
    }

    #[test]
    fn kill_switch_blocks_acquire() {
        let c = Cluster::new();
        let a = WriterNodeId([1u8; 32]);
        let sk = ShardKey([7u8; 32]);
        c.kill(a);
        let r = c.try_acquire_leader(a, sk, hlc_now(a));
        assert!(matches!(r, Err(WriterElectionError::WriterUnavailable)));
        c.revive(a);
        let _id = c.try_acquire_leader(a, sk, hlc_now(a)).unwrap();
    }

    #[test]
    fn wal_append_assigns_monotonic_lsn() {
        let c = Cluster::new();
        let sk = ShardKey([0u8; 32]);
        let entry = WalEntry::build_v13(0x21, sk, vec![1, 2, 3]);
        let lsn1 = c.append_wal_entry(entry.clone()).unwrap();
        let lsn2 = c.append_wal_entry(entry.clone()).unwrap();
        let lsn3 = c.append_wal_entry(entry).unwrap();
        assert_eq!((lsn1, lsn2, lsn3), (1, 2, 3));
        let entries = c.read_wal_range(1, None);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].lsn, 1);
        assert_eq!(entries[1].lsn, 2);
        assert_eq!(entries[2].lsn, 3);
        assert_eq!(entries[1].previous_lsn, 1);
    }

    #[test]
    fn wal_checksum_survives_lsn_finalize() {
        let c = Cluster::new();
        let sk = ShardKey([0u8; 32]);
        let entry = WalEntry::build_v13(0x21, sk, vec![1, 2, 3]);
        let checksum_before = entry.checksum;
        let lsn = c.append_wal_entry(entry.clone()).unwrap();
        let stored = c.read_wal_range(lsn, Some(lsn + 1));
        assert_eq!(stored[0].lsn, lsn);
        // Checksum was recomputed after lsn finalize — should differ
        // from the pre-finalize placeholder.
        assert_ne!(stored[0].checksum, checksum_before);
        // Verify the recomputed checksum matches blake3(prefix || payload).
        let mut input = Vec::with_capacity(60 + stored[0].payload.len());
        input.extend_from_slice(&stored[0].prefix_bytes);
        input.extend_from_slice(&stored[0].payload);
        assert_eq!(stored[0].checksum, *blake3::hash(&input).as_bytes());
        // Magic preserved.
        assert_eq!(stored[0].magic, WAL_MAGIC_V13);
    }
}
