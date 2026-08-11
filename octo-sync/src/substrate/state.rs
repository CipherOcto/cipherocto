//! Writer state + replay state (per RFC-0862 v1.3 §Substrate types).
//!
//! `WriterIdentity` is the lease record returned by a successful
//! `WriterElection::acquire_writer` call. `WriterContext` is the
//! per-shard in-memory state the writer-election substrate maintains
//! during a lease. `ReplayState` is the 4-variant state machine tracking
//! WAL replay progress during `replay_wal` (separate from the 7-state
//! `WriterLifecycle` defined in RFC-0862 v1.3 §Roles and Authorities).

use std::sync::atomic::{AtomicBool, AtomicU32};

use borsh::{BorshDeserialize, BorshSerialize};

use super::hlc::HlcTimestamp;
use super::ids::ShardKey;
use super::ids::ShardMissionId;
use super::ids::WriterNodeId;

/// Writer identity returned by a successful election acquire (per RFC-0862
/// v1.3 §WriterElection Protocol).
///
/// `WriterIdentity` binds the writer_node_id, mission, term, elected
/// HLC, and shard key into a single record. The term + elected_at_hlc
/// pair establishes the lease's monotonicity: a later acquire on the
/// same `shard_key` MUST return a strictly higher `(term, elected_at_hlc)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct WriterIdentity {
    /// Writer node holding the lease.
    pub writer_node_id: WriterNodeId,
    /// Mission identity for which the writer was elected.
    pub mission_id: ShardMissionId,
    /// Election term (monotonic per shard_key).
    pub term: u64,
    /// HLC timestamp at the moment of election.
    pub elected_at_hlc: HlcTimestamp,
    /// Shard key for which the writer was elected.
    pub shard_key: ShardKey,
}

/// Per-shard in-memory state maintained by the writer-election substrate
/// during a lease (per RFC-0862 v1.3 §Substrate types).
///
/// `relinquish_pending` and `flush_attempts` are atomic so the substrate
/// can take `&self` method receivers; `replay_state` is a plain
/// `ReplayState` (mutated via `&mut WriterContext` in `replay_wal`).
pub struct WriterContext {
    /// `true` when a `relinquish_writer` has been issued but the lease
    /// handoff is still in progress.
    pub relinquish_pending: AtomicBool,
    /// Number of WAL-flush attempts during the current lease.
    pub flush_attempts: AtomicU32,
    /// Maximum allowed flush attempts before the lease is force-expired.
    pub max_attempts: u32,
    /// WAL replay state (mutated by `replay_wal`).
    pub replay_state: ReplayState,
}

/// WAL replay state (per RFC-0862 v1.3 §Substrate types).
///
/// 4-variant state machine. Transitions:
/// `Idle → InProgress → {Complete | Failed}`. The `replay_wal` function
/// owns the transition logic; callers obtain the current state via
/// `WriterContext.replay_state`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ReplayState {
    /// No replay in progress.
    #[default]
    Idle,
    /// Replay started; tracking LSN range + entries attempted.
    InProgress {
        /// LSN at which replay started.
        start_lsn: u64,
        /// Last successfully applied LSN.
        last_applied_lsn: u64,
        /// Number of entries read so far.
        attempted_entries: u32,
    },
    /// Replay failed at the given LSN range. `reason` is a static
    /// failure category (e.g., "WAL LSN gap or non-monotonic").
    Failed {
        /// LSN at which replay started.
        start_lsn: u64,
        /// Last successfully applied LSN.
        last_applied_lsn: u64,
        /// Number of entries read before failure.
        attempted_entries: u32,
        /// Static failure category string.
        reason: &'static str,
    },
    /// Replay completed; `tip_lsn` is the highest applied LSN.
    Complete {
        /// Highest applied LSN.
        tip_lsn: u64,
        /// Total entries applied.
        total_entries: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_context_defaults() {
        let ctx = WriterContext {
            relinquish_pending: AtomicBool::new(false),
            flush_attempts: AtomicU32::new(0),
            max_attempts: 100,
            replay_state: ReplayState::Idle,
        };
        assert!(!ctx
            .relinquish_pending
            .load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(
            ctx.flush_attempts
                .load(std::sync::atomic::Ordering::Acquire),
            0
        );
        assert_eq!(ctx.max_attempts, 100);
        assert_eq!(ctx.replay_state, ReplayState::Idle);
    }

    #[test]
    fn writer_identity_borsh_round_trip() {
        let id = WriterIdentity {
            writer_node_id: WriterNodeId([1u8; 32]),
            mission_id: ShardMissionId([2u8; 32]),
            term: 7,
            elected_at_hlc: HlcTimestamp {
                physical_ms: 1000,
                logical: 5,
                writer_node_id: WriterNodeId([1u8; 32]),
            },
            shard_key: ShardKey([3u8; 32]),
        };
        let bytes = borsh::to_vec(&id).unwrap();
        let decoded: WriterIdentity = WriterIdentity::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded, id);
    }

    #[test]
    fn replay_state_default_is_idle() {
        let s: ReplayState = Default::default();
        assert_eq!(s, ReplayState::Idle);
    }
}
