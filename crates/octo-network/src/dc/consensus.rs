//! Cross-platform DC consensus (mission 0855p-c-cross-platform-consensus).
//!
//! 2-phase commit protocol for REBIND/UNBIND across N platforms
//! (similar to 0850p-c-cross-node-rebind, but at the DC level).
//!
//! ## Quorum rules
//!
//! - N=1: no consensus (single platform); unilateral.
//! - N=2: both must agree (2/2).
//! - N≥3: 2/3 majority.
//!
//! Tie-break for N=2 with one yes, one no: action is rejected
//! (50% < 100%).

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::mon::bind_envelope::BindEnvelope;

/// DC consensus timeout (per mission spec).
pub const DC_CONSENSUS_TIMEOUT_EPOCHS: u64 = 1;

/// The action being consensus'd on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusAction {
    Rebind,
    Unbind,
}

impl ConsensusAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConsensusAction::Rebind => "rebind",
            ConsensusAction::Unbind => "unbind",
        }
    }
}

/// A vote from a DC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusVote {
    /// The DC has validated and is ready to commit.
    Prepared,
    /// The DC refuses to commit (with reason).
    Rejected,
}

/// State of the consensus coordinator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusState {
    Idle,
    Preparing,
    Committing,
    Committed,
    Aborted,
    TimedOut,
}

/// Outcome of the consensus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusOutcome {
    Committed,
    Aborted { reason: String },
    TimedOut,
    /// The consensus is still in progress (Idle, Preparing, or
    /// Committing). Callers should check the coordinator's
    /// `state` first to disambiguate.
    InProgress,
}

/// A consensus envelope (PREPARE / COMMIT / ABORT).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ConsensusEnvelope {
    #[serde(rename = "dc_consensus_prepare")]
    Prepare(PrepareEnvelope),
    #[serde(rename = "dc_consensus_commit")]
    Commit(CommitEnvelope),
    #[serde(rename = "dc_consensus_abort")]
    Abort(AbortEnvelope),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrepareEnvelope {
    pub domain_id: String,
    pub action: ConsensusAction,
    pub new_bind: Option<BindEnvelope>,
    pub init_at_epoch: u64,
    pub deadline_epoch: u64,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitEnvelope {
    pub domain_id: String,
    pub action: ConsensusAction,
    pub new_bind: Option<BindEnvelope>,
    pub vote_proofs: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbortEnvelope {
    pub domain_id: String,
    pub action: ConsensusAction,
    pub reason: String,
    pub signature: Vec<u8>,
}

/// DC consensus coordinator.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DcConsensusCoordinator {
    pub domain_id: String,
    pub action: ConsensusAction,
    pub new_bind: Option<BindEnvelope>,
    pub participants: Vec<String>, // DC pubkeys
    pub state: ConsensusState,
    pub responses: Vec<(String, ConsensusVote)>,
    pub init_at_epoch: u64,
    pub deadline_epoch: u64,
    pub abort_reason: Option<String>,
}

impl DcConsensusCoordinator {
    pub fn new(
        domain_id: impl Into<String>,
        action: ConsensusAction,
        new_bind: Option<BindEnvelope>,
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
            action,
            new_bind,
            participants,
            state: ConsensusState::Preparing,
            responses: Vec::new(),
            init_at_epoch: now,
            deadline_epoch: now + DC_CONSENSUS_TIMEOUT_EPOCHS,
            abort_reason: None,
        }
    }

    /// Returns the quorum required for this coordinator.
    pub fn quorum(&self) -> Quorum {
        Quorum::for_n(self.participants.len())
    }

    /// Record a vote from a DC. Returns the new state.
    pub fn record_vote(&mut self, dc_pubkey: &str, vote: ConsensusVote) -> ConsensusState {
        if self.state != ConsensusState::Preparing {
            return self.state;
        }
        // Guard: 0 participants is a construction error. Refuse
        // to commit (would be Unilateral + is_met(0,0) = true).
        if self.participants.is_empty() {
            return self.abort("no participants configured".to_string());
        }
        if !self.participants.iter().any(|p| p == dc_pubkey) {
            return self.abort(format!("unknown dc {dc_pubkey}"));
        }
        self.responses.retain(|(p, _)| p != dc_pubkey);
        self.responses.push((dc_pubkey.to_string(), vote));
        if vote == ConsensusVote::Rejected {
            return self.abort(format!("{dc_pubkey} voted rejected"));
        }
        // Check if quorum is met.
        let prepared = self
            .responses
            .iter()
            .filter(|(_, v)| *v == ConsensusVote::Prepared)
            .count();
        if self.quorum().is_met(prepared, self.participants.len()) {
            self.state = ConsensusState::Committing;
        }
        self.state
    }

    /// Check deadline and transition to TimedOut/Committing.
    pub fn check_deadline(&mut self, current_epoch: u64) -> ConsensusState {
        if self.state != ConsensusState::Preparing {
            return self.state;
        }
        // Guard: 0 participants is a construction error. Refuse
        // to commit. (See record_vote for the parallel guard.)
        if self.participants.is_empty() {
            return self.abort("no participants configured".to_string());
        }
        if current_epoch < self.deadline_epoch {
            return self.state;
        }
        let prepared = self
            .responses
            .iter()
            .filter(|(_, v)| *v == ConsensusVote::Prepared)
            .count();
        if self.quorum().is_met(prepared, self.participants.len()) {
            self.state = ConsensusState::Committing;
        } else {
            self.state = ConsensusState::TimedOut;
            self.abort_reason = Some(format!(
                "deadline elapsed with {}/{} prepared",
                prepared,
                self.participants.len()
            ));
        }
        self.state
    }

    /// Mark as committed.
    pub fn mark_committed(&mut self) {
        if self.state == ConsensusState::Committing {
            self.state = ConsensusState::Committed;
        }
    }

    fn abort(&mut self, reason: String) -> ConsensusState {
        if matches!(
            self.state,
            ConsensusState::Idle | ConsensusState::Committed | ConsensusState::Aborted | ConsensusState::TimedOut
        ) {
            return self.state;
        }
        self.state = ConsensusState::Aborted;
        self.abort_reason = Some(reason);
        ConsensusState::Aborted
    }

    /// Build the outcome.
    pub fn outcome(&self) -> ConsensusOutcome {
        match self.state {
            ConsensusState::Committed => ConsensusOutcome::Committed,
            ConsensusState::Aborted => ConsensusOutcome::Aborted {
                reason: self.abort_reason.clone().unwrap_or_else(|| "abort".into()),
            },
            ConsensusState::TimedOut => ConsensusOutcome::TimedOut,
            ConsensusState::Idle
            | ConsensusState::Preparing
            | ConsensusState::Committing => ConsensusOutcome::InProgress,
        }
    }
}

/// Quorum rules for DC consensus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quorum {
    /// N=1: unilateral (no consensus needed).
    Unilateral,
    /// N=2: unanimous.
    Unanimous,
    /// N>=3: 2/3 majority.
    TwoThirds,
}

impl Quorum {
    pub fn for_n(n: usize) -> Self {
        match n {
            0 | 1 => Quorum::Unilateral,
            2 => Quorum::Unanimous,
            _ => Quorum::TwoThirds,
        }
    }

    /// Returns true if `prepared` votes (out of `n` total)
    /// satisfy the quorum.
    pub fn is_met(&self, prepared: usize, n: usize) -> bool {
        // Defensive: 0 participants is a construction error.
        // Without this, Unilateral would return true (0 == 0
        // is trivially met), allowing an N=0 consensus to
        // "commit" with no votes. Callers should also guard
        // against N=0 at a higher level, but defending in depth
        // here prevents accidental misuse.
        if n == 0 {
            return false;
        }
        match self {
            Quorum::Unilateral => true, // no other DCs to wait for
            Quorum::Unanimous => prepared == n,
            Quorum::TwoThirds => prepared * 3 >= n * 2,
        }
    }
}

/// Build the libp2p gossip topic.
pub fn consensus_topic(domain_id: &str) -> String {
    assert!(!domain_id.is_empty(), "domain_id must not be empty");
    format!("/dot/dc-consensus/{domain_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_binds() -> Option<BindEnvelope> {
        None
    }

    #[test]
    fn n1_unilateral() {
        let q = Quorum::for_n(1);
        assert!(q.is_met(0, 1));
    }

    #[test]
    fn n0_is_never_met() {
        // Defensive: 0 participants must not claim quorum is met,
        // even though for_n(0) returns Unilateral.
        let q = Quorum::Unilateral;
        assert!(!q.is_met(0, 0));
        assert!(!Quorum::Unanimous.is_met(0, 0));
        assert!(!Quorum::TwoThirds.is_met(0, 0));
    }

    #[test]
    #[should_panic(expected = "domain_id must not be empty")]
    fn consensus_topic_rejects_empty() {
        let _ = consensus_topic("");
    }

    #[test]
    fn n2_unanimous_one_yes_not_enough() {
        let q = Quorum::for_n(2);
        assert!(!q.is_met(1, 2));
        assert!(q.is_met(2, 2));
    }

    #[test]
    fn n3_two_thirds_majority() {
        let q = Quorum::for_n(3);
        assert!(!q.is_met(1, 3));
        assert!(q.is_met(2, 3));
    }

    #[test]
    fn n4_two_thirds_majority() {
        let q = Quorum::for_n(4);
        assert!(!q.is_met(2, 4)); // 2/4 = 50% < 2/3
        assert!(q.is_met(3, 4)); // 3/4 = 75% >= 2/3
    }

    #[test]
    fn coordinator_unilateral_n1() {
        let mut c = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Rebind,
            empty_binds(),
            vec!["dc-1".into()],
        );
        // N=1: no vote needed; goes straight to Committing.
        assert_eq!(c.state, ConsensusState::Preparing);
        // No votes to record. Trigger deadline check.
        let state = c.check_deadline(c.deadline_epoch + 1);
        assert_eq!(state, ConsensusState::Committing);
    }

    #[test]
    fn coordinator_unanimous_n2() {
        let mut c = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Rebind,
            empty_binds(),
            vec!["dc-1".into(), "dc-2".into()],
        );
        c.record_vote("dc-1", ConsensusVote::Prepared);
        assert_eq!(c.state, ConsensusState::Preparing);
        c.record_vote("dc-2", ConsensusVote::Prepared);
        assert_eq!(c.state, ConsensusState::Committing);
        c.mark_committed();
        assert_eq!(c.state, ConsensusState::Committed);
    }

    #[test]
    fn coordinator_unanimous_n2_with_reject() {
        let mut c = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Rebind,
            empty_binds(),
            vec!["dc-1".into(), "dc-2".into()],
        );
        c.record_vote("dc-1", ConsensusVote::Prepared);
        c.record_vote("dc-2", ConsensusVote::Rejected);
        assert_eq!(c.state, ConsensusState::Aborted);
    }

    #[test]
    fn coordinator_two_thirds_n3() {
        let mut c = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Rebind,
            empty_binds(),
            vec!["dc-1".into(), "dc-2".into(), "dc-3".into()],
        );
        c.record_vote("dc-1", ConsensusVote::Prepared);
        c.record_vote("dc-2", ConsensusVote::Prepared);
        assert_eq!(c.state, ConsensusState::Committing); // 2/3 quorum
        c.mark_committed();
    }

    #[test]
    fn coordinator_timeout_no_quorum() {
        let mut c = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Rebind,
            empty_binds(),
            vec!["dc-1".into(), "dc-2".into()],
        );
        c.record_vote("dc-1", ConsensusVote::Prepared);
        // dc-2 never votes.
        let state = c.check_deadline(c.deadline_epoch + 1);
        assert_eq!(state, ConsensusState::TimedOut);
    }

    #[test]
    fn unknown_dc_vote_aborts() {
        let mut c = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Rebind,
            empty_binds(),
            vec!["dc-1".into()],
        );
        c.record_vote("attacker", ConsensusVote::Prepared);
        assert_eq!(c.state, ConsensusState::Aborted);
    }

    #[test]
    fn outcome_committed() {
        let mut c = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Unbind,
            empty_binds(),
            vec!["dc-1".into()],
        );
        c.state = ConsensusState::Committed;
        assert_eq!(c.outcome(), ConsensusOutcome::Committed);
    }

    #[test]
    fn topic_format() {
        assert_eq!(consensus_topic("d1"), "/dot/dc-consensus/d1");
    }

    #[test]
    fn n0_participants_aborts() {
        // 0 participants is a construction error; coordinator
        // must abort rather than commit with 0 votes.
        let mut c = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Rebind,
            empty_binds(),
            vec![],
        );
        // record_vote aborts because participants is empty.
        let state = c.record_vote("dc-1", ConsensusVote::Prepared);
        assert_eq!(state, ConsensusState::Aborted);
        match c.outcome() {
            ConsensusOutcome::Aborted { reason } => {
                assert!(reason.contains("no participants"));
            }
            other => panic!("expected Aborted, got {other:?}"),
        }

        // check_deadline also aborts (with fresh coordinator).
        let mut c2 = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Rebind,
            empty_binds(),
            vec![],
        );
        let state = c2.check_deadline(c2.deadline_epoch + 1);
        assert_eq!(state, ConsensusState::Aborted);
    }

    #[test]
    fn outcome_in_progress_for_preparing_state() {
        // outcome() must report InProgress (not Aborted) for
        // the Preparing state.
        let c = DcConsensusCoordinator::new(
            "d1",
            ConsensusAction::Rebind,
            empty_binds(),
            vec!["dc-1".into()],
        );
        assert_eq!(c.state, ConsensusState::Preparing);
        assert_eq!(c.outcome(), ConsensusOutcome::InProgress);
    }
}
