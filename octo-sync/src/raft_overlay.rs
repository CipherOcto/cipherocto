//! Raft overlay (F1/F8 future, per RFC-0862 §Future Work, mission 0862i).
//!
//! **STATUS: DEFERRED.** This mission is not in v1 or any current
//! implementation phase. The module is a placeholder for the future
//! Raft-based multi-leader implementation per RFC-0200 §A.
//!
//! # What this module would do (when un-deferred)
//!
//! - Elect a writer via Raft consensus (one writer per mission)
//! - Heartbeats writer liveness (3 missed heartbeats → election)
//! - Auto-failover: when the writer fails, a new writer is elected
//!   from the readers
//!
//! # Trait boundary
//!
//! The Raft overlay produces "Raft entries" (each is a WAL entry from the
//! Sync protocol). The Sync engine wraps each WAL entry in a Raft entry and
//! submits it to Raft consensus. When the Raft entry is committed, the
//! Sync engine applies it via `adapter.apply_wal_entry(entry)` — the
//! underlying `StoolapAdapter` impl internally calls
//! `MVCCEngine::replay_two_phase`.
//!
//! Per RFC-0862 v1.1.0, the cipherocto sync engine never calls
//! `MVCCEngine::replay_two_phase` directly; the trait is the integration
//! boundary.

use crate::adapter::DatabaseSyncAdapter;
use crate::envelope::WalTailChunk;
use std::sync::Arc;

/// A Raft log entry (one WAL entry wrapped for consensus).
#[derive(Debug, Clone)]
pub struct RaftEntry {
    /// The term when this entry was received by the leader.
    pub term: u64,
    /// The index of this entry in the Raft log.
    pub index: u64,
    /// The WAL entry payload (raw `WALEntry::encode()` output).
    pub wal_entry: Vec<u8>,
}

impl RaftEntry {
    /// Create a new `RaftEntry`.
    pub fn new(term: u64, index: u64, wal_entry: Vec<u8>) -> Self {
        Self { term, index, wal_entry }
    }
}

/// The Raft state machine role (per RFC-0862 §Future Work F1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaftRole {
    /// Follower (the default state).
    Follower,
    /// Candidate (during an election).
    Candidate,
    /// Leader (the writer).
    Leader,
}

/// The Raft overlay (stub).
///
/// v1 stub: only the type definitions are provided. The full Raft state
/// machine, election, heartbeat, and auto-failover logic is in the future
/// mission.
pub struct RaftOverlay {
    /// The local role.
    role: RaftRole,
    /// The local adapter (for the apply path).
    adapter: Arc<dyn DatabaseSyncAdapter>,
    /// The current term.
    term: u64,
}

impl RaftOverlay {
    /// Create a new `RaftOverlay` in the Follower role.
    pub fn new(adapter: Arc<dyn DatabaseSyncAdapter>) -> Self {
        Self { role: RaftRole::Follower, adapter, term: 0 }
    }

    /// Return the current role.
    pub fn role(&self) -> RaftRole {
        self.role
    }

    /// Return the current term.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Apply a committed Raft entry to the local database.
    /// Per RFC-0862 v1.1.0, this goes through `adapter.apply_wal_entry`,
    /// NOT via direct `MVCCEngine::replay_two_phase`.
    pub fn apply(&self, entry: &RaftEntry) -> Result<(), crate::error::SyncError> {
        self.adapter.apply_wal_entry(&entry.wal_entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::MockAdapter;

    #[test]
    fn new_overlay_is_follower() {
        let adapter: Arc<dyn DatabaseSyncAdapter> = Arc::new(MockAdapter::new([0u8; 32], [0u8; 32]));
        let o = RaftOverlay::new(adapter);
        assert_eq!(o.role(), RaftRole::Follower);
        assert_eq!(o.term(), 0);
    }

    #[test]
    fn apply_uses_adapter() {
        let adapter: Arc<MockAdapter> = Arc::new(MockAdapter::new([0u8; 32], [0u8; 32]));
        let a = adapter.clone() as Arc<dyn DatabaseSyncAdapter>;
        let o = RaftOverlay::new(a);
        let entry = RaftEntry::new(1, 1, b"test-wal-entry".to_vec());
        o.apply(&entry).unwrap();
        assert_eq!(adapter.current_lsn().unwrap(), 1);
    }

    #[test]
    fn raft_entry_construction() {
        let e = RaftEntry::new(1, 5, b"payload".to_vec());
        assert_eq!(e.term, 1);
        assert_eq!(e.index, 5);
        assert_eq!(e.wal_entry, b"payload");
    }
}
