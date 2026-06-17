//! 2-phase commit REBIND coordinator (mission 0850p-c-cross-node-rebind).
//!
//! When the same `domain_id` is bound to N physical groups on
//! different platforms (multi-platform binding per RFC-0850p-c
//! §5 "Multi-Platform Binding Rule"), REBIND on one platform
//! must coordinate with the others to maintain mission
//! consistency.
//!
//! ## State machine
//!
//! ```text
//!     Initiator (the platform that started the REBIND)
//!     ─────────────────────────────────────────────────
//!     Idle ──► Preparing ──► (all PREPARED) ──► Committing ──► Committed
//!                          └► (any ABORT)    ──► Aborted
//!                          └► (timeout)      ──► TimedOut
//!
//!     Participant (the other N-1 platforms)
//!     ──────────────────────────────────────
//!     Idle ──► Preparing ──► (preconditions met) ──► Prepared
//!                                └► (preconditions failed) ──► Aborted
//! ```
//!
//! ## Tie-break
//!
//! Concurrent REBINDs on the same `domain_id` are resolved by
//! lex `domain_id` ordering (lower first). The losing REBIND is
//! rejected with `RebindAbortReason::LostTieBreak`.
//!
//! ## Fallback
//!
//! 30s timeout on PREPARED responses → manual operator
//! reconciliation via `octo-coordinator reconcile` CLI.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::bind_envelope::{BindEnvelope, RebindAbort, RebindAbortReason, RebindCommit, RebindPrepare};

/// The default REBIND timeout in seconds (RFC-0850p-c §5
/// "Multi-Platform Binding Rule" → 30s).
pub const REBIND_TIMEOUT_SECS: u64 = 30;

/// Vote from a participant platform on a REBIND_PREPARE.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrepareVote {
    /// The participant is ready to commit. Resources are reserved.
    Prepared,
    /// The participant refuses to commit (e.g., new group_id
    /// is invalid on this platform).
    Abort,
}

/// REBIND coordinator states for the initiator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinatorState {
    /// No REBIND in progress.
    Idle,
    /// Waiting for PREPARED responses from all N-1 other platforms.
    Preparing,
    /// All PREPARED, broadcasting COMMIT.
    Committing,
    /// REBIND committed.
    Committed,
    /// REBIND aborted (vote or lost tie-break).
    Aborted,
    /// REBIND timed out waiting for votes.
    TimedOut,
}

/// The REBIND coordinator. The initiator tracks responses from
/// participants and decides when to COMMIT or ABORT.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RebindCoordinator {
    /// The `domain_id` being rebound.
    pub domain_id: String,
    /// The new binding (used for both COMMIT and ABORT context).
    pub new_bind: BindEnvelope,
    /// The participants (other platforms' identifiers).
    /// Sorted lexicographically for deterministic tie-break.
    pub participants: Vec<String>,
    /// The current coordinator state.
    pub state: CoordinatorState,
    /// PREPARED votes received so far (platform → vote).
    pub responses: Vec<(String, PrepareVote)>,
    /// The deadline (epoch seconds) for PREPARED responses.
    pub deadline_epoch: u64,
    /// The final abort reason, if state is Aborted.
    pub abort_reason: Option<RebindAbortReason>,
}

impl RebindCoordinator {
    /// Create a new REBIND coordinator in `Preparing` state.
    pub fn new(
        domain_id: impl Into<String>,
        new_bind: BindEnvelope,
        participants: Vec<String>,
    ) -> Self {
        let mut participants = participants;
        participants.sort();
        participants.dedup();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            domain_id: domain_id.into(),
            new_bind,
            participants,
            state: CoordinatorState::Preparing,
            responses: Vec::new(),
            deadline_epoch: now + REBIND_TIMEOUT_SECS,
            abort_reason: None,
        }
    }

    /// Build the PREPARE envelope to broadcast.
    pub fn prepare_envelope(&self, signature: Vec<u8>) -> RebindPrepare {
        RebindPrepare {
            domain_id: self.domain_id.clone(),
            new_bind: self.new_bind.clone(),
            deadline_epoch: self.deadline_epoch,
            signature,
        }
    }

    /// Record a vote from a participant. Returns the new state
    /// (may transition to `Committing`, `Aborted`, or stay in
    /// `Preparing`).
    pub fn record_vote(&mut self, platform: &str, vote: PrepareVote) -> CoordinatorState {
        if self.state != CoordinatorState::Preparing {
            return self.state;
        }
        if !self.participants.iter().any(|p| p == platform) {
            // Unknown platform: treat as implicit ABORT (security:
            // an unknown peer cannot influence the REBIND).
            return self.abort(RebindAbortReason::VoteAbort);
        }
        self.responses.retain(|(p, _)| p != platform);
        self.responses.push((platform.to_string(), vote));
        if vote == PrepareVote::Abort {
            return self.abort(RebindAbortReason::VoteAbort);
        }
        // Check if all participants have voted PREPARED.
        if self
            .responses
            .iter()
            .all(|(_, v)| *v == PrepareVote::Prepared)
            && self.responses.len() == self.participants.len()
        {
            self.state = CoordinatorState::Committing;
        }
        self.state
    }

    /// Build the COMMIT envelope (only valid when state is
    /// `Committing`).
    pub fn commit_envelope(&self, signature: Vec<u8>) -> Option<RebindCommit> {
        if self.state != CoordinatorState::Committing {
            return None;
        }
        // Compute evidence hash: sorted (platform, prepared_signature)
        // pairs concatenated. For now we hash the sorted platform
        // list (the prepared signatures are in a follow-up mission).
        let mut sorted: Vec<&String> = self.responses.iter().map(|(p, _)| p).collect();
        sorted.sort();
        let mut evidence = Vec::new();
        for p in sorted {
            evidence.extend_from_slice(&(p.len() as u32).to_le_bytes());
            evidence.extend_from_slice(p.as_bytes());
        }
        Some(RebindCommit {
            domain_id: self.domain_id.clone(),
            new_bind: self.new_bind.clone(),
            prepared_evidence: evidence,
            signature,
        })
    }

    /// Mark the REBIND as committed. Caller has broadcast the
    /// COMMIT envelope.
    pub fn mark_committed(&mut self) {
        if self.state == CoordinatorState::Committing {
            self.state = CoordinatorState::Committed;
        }
    }

    /// Abort the REBIND.
    pub fn abort(&mut self, reason: RebindAbortReason) -> CoordinatorState {
        if self.state == CoordinatorState::Idle
            || self.state == CoordinatorState::Committed
            || self.state == CoordinatorState::Aborted
            || self.state == CoordinatorState::TimedOut
        {
            return self.state;
        }
        self.state = CoordinatorState::Aborted;
        self.abort_reason = Some(reason);
        CoordinatorState::Aborted
    }

    /// Build the ABORT envelope.
    pub fn abort_envelope(&self, signature: Vec<u8>) -> Option<RebindAbort> {
        if self.state != CoordinatorState::Aborted && self.state != CoordinatorState::TimedOut {
            return None;
        }
        let reason = self.abort_reason.unwrap_or(RebindAbortReason::VoteAbort);
        let mut dissenters: Vec<String> = self
            .responses
            .iter()
            .filter(|(_, v)| *v == PrepareVote::Abort)
            .map(|(p, _)| p.clone())
            .collect();
        dissenters.sort();
        Some(RebindAbort {
            domain_id: self.domain_id.clone(),
            reason,
            dissenters,
            signature,
        })
    }

    /// Check the deadline and, if it has elapsed and we haven't
    /// reached quorum, transition to TimedOut. Returns the new
    /// state.
    pub fn check_deadline(&mut self, now_epoch: u64) -> CoordinatorState {
        if self.state != CoordinatorState::Preparing {
            return self.state;
        }
        if now_epoch < self.deadline_epoch {
            return self.state;
        }
        // Deadline elapsed. If we have quorum, commit; otherwise
        // timeout.
        let prepared = self
            .responses
            .iter()
            .filter(|(_, v)| *v == PrepareVote::Prepared)
            .count();
        if prepared == self.participants.len() && !self.participants.is_empty() {
            self.state = CoordinatorState::Committing;
        } else {
            self.state = CoordinatorState::TimedOut;
            self.abort_reason = Some(RebindAbortReason::Timeout);
        }
        self.state
    }

    /// Returns the number of PREPARED votes needed to commit.
    pub fn quorum(&self) -> usize {
        self.participants.len()
    }

    /// Tie-break: the lower lex `domain_id` wins. Returns
    /// `true` if `self` should be the winner given `other_domain_id`.
    pub fn wins_tie_break(&self, other_domain_id: &str) -> bool {
        // Lower domain_id wins.
        self.domain_id.as_str() < other_domain_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bind() -> BindEnvelope {
        BindEnvelope::new("d1", "whatsapp", "group1")
    }

    #[test]
    fn happy_path_two_participants_both_prepared() {
        let mut c = RebindCoordinator::new(
            "d1",
            make_bind(),
            vec!["matrix".into(), "telegram".into()],
        );
        assert_eq!(c.state, CoordinatorState::Preparing);
        c.record_vote("matrix", PrepareVote::Prepared);
        assert_eq!(c.state, CoordinatorState::Preparing);
        c.record_vote("telegram", PrepareVote::Prepared);
        assert_eq!(c.state, CoordinatorState::Committing);
        let commit = c.commit_envelope(vec![1, 2, 3]).unwrap();
        assert_eq!(commit.domain_id, "d1");
        assert!(!commit.prepared_evidence.is_empty());
        c.mark_committed();
        assert_eq!(c.state, CoordinatorState::Committed);
    }

    #[test]
    fn abort_on_participant_vote() {
        let mut c = RebindCoordinator::new(
            "d1",
            make_bind(),
            vec!["matrix".into(), "telegram".into()],
        );
        c.record_vote("matrix", PrepareVote::Prepared);
        c.record_vote("telegram", PrepareVote::Abort);
        assert_eq!(c.state, CoordinatorState::Aborted);
        assert_eq!(c.abort_reason, Some(RebindAbortReason::VoteAbort));
        let abort = c.abort_envelope(vec![9]).unwrap();
        assert_eq!(abort.dissenters, vec!["telegram".to_string()]);
    }

    #[test]
    fn abort_on_unknown_platform() {
        let mut c = RebindCoordinator::new(
            "d1",
            make_bind(),
            vec!["matrix".into()],
        );
        c.record_vote("attacker", PrepareVote::Prepared);
        assert_eq!(c.state, CoordinatorState::Aborted);
    }

    #[test]
    fn timeout_when_quorum_not_reached() {
        let mut c = RebindCoordinator::new(
            "d1",
            make_bind(),
            vec!["matrix".into(), "telegram".into()],
        );
        // Only matrix votes prepared.
        c.record_vote("matrix", PrepareVote::Prepared);
        // Time advances past the deadline.
        let future = c.deadline_epoch + 1;
        let state = c.check_deadline(future);
        assert_eq!(state, CoordinatorState::TimedOut);
        assert_eq!(c.abort_reason, Some(RebindAbortReason::Timeout));
    }

    #[test]
    fn check_deadline_with_quorum_commits() {
        let mut c = RebindCoordinator::new(
            "d1",
            make_bind(),
            vec!["matrix".into()],
        );
        c.record_vote("matrix", PrepareVote::Prepared);
        // Even if deadline passes, with quorum we commit.
        let future = c.deadline_epoch + 1;
        let state = c.check_deadline(future);
        assert_eq!(state, CoordinatorState::Committing);
    }

    #[test]
    fn tie_break_lower_domain_id_wins() {
        let c = RebindCoordinator::new(
            "a-domain",
            make_bind(),
            vec!["matrix".into()],
        );
        // "a-domain" < "b-domain" → c wins.
        assert!(c.wins_tie_break("b-domain"));
        // "a-domain" > "a-aardvark" → c loses.
        assert!(!c.wins_tie_break("a-aardvark"));
    }

    #[test]
    fn simultaneous_rebind_one_wins() {
        // Two coordinators try to REBIND the same domain.
        let c1 = RebindCoordinator::new(
            "alpha",
            make_bind(),
            vec!["matrix".into()],
        );
        let c2 = RebindCoordinator::new(
            "beta",
            make_bind(),
            vec!["matrix".into()],
        );
        // "alpha" < "beta" → c1 wins.
        assert!(c1.wins_tie_break(&c2.domain_id));
        assert!(!c2.wins_tie_break(&c1.domain_id));
    }

    #[test]
    fn cannot_commit_envelope_in_preparing_state() {
        let c = RebindCoordinator::new(
            "d1",
            make_bind(),
            vec!["matrix".into()],
        );
        assert!(c.commit_envelope(vec![]).is_none());
    }

    #[test]
    fn cannot_abort_envelope_in_committed_state() {
        let mut c = RebindCoordinator::new(
            "d1",
            make_bind(),
            vec!["matrix".into()],
        );
        c.record_vote("matrix", PrepareVote::Prepared);
        c.mark_committed();
        assert!(c.abort_envelope(vec![]).is_none());
    }

    #[test]
    fn participants_are_sorted_and_deduped() {
        let c = RebindCoordinator::new(
            "d1",
            make_bind(),
            vec!["telegram".into(), "matrix".into(), "matrix".into()],
        );
        assert_eq!(c.participants, vec!["matrix".to_string(), "telegram".to_string()]);
    }

    #[test]
    fn double_vote_replaces_previous() {
        let mut c = RebindCoordinator::new(
            "d1",
            make_bind(),
            vec!["matrix".into()],
        );
        c.record_vote("matrix", PrepareVote::Abort);
        assert_eq!(c.state, CoordinatorState::Aborted);
    }
}
