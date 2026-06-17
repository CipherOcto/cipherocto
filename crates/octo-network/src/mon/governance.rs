//! Mission Governance (RFC-0855 §11)
//!
//! Governance models determine how state transitions are approved.
//! Each model has different voting rules and decision mechanisms.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Governance models (RFC-0855 §11.1)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum GovernanceModel {
    /// Single Coordinator makes all decisions
    Centralized = 0x0001,
    /// Token-weighted voting
    Dao = 0x0002,
    /// Multi-party consensus
    Federated = 0x0003,
    /// AI proposes, humans approve
    AiAssisted = 0x0004,
    /// AI-only decision making
    Autonomous = 0x0005,
}

impl GovernanceModel {
    /// Parse from u16.
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::Centralized),
            0x0002 => Some(Self::Dao),
            0x0003 => Some(Self::Federated),
            0x0004 => Some(Self::AiAssisted),
            0x0005 => Some(Self::Autonomous),
            _ => None,
        }
    }
}

/// Emergency authority (RFC-0855 §11.2)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum EmergencyAuthority {
    Coordinator = 0x0001,
    Quorum = 0x0002,
    None = 0x0003,
}

/// Governance policy (RFC-0855 §11.2)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct GovernancePolicy {
    pub model: GovernanceModel,
    pub quorum_numerator: u16,
    pub quorum_denominator: u16,
    pub proposal_deadline_epochs: u64,
    pub emergency_authority: EmergencyAuthority,
}

impl GovernancePolicy {
    /// Create a validated governance policy.
    pub fn new(
        model: GovernanceModel,
        quorum_numerator: u16,
        quorum_denominator: u16,
        proposal_deadline_epochs: u64,
        emergency_authority: EmergencyAuthority,
    ) -> Result<Self, crate::mon::error::MonError> {
        if quorum_denominator == 0 {
            return Err(crate::mon::error::MonError::InvalidGovernancePolicy {
                reason: "quorum_denominator must be > 0".to_string(),
            });
        }
        if quorum_numerator > quorum_denominator {
            return Err(crate::mon::error::MonError::InvalidGovernancePolicy {
                reason: "quorum_numerator must be <= quorum_denominator".to_string(),
            });
        }
        if proposal_deadline_epochs == 0 {
            return Err(crate::mon::error::MonError::InvalidGovernancePolicy {
                reason: "proposal_deadline_epochs must be > 0".to_string(),
            });
        }
        Ok(Self {
            model,
            quorum_numerator,
            quorum_denominator,
            proposal_deadline_epochs,
            emergency_authority,
        })
    }

    /// Default DAO policy: 2/3 quorum, 10 epoch deadline, coordinator emergency.
    pub fn default_dao() -> Self {
        Self::new(
            GovernanceModel::Dao,
            2,
            3,
            10,
            EmergencyAuthority::Coordinator,
        )
        .expect("default_dao parameters are valid")
    }

    /// Check if a vote count meets quorum (count-based; for
    /// weight-based, use [`Self::is_weighted_quorum_met`]).
    pub fn is_quorum_met(&self, votes_for: u32, total_eligible: u32) -> bool {
        if total_eligible == 0 {
            return false;
        }
        // votes_for / total_eligible >= quorum_numerator / quorum_denominator
        // Cross-multiply to avoid floating point
        (votes_for as u64) * (self.quorum_denominator as u64)
            >= (self.quorum_numerator as u64) * (total_eligible as u64)
    }

    /// Check if a vote WEIGHT meets quorum (weight-based;
    /// appropriate for token-weighted DAO voting).
    pub fn is_weighted_quorum_met(&self, weight_voted: u64, total_eligible_weight: u64) -> bool {
        if total_eligible_weight == 0 {
            return false;
        }
        // weight_voted / total_eligible_weight >= quorum_numerator / quorum_denominator
        // Cross-multiply to avoid floating point and overflow.
        weight_voted
            .saturating_mul(self.quorum_denominator as u64)
            >= (self.quorum_numerator as u64).saturating_mul(total_eligible_weight)
    }
}

/// Decision types for governance voting (RFC-0855 §11.3)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum DecisionType {
    Admission = 0x0001,
    RoleAssignment = 0x0002,
    TopologyChange = 0x0003,
    MissionTermination = 0x0004,
    PolicyModification = 0x0005,
    EmergencyRekey = 0x0006,
    ParticipantExpulsion = 0x0007,
}

/// Proposal lifecycle states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum ProposalState {
    Created = 0x0001,
    Voting = 0x0002,
    Approved = 0x0003,
    Rejected = 0x0004,
    Executed = 0x0005,
    Expired = 0x0006,
}

/// A governance proposal.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GovernanceProposal {
    /// Proposal identifier
    pub proposal_id: [u8; 32],
    /// Type of decision
    pub decision_type: DecisionType,
    /// Current state
    pub state: ProposalState,
    /// Epoch when proposal was created
    pub created_epoch: u64,
    /// Epoch when voting deadline expires
    pub deadline_epoch: u64,
    /// Proposer gateway ID
    pub proposer: [u8; 32],
    /// Votes in favor (gateway_id -> weight)
    pub votes_for: BTreeMap<[u8; 32], u64>,
    /// Votes against (gateway_id -> weight)
    pub votes_against: BTreeMap<[u8; 32], u64>,
}

impl GovernanceProposal {
    /// Create a new proposal in Created state.
    pub fn new(
        proposal_id: [u8; 32],
        decision_type: DecisionType,
        proposer: [u8; 32],
        created_epoch: u64,
        deadline_epoch: u64,
    ) -> Self {
        Self {
            proposal_id,
            decision_type,
            state: ProposalState::Created,
            created_epoch,
            deadline_epoch,
            proposer,
            votes_for: BTreeMap::new(),
            votes_against: BTreeMap::new(),
        }
    }

    /// Open voting on this proposal.
    pub fn open_voting(&mut self) -> bool {
        if self.state == ProposalState::Created {
            self.state = ProposalState::Voting;
            return true;
        }
        false
    }

    /// Cast a vote. Returns false if proposal is not in Voting state
    /// or the weight is 0 (zero-weight votes are rejected to prevent
    /// BTreeMap spam with non-contributing entries).
    pub fn cast_vote(&mut self, voter: [u8; 32], weight: u64, in_favor: bool) -> bool {
        if self.state != ProposalState::Voting {
            return false;
        }
        if weight == 0 {
            return false;
        }
        if in_favor {
            self.votes_for.insert(voter, weight);
        } else {
            self.votes_against.insert(voter, weight);
        }
        true
    }

    /// Get total weight of votes in favor.
    pub fn total_for(&self) -> u64 {
        self.votes_for.values().sum()
    }

    /// Get total weight of votes against.
    pub fn total_against(&self) -> u64 {
        self.votes_against.values().sum()
    }

    /// Resolve the proposal based on governance policy.
    ///
    /// Uses count-based quorum (`total_eligible_voters` is the
    /// number of distinct voters). For weight-based quorum (DAO
    /// with token-weighted voting), use [`Self::resolve_weighted`].
    ///
    /// Returns the new state (Approved, Rejected, or remains Voting).
    pub fn resolve(
        &mut self,
        policy: &GovernancePolicy,
        total_eligible_voters: u32,
    ) -> ProposalState {
        if self.state != ProposalState::Voting {
            return self.state;
        }

        let for_count = self.votes_for.len() as u32;
        let against_count = self.votes_against.len() as u32;

        // Centralized: proposer decides (single coordinator)
        if policy.model == GovernanceModel::Centralized {
            self.state = ProposalState::Approved;
            return self.state;
        }

        // Autonomous: AI decides based on weighted votes
        if policy.model == GovernanceModel::Autonomous {
            if self.total_for() > self.total_against() {
                self.state = ProposalState::Approved;
            } else {
                self.state = ProposalState::Rejected;
            }
            return self.state;
        }

        // Federated, AiAssisted: count-based quorum + majority
        if policy.model == GovernanceModel::Federated
            || policy.model == GovernanceModel::AiAssisted
        {
            if policy.is_quorum_met(for_count + against_count, total_eligible_voters) {
                if self.total_for() > self.total_against() {
                    self.state = ProposalState::Approved;
                } else {
                    self.state = ProposalState::Rejected;
                }
            }
            return self.state;
        }

        // Dao: use weight-based quorum (caller should prefer
        // resolve_weighted; this is a fallback for callers that
        // only have voter count).
        if policy.is_quorum_met(for_count + against_count, total_eligible_voters) {
            if self.total_for() > self.total_against() {
                self.state = ProposalState::Approved;
            } else {
                self.state = ProposalState::Rejected;
            }
        }

        self.state
    }

    /// Resolve a DAO proposal with weight-based quorum.
    ///
    /// `total_eligible_weight` is the SUM of all eligible voters'
    /// weights. The proposal is approved if:
    /// 1. voted weight >= quorum fraction of total weight, AND
    /// 2. for-weight > against-weight
    pub fn resolve_weighted(
        &mut self,
        policy: &GovernancePolicy,
        total_eligible_weight: u64,
    ) -> ProposalState {
        if self.state != ProposalState::Voting {
            return self.state;
        }
        let voted_weight = self
            .total_for()
            .saturating_add(self.total_against());
        if policy.is_weighted_quorum_met(voted_weight, total_eligible_weight) {
            if self.total_for() > self.total_against() {
                self.state = ProposalState::Approved;
            } else {
                self.state = ProposalState::Rejected;
            }
        }
        self.state
    }

    /// Mark proposal as expired if deadline has passed.
    pub fn expire_if_past_deadline(&mut self, current_epoch: u64) -> bool {
        if self.state == ProposalState::Voting && current_epoch > self.deadline_epoch {
            self.state = ProposalState::Expired;
            return true;
        }
        false
    }

    /// Execute an approved proposal.
    pub fn execute(&mut self) -> bool {
        if self.state == ProposalState::Approved {
            self.state = ProposalState::Executed;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_policy() -> GovernancePolicy {
        GovernancePolicy::default_dao()
    }

    fn make_proposal(proposer: u8) -> GovernanceProposal {
        GovernanceProposal::new(
            [0xAA; 32],
            DecisionType::Admission,
            [proposer; 32],
            100,
            200,
        )
    }

    // -- GovernanceModel tests --

    #[test]
    fn test_governance_model_repr() {
        assert_eq!(GovernanceModel::Centralized as u16, 0x0001);
        assert_eq!(GovernanceModel::Autonomous as u16, 0x0005);
    }

    #[test]
    fn test_governance_model_from_u16() {
        assert_eq!(
            GovernanceModel::from_u16(0x0001),
            Some(GovernanceModel::Centralized)
        );
        assert_eq!(GovernanceModel::from_u16(0x0099), None);
    }

    // -- GovernancePolicy tests --

    #[test]
    fn test_default_dao_policy() {
        let p = GovernancePolicy::default_dao();
        assert_eq!(p.model, GovernanceModel::Dao);
        assert_eq!(p.quorum_numerator, 2);
        assert_eq!(p.quorum_denominator, 3);
        assert_eq!(p.emergency_authority, EmergencyAuthority::Coordinator);
    }

    #[test]
    fn test_emergency_authority_repr() {
        assert_eq!(EmergencyAuthority::Coordinator as u16, 0x0001);
        assert_eq!(EmergencyAuthority::None as u16, 0x0003);
    }

    #[test]
    fn test_governance_policy_new_valid() {
        let p = GovernancePolicy::new(
            GovernanceModel::Dao,
            2,
            3,
            10,
            EmergencyAuthority::Coordinator,
        );
        assert!(p.is_ok());
    }

    #[test]
    fn test_governance_policy_new_zero_denominator() {
        let p = GovernancePolicy::new(
            GovernanceModel::Dao,
            2,
            0,
            10,
            EmergencyAuthority::Coordinator,
        );
        assert!(p.is_err());
    }

    #[test]
    fn test_governance_policy_new_numerator_exceeds_denominator() {
        let p = GovernancePolicy::new(
            GovernanceModel::Dao,
            5,
            3,
            10,
            EmergencyAuthority::Coordinator,
        );
        assert!(p.is_err());
    }

    #[test]
    fn test_governance_policy_new_zero_deadline() {
        let p = GovernancePolicy::new(
            GovernanceModel::Dao,
            2,
            3,
            0,
            EmergencyAuthority::Coordinator,
        );
        assert!(p.is_err());
    }

    #[test]
    fn test_quorum_met() {
        let p = default_policy(); // 2/3 quorum
        assert!(p.is_quorum_met(7, 10)); // 7/10 >= 2/3
        assert!(!p.is_quorum_met(5, 10)); // 5/10 < 2/3
    }

    #[test]
    fn test_quorum_exact() {
        let p = default_policy(); // 2/3 quorum
        assert!(p.is_quorum_met(2, 3)); // exact 2/3
        assert!(p.is_quorum_met(4, 6)); // exact 2/3
    }

    #[test]
    fn test_quorum_zero_eligible() {
        let p = default_policy();
        assert!(!p.is_quorum_met(0, 0));
    }

    #[test]
    fn test_weighted_quorum_met() {
        let p = default_policy(); // 2/3 quorum
        // 70 of 100 weight voted: 70/100 >= 2/3 → met.
        assert!(p.is_weighted_quorum_met(70, 100));
        // 50 of 100: 50/100 < 2/3 → not met.
        assert!(!p.is_weighted_quorum_met(50, 100));
    }

    #[test]
    fn test_weighted_quorum_exact() {
        let p = default_policy();
        assert!(p.is_weighted_quorum_met(2, 3));
        assert!(p.is_weighted_quorum_met(66, 99));
    }

    #[test]
    fn test_weighted_quorum_zero_total() {
        let p = default_policy();
        assert!(!p.is_weighted_quorum_met(0, 0));
        // 0 voted / 0 total → false (defensive, matches count).
        assert!(!p.is_weighted_quorum_met(10, 0));
    }

    // -- GovernanceProposal tests --

    #[test]
    fn test_proposal_lifecycle_centralized() {
        let policy = GovernancePolicy::new(
            GovernanceModel::Centralized,
            1,
            1,
            10,
            EmergencyAuthority::Coordinator,
        )
        .unwrap();
        let mut prop = make_proposal(0x01);
        assert_eq!(prop.state, ProposalState::Created);

        prop.open_voting();
        assert_eq!(prop.state, ProposalState::Voting);

        // Centralized: auto-approved
        prop.resolve(&policy, 5);
        assert_eq!(prop.state, ProposalState::Approved);

        assert!(prop.execute());
        assert_eq!(prop.state, ProposalState::Executed);
    }

    #[test]
    fn test_proposal_lifecycle_dao_approved() {
        let policy = default_policy(); // 2/3 quorum
        let mut prop = make_proposal(0x01);
        prop.open_voting();

        // 3 for, 1 against out of 5 eligible = 4/5 voted (>= 2/3 quorum), majority for
        prop.cast_vote([0x01; 32], 100, true);
        prop.cast_vote([0x02; 32], 80, true);
        prop.cast_vote([0x03; 32], 60, true);
        prop.cast_vote([0x04; 32], 40, false);

        prop.resolve(&policy, 5);
        assert_eq!(prop.state, ProposalState::Approved);
    }

    #[test]
    fn test_proposal_lifecycle_dao_weighted_approved() {
        // Weight-based DAO: 70 of 100 weight voted (70% >= 2/3),
        // and 50 for, 20 against (for > against).
        let policy = default_policy();
        let mut prop = make_proposal(0x01);
        prop.open_voting();
        prop.cast_vote([0x01; 32], 50, true);
        prop.cast_vote([0x02; 32], 20, false);
        // Total weight voted = 70; total eligible = 100; 70/100 = 70% >= 2/3.
        prop.resolve_weighted(&policy, 100);
        assert_eq!(prop.state, ProposalState::Approved);
    }

    #[test]
    fn test_proposal_lifecycle_dao_weighted_quorum_fail() {
        // Only 50 of 200 weight voted: 25% < 2/3. Proposal stays
        // in Voting even though all votes are for.
        let policy = default_policy();
        let mut prop = make_proposal(0x01);
        prop.open_voting();
        prop.cast_vote([0x01; 32], 30, true);
        prop.cast_vote([0x02; 32], 20, true);
        prop.resolve_weighted(&policy, 200);
        assert_eq!(prop.state, ProposalState::Voting);
    }

    #[test]
    fn test_proposal_lifecycle_dao_rejected() {
        let policy = default_policy();
        let mut prop = make_proposal(0x01);
        prop.open_voting();

        prop.cast_vote([0x01; 32], 10, true);
        prop.cast_vote([0x02; 32], 100, false);
        prop.cast_vote([0x03; 32], 80, false);

        prop.resolve(&policy, 3); // 3/3 voted = 100% >= 2/3 quorum
        assert_eq!(prop.state, ProposalState::Rejected);
    }

    #[test]
    fn test_proposal_no_quorum() {
        let policy = default_policy(); // 2/3 quorum
        let mut prop = make_proposal(0x01);
        prop.open_voting();

        // Only 1 voter out of 10 eligible = 1/10 < 2/3
        prop.cast_vote([0x01; 32], 100, true);
        prop.resolve(&policy, 10);
        assert_eq!(prop.state, ProposalState::Voting); // still voting, no quorum
    }

    #[test]
    fn test_proposal_autonomous() {
        let policy = GovernancePolicy::new(
            GovernanceModel::Autonomous,
            1,
            1,
            10,
            EmergencyAuthority::None,
        )
        .unwrap();
        let mut prop = make_proposal(0x01);
        prop.open_voting();

        prop.cast_vote([0x01; 32], 100, true);
        prop.cast_vote([0x02; 32], 50, false);

        prop.resolve(&policy, 10);
        assert_eq!(prop.state, ProposalState::Approved); // for > against
    }

    #[test]
    fn test_proposal_expire() {
        let mut prop = make_proposal(0x01);
        prop.open_voting();
        assert!(!prop.expire_if_past_deadline(150)); // before deadline
        assert!(prop.expire_if_past_deadline(201)); // after deadline
        assert_eq!(prop.state, ProposalState::Expired);
    }

    #[test]
    fn test_proposal_cannot_vote_when_not_voting() {
        let mut prop = make_proposal(0x01);
        // Still in Created state
        assert!(!prop.cast_vote([0x01; 32], 100, true));
    }

    #[test]
    fn test_proposal_cannot_execute_unapproved() {
        let mut prop = make_proposal(0x01);
        prop.open_voting();
        assert!(!prop.execute()); // not approved yet
    }

    #[test]
    fn test_proposal_vote_weights() {
        let mut prop = make_proposal(0x01);
        prop.open_voting();
        prop.cast_vote([0x01; 32], 100, true);
        prop.cast_vote([0x02; 32], 50, true);
        prop.cast_vote([0x03; 32], 30, false);
        assert_eq!(prop.total_for(), 150);
        assert_eq!(prop.total_against(), 30);
    }

    #[test]
    fn test_decision_type_repr() {
        assert_eq!(DecisionType::Admission as u16, 0x0001);
        assert_eq!(DecisionType::ParticipantExpulsion as u16, 0x0007);
    }
}
