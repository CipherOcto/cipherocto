//! Canonical `GovernanceProposal` struct + `ProposalState` state machine +
//! `DecisionType` decision tag per RFC-0013 §Module Layout `proposal` +
//! RFC-0855 §11.3.

use serde::{Deserialize, Serialize};

/// Canonical governance proposal struct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceProposal {
    /// Monotonic proposal ID (per-issuer; issuer enforces uniqueness).
    pub proposal_id: u64,
    /// Issuer node DID (raw canonical wire form).
    pub issuer: String,
    /// Decision type being proposed.
    pub decision: DecisionType,
    /// Current state in the proposal lifecycle state machine.
    pub state: ProposalState,
    /// Voting window open timestamp (milliseconds since UNIX epoch).
    pub voting_opens_at_millis: u64,
    /// Voting window close timestamp (milliseconds since UNIX epoch).
    pub voting_closes_at_millis: u64,
    /// Tally of approval votes cast (basis points; 0..=10000).
    pub approval_tally_bps: u32,
    /// Tally of rejection votes cast (basis points; 0..=10000).
    pub rejection_tally_bps: u32,
}

/// Proposal lifecycle state machine. `#[non_exhaustive]` per CLAUDE.md
/// §Extension over enumeration — new states land via amendment.
///
/// # State transitions
///
/// - `Created → Voting` (admin opens vote)
/// - `Voting → Approved` (quorum reached + approval threshold met)
/// - `Voting → Rejected` (quorum reached + rejection threshold met OR
///   voting window expires without quorum)
/// - `Voting → Expired` (voting window closes without quorum)
/// - `Approved → Executed` (decision applied)
/// - `Approved → Expired` (execution window passes)
///
/// # Discriminants
///
/// `#[repr(u16)]` for byte-identical storage per RFC-0855 §11.3:
/// Created=0, Voting=1, Approved=2, Rejected=3, Executed=4, Expired=5.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum ProposalState {
    /// Proposal created, voting not yet opened.
    Created = 0,
    /// Voting window open, votes being collected.
    Voting = 1,
    /// Voting closed, approval threshold met.
    Approved = 2,
    /// Voting closed, rejection threshold met OR quorum failed.
    Rejected = 3,
    /// Decision applied (terminal state).
    Executed = 4,
    /// Voting window expired OR execution window expired (terminal state).
    Expired = 5,
}

/// Decision type being proposed. `#[non_exhaustive]` per CLAUDE.md
/// §Extension over enumeration — new decision types land via amendment.
///
/// # Discriminants
///
/// `#[repr(u16)]` for byte-identical storage per RFC-0855 §11.3:
/// Admission=0, RoleAssignment=1, TopologyChange=2,
/// MissionTermination=3, PolicyModification=4, EmergencyRekey=5,
/// ParticipantExpulsion=6.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum DecisionType {
    /// New participant admission (RFC-0855 §11.4).
    Admission = 0,
    /// Role assignment to participant.
    RoleAssignment = 1,
    /// Network topology change (node add/remove).
    TopologyChange = 2,
    /// Mission termination (force-stop active mission).
    MissionTermination = 3,
    /// Policy modification (change governance parameters).
    PolicyModification = 4,
    /// Emergency key rotation (override normal flow).
    EmergencyRekey = 5,
    /// Participant expulsion (revoke participant membership).
    ParticipantExpulsion = 6,
}

impl ProposalState {
    /// Whether `self` is a terminal state (no further transitions
    /// possible). `Executed` and `Expired` are terminal.
    pub fn is_terminal(self) -> bool {
        matches!(self, ProposalState::Executed | ProposalState::Expired)
    }

    /// Validate a state transition. Returns `true` if `from → to` is a
    /// legal transition per RFC-0013 §Lifecycle Requirements.
    pub fn can_transition_to(self, to: ProposalState) -> bool {
        use ProposalState::*;
        matches!(
            (self, to),
            (Created, Voting)
                | (Voting, Approved)
                | (Voting, Rejected)
                | (Voting, Expired)
                | (Approved, Executed)
                | (Approved, Expired)
        )
    }
}
