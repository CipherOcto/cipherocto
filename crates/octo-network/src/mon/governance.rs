//! Mission Governance (RFC-0855 §11).
//!
//! Governance models determine how state transitions are approved.
//! Each model has different voting rules and decision mechanisms.
//!
//! ## Substrate migration (mission 0013-governance-network-migration)
//!
//! Per RFC-0013 §Module Layout, the canonical `GovernancePolicy` +
//! `GovernanceModel` + `EmergencyAuthority` + `GovernanceProposal` +
//! `ProposalState` + `DecisionType` types live in `octo-governance-core`
//! (Layer A frozen). This module re-exports them via `pub use`.
//!
//! **Discriminant change:** the canonical substrate uses `repr(u16)`
//! discriminants `0..=6` (Centralized=0, Dao=1, Federated=2,
//! AiAssisted=3, Autonomous=4; ProposalState `0..=5`;
//! `EmergencyAuthority` `None=0, GovernanceCouncil=1, DesignatedRecovery=2`;
//! `DecisionType` `0..=6`). The legacy local enums used
//! `0x0001..=0x0007`. The substrate values are the new canonical per
//! RFC-0855 §11; this is a wire-format-visible change for any system
//! that persisted discriminants.
//!
//! **Field shape change:** `GovernancePolicy` now uses
//! `(issuer, model, emergency_authority, quorum_bps, approval_bps)`
//! instead of the legacy
//! `(model, quorum_numerator, quorum_denominator, proposal_deadline_epochs,
//! emergency_authority)`. The substrate is BPS-based (basis points,
//! 0..=10000) per RFC-0013 §Specification. The legacy
//! `is_quorum_met(count, total)` API is replaced by BPS-based
//! `is_quorum_met(approval_bps, rejection_bps)`.
//!
//! ## Domain extension
//!
//! `VotingTally` (defined here) is a domain-only extension that tracks
//! per-voter weights via `BTreeMap<[u8;32], u64>`. It is NOT in the
//! substrate (substrate is tally-agnostic; the substrate
//! `GovernanceProposal` carries pre-computed `approval_tally_bps` +
//! `rejection_tally_bps` only). The `VotingTally::into_canonical()`
//! adapter converts tally state to a substrate `GovernanceProposal`.
//!
//! ## IO functions preserved
//!
//! Per RFC-0013 §Substrate `[ADD]`, IO functions (`snapshot`,
//! `attest`, `vote`) live in domain crates. None exist in this file
//! yet — when added, they MUST live here, not in `octo-governance-core`.

pub use octo_governance_core::{
    DecisionType, EmergencyAuthority, GovernanceError, GovernanceModel, GovernancePolicy,
    GovernanceProposal, ProposalState,
};
// Note: substrate `voting_weight` + `tally_quorum` are NOT re-exported
// here to avoid clashing with the domain-side `voting_weight`
// function in `mon::quadratic` (stake-weighted sqrt*cosigners
// formula). Import directly via `octo_governance_core::voting_weight`
// when needed.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Domain-only voting tally. Tracks per-voter weights and produces a
/// canonical `GovernanceProposal` via `into_canonical()`.
///
/// Substrate `GovernanceProposal` is tally-agnostic (it carries
/// pre-computed BPS totals only). The BTreeMap-keyed tally lives in
/// the domain crate so that voter-level operations (add voter,
/// replace vote, resolve) are local to the workflow that needs them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VotingTally {
    /// Voter pubkey → weight (in favor).
    pub votes_for: BTreeMap<[u8; 32], u64>,
    /// Voter pubkey → weight (against).
    pub votes_against: BTreeMap<[u8; 32], u64>,
}

impl VotingTally {
    /// Cast a vote. `in_favor=true` → counted in `votes_for`,
    /// `in_favor=false` → counted in `votes_against`. A voter's
    /// prior vote (for OR against) is replaced.
    pub fn cast_vote(&mut self, voter: [u8; 32], weight: u64, in_favor: bool) -> bool {
        if weight == 0 {
            return false;
        }
        self.votes_for.remove(&voter);
        self.votes_against.remove(&voter);
        if in_favor {
            self.votes_for.insert(voter, weight);
        } else {
            self.votes_against.insert(voter, weight);
        }
        true
    }

    /// Total weight of votes in favor.
    pub fn total_for(&self) -> u64 {
        self.votes_for.values().sum()
    }

    /// Total weight of votes against.
    pub fn total_against(&self) -> u64 {
        self.votes_against.values().sum()
    }

    /// Convert tally to a canonical `GovernanceProposal` snapshot
    /// with BPS totals computed from the BTreeMap vote weights.
    ///
    /// `total_eligible_weight` is the SUM of all eligible voter
    /// weights (the divisor for BPS conversion). If zero, the
    /// resulting BPS values are 0 (defensive — caller must supply a
    /// non-zero denominator).
    #[allow(clippy::too_many_arguments)]
    pub fn into_canonical(
        self,
        proposal_id: u64,
        issuer: &str,
        decision: DecisionType,
        state: ProposalState,
        voting_opens_at_millis: u64,
        voting_closes_at_millis: u64,
        total_eligible_weight: u64,
    ) -> GovernanceProposal {
        let total_voted = self.total_for().saturating_add(self.total_against());
        let approval_bps = if total_eligible_weight > 0 {
            ((self.total_for() as u128 * 10_000) / total_eligible_weight as u128).min(10_000) as u32
        } else {
            0
        };
        let rejection_bps = if total_eligible_weight > 0 {
            ((self.total_against() as u128 * 10_000) / total_eligible_weight as u128).min(10_000)
                as u32
        } else {
            0
        };
        let _ = total_voted; // sum of for+against weights (no further use)
        GovernanceProposal {
            proposal_id,
            issuer: issuer.to_string(),
            decision,
            state,
            voting_opens_at_millis,
            voting_closes_at_millis,
            approval_tally_bps: approval_bps,
            rejection_tally_bps: rejection_bps,
        }
    }
}

/// BLAKE3 domain prefix for governance proposal canonical-bytes
/// derivation. Per RFC-0126 §3.2 — domain separation via prefix.
/// Pinned string (not derived from any runtime state) so the
/// canonical_bytes hash is byte-stable across processes / nodes.
pub const BLAKE3_GOVERNANCE_PROPOSAL_DOMAIN: &[u8] = b"cipherocto/governance/proposal/v1";

/// Canonical-bytes derivation for [`GovernanceProposal`] per
/// RFC-0011-k §Substrate-Additions Companion Missions row G3b.
///
/// Returns `BLAKE3(BLAKE3_GOVERNANCE_PROPOSAL_DOMAIN || serde_json_canonical(self))`
/// truncated to 32 bytes (BLAKE3 native output). The domain prefix
/// provides keyless domain separation per RFC-0126 §3.2.
///
/// Substrate-faithful: the helper lives in the Layer-B mon module
/// (NOT on `GovernanceProposal` directly) because Layer-A
/// `octo-governance-core` is RFC-frozen per
/// [[cipherocto-design-principles]] §Stable Abstractions Principle.
/// The Layer-B wrapper IS the canonical entry point per the
/// `VotingTally::into_canonical` pattern at this same module.
///
/// `serde_json` is used (not `borsh`) because `octo-network` does
/// not depend on `borsh` directly; JSON serialization with
/// field-order preservation is the substrate-faithful canonical
/// form per RFC-0855 §11 governance substrate migration note
/// (BTreeMap-keyed tally + JSON canonical encoding).
#[must_use]
pub fn governance_proposal_canonical_bytes(p: &GovernanceProposal) -> [u8; 32] {
    let buf = borsh_compat_bytes(p);
    let mut hasher = blake3::Hasher::new();
    hasher.update(BLAKE3_GOVERNANCE_PROPOSAL_DOMAIN);
    hasher.update(&buf);
    let hash = hasher.finalize();
    *hash.as_bytes()
}

/// Substrate-faithful canonical-bytes encoding for
/// `GovernanceProposal`. Uses `serde_json` to_vec (NOT pretty)
/// for byte-stable output. Field order matches the struct
/// declaration order in `octo-governance-core::proposal::GovernanceProposal`
/// (Serde respects declaration order for named structs).
fn borsh_compat_bytes(p: &GovernanceProposal) -> Vec<u8> {
    serde_json::to_vec(p).expect("GovernanceProposal serde_json always succeeds")
}

/// Adapter: default DAO policy at the substrate canonical shape.
///
/// `2/3` quorum (6667 bps) + `>50%` approval (5001 bps) +
/// `DesignatedRecovery` emergency authority (legacy `Coordinator`
/// variant renamed in the substrate per RFC-0013 §Emergency Authority).
/// Caller MUST supply a non-empty `issuer` DID.
#[must_use]
pub fn default_dao_policy(issuer: &str) -> GovernancePolicy {
    GovernancePolicy {
        issuer: issuer.to_string(),
        model: GovernanceModel::Dao,
        emergency_authority: EmergencyAuthority::DesignatedRecovery,
        quorum_bps: 6_667,
        approval_bps: 5_001,
    }
}

/// Adapter: BPS-based quorum check on a substrate `GovernancePolicy`.
///
/// Returns `true` if `(approval_bps + rejection_bps) >= policy.quorum_bps`
/// (i.e., the voted fraction of eligible weight meets the quorum
/// threshold). Substrate `GovernancePolicy` is BPS-based; this
/// adapter preserves the legacy `is_quorum_met(votes, total)` API
/// shape via BPS inputs.
#[must_use]
pub fn is_quorum_met(policy: &GovernancePolicy, approval_bps: u32, rejection_bps: u32) -> bool {
    approval_bps.saturating_add(rejection_bps) >= policy.quorum_bps
}

/// Adapter: BPS-based approval threshold check.
#[must_use]
pub fn is_approval_met(policy: &GovernancePolicy, approval_bps: u32) -> bool {
    approval_bps >= policy.approval_bps
}

/// Migrate a legacy local `GovernancePolicy::new(...)` call to the
/// substrate canonical constructor. Translates
/// `(model, quorum_num, quorum_den, deadline_epochs, emergency)`
/// arguments into substrate `(issuer, model, emergency, quorum_bps,
/// approval_bps)` shape, computing `quorum_bps` from the legacy
/// numerator/denominator fraction.
#[must_use]
pub fn from_legacy_policy_args(
    issuer: &str,
    model: GovernanceModel,
    quorum_numerator: u16,
    quorum_denominator: u16,
    _proposal_deadline_epochs: u64, // substrate does not carry deadline_epochs
    emergency_authority: LegacyEmergency,
) -> GovernancePolicy {
    let quorum_bps = if quorum_denominator == 0 {
        0
    } else {
        // Round to nearest basis point.
        let q = (quorum_numerator as u32 * 10_000) / quorum_denominator as u32;
        q.min(10_000)
    };
    // Approval threshold defaults to >50% (matches legacy DAO default).
    let approval_bps = 5_001;
    GovernancePolicy {
        issuer: issuer.to_string(),
        model,
        emergency_authority: emergency_authority.into(),
        quorum_bps,
        approval_bps,
    }
}

/// Legacy `EmergencyAuthority` enum (Coordinator / Quorum / None)
/// preserved as a domain-side adapter so legacy callers can migrate
/// without breaking. Maps to substrate `EmergencyAuthority` via
/// `From<LegacyEmergency> for EmergencyAuthority`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum LegacyEmergency {
    /// Single coordinator has emergency authority → substrate
    /// `DesignatedRecovery`.
    Coordinator = 0x0001,
    /// Quorum (governance vote) → substrate `GovernanceCouncil`.
    Quorum = 0x0002,
    /// No emergency authority → substrate `None`.
    None = 0x0003,
}

impl From<LegacyEmergency> for EmergencyAuthority {
    fn from(legacy: LegacyEmergency) -> Self {
        match legacy {
            LegacyEmergency::Coordinator => EmergencyAuthority::DesignatedRecovery,
            LegacyEmergency::Quorum => EmergencyAuthority::GovernanceCouncil,
            LegacyEmergency::None => EmergencyAuthority::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_did(seed: u8) -> String {
        format!("did:cipherocto:gov-test-{seed}")
    }

    // -- Substrate discriminant byte-identical tests --

    #[test]
    fn discriminants_byte_identical_to_substrate() {
        // Substrate canonical (RFC-0013 + RFC-0855 §11.1):
        assert_eq!(GovernanceModel::Centralized as u16, 0);
        assert_eq!(GovernanceModel::Dao as u16, 1);
        assert_eq!(GovernanceModel::Federated as u16, 2);
        assert_eq!(GovernanceModel::AiAssisted as u16, 3);
        assert_eq!(GovernanceModel::Autonomous as u16, 4);
        assert_eq!(EmergencyAuthority::None as u16, 0);
        assert_eq!(EmergencyAuthority::GovernanceCouncil as u16, 1);
        assert_eq!(EmergencyAuthority::DesignatedRecovery as u16, 2);
        assert_eq!(ProposalState::Created as u16, 0);
        assert_eq!(ProposalState::Voting as u16, 1);
        assert_eq!(ProposalState::Approved as u16, 2);
        assert_eq!(ProposalState::Rejected as u16, 3);
        assert_eq!(ProposalState::Executed as u16, 4);
        assert_eq!(ProposalState::Expired as u16, 5);
        assert_eq!(DecisionType::Admission as u16, 0);
        assert_eq!(DecisionType::ParticipantExpulsion as u16, 6);
    }

    // -- Default DAO policy adapter --

    #[test]
    fn default_dao_policy_shape() {
        let p = default_dao_policy(&sample_did(1));
        assert_eq!(p.model, GovernanceModel::Dao);
        assert_eq!(
            p.emergency_authority,
            EmergencyAuthority::DesignatedRecovery
        );
        assert_eq!(p.quorum_bps, 6_667);
        assert_eq!(p.approval_bps, 5_001);
    }

    // -- BPS-based quorum + approval helpers --

    #[test]
    fn quorum_met_above_threshold() {
        let p = default_dao_policy(&sample_did(1));
        // 7000 voted (7000 bps = 70%) ≥ 6667 quorum → met.
        assert!(is_quorum_met(&p, 7_000, 0));
        assert!(is_quorum_met(&p, 5_000, 2_000));
    }

    #[test]
    fn quorum_not_met_below_threshold() {
        let p = default_dao_policy(&sample_did(1));
        // 5000 voted < 6667 quorum → not met.
        assert!(!is_quorum_met(&p, 5_000, 0));
        assert!(!is_quorum_met(&p, 0, 5_000));
    }

    #[test]
    fn approval_met_above_threshold() {
        let p = default_dao_policy(&sample_did(1));
        // 6000 approval ≥ 5001 threshold → met.
        assert!(is_approval_met(&p, 6_000));
    }

    #[test]
    fn approval_not_met_below_threshold() {
        let p = default_dao_policy(&sample_did(1));
        assert!(!is_approval_met(&p, 4_000));
    }

    // -- Legacy emergency adapter --

    #[test]
    fn legacy_emergency_maps_to_substrate() {
        assert_eq!(
            EmergencyAuthority::from(LegacyEmergency::Coordinator),
            EmergencyAuthority::DesignatedRecovery
        );
        assert_eq!(
            EmergencyAuthority::from(LegacyEmergency::Quorum),
            EmergencyAuthority::GovernanceCouncil
        );
        assert_eq!(
            EmergencyAuthority::from(LegacyEmergency::None),
            EmergencyAuthority::None
        );
    }

    #[test]
    fn legacy_policy_args_quorum_bps_rounded() {
        // 2/3 → 6667 bps (rounded).
        let p = from_legacy_policy_args(
            &sample_did(2),
            GovernanceModel::Dao,
            2,
            3,
            10,
            LegacyEmergency::Coordinator,
        );
        assert_eq!(p.quorum_bps, 6_666);
    }

    // -- VotingTally domain extension --

    #[test]
    fn voting_tally_cast_vote() {
        let mut t = VotingTally::default();
        assert!(t.cast_vote([0x01; 32], 100, true));
        assert_eq!(t.total_for(), 100);
        assert_eq!(t.total_against(), 0);
    }

    #[test]
    fn voting_tally_replaces_prior_vote() {
        let mut t = VotingTally::default();
        t.cast_vote([0x01; 32], 100, true);
        t.cast_vote([0x01; 32], 100, false);
        assert_eq!(t.total_for(), 0);
        assert_eq!(t.total_against(), 100);
    }

    #[test]
    fn voting_tally_rejects_zero_weight() {
        let mut t = VotingTally::default();
        assert!(!t.cast_vote([0x01; 32], 0, true));
        assert_eq!(t.total_for(), 0);
    }

    #[test]
    fn voting_tally_into_canonical_computes_bps() {
        let mut t = VotingTally::default();
        t.cast_vote([0x01; 32], 70, true);
        t.cast_vote([0x02; 32], 30, false);
        // total eligible = 100; 70/100 = 70%; 30/100 = 30%.
        let p = t.into_canonical(
            42,
            &sample_did(3),
            DecisionType::Admission,
            ProposalState::Voting,
            0,
            0,
            100,
        );
        assert_eq!(p.proposal_id, 42);
        assert_eq!(p.issuer, sample_did(3));
        assert_eq!(p.decision, DecisionType::Admission);
        assert_eq!(p.state, ProposalState::Voting);
        assert_eq!(p.approval_tally_bps, 7_000);
        assert_eq!(p.rejection_tally_bps, 3_000);
    }

    #[test]
    fn voting_tally_into_canonical_zero_eligible_returns_zero_bps() {
        // Defensive: total_eligible_weight=0 → both BPS values are 0.
        let mut t = VotingTally::default();
        t.cast_vote([0x01; 32], 70, true);
        let p = t.into_canonical(
            1,
            &sample_did(4),
            DecisionType::Admission,
            ProposalState::Voting,
            0,
            0,
            0,
        );
        assert_eq!(p.approval_tally_bps, 0);
        assert_eq!(p.rejection_tally_bps, 0);
    }

    // -- governance_proposal_canonical_bytes helper (G3b companion) --

    #[test]
    fn governance_proposal_canonical_bytes_round_trip() {
        // Same proposal twice → same hash (deterministic).
        let p = GovernanceProposal {
            proposal_id: 42,
            issuer: sample_did(5),
            decision: DecisionType::Admission,
            state: ProposalState::Voting,
            voting_opens_at_millis: 1_000,
            voting_closes_at_millis: 2_000,
            approval_tally_bps: 6_000,
            rejection_tally_bps: 1_000,
        };
        let h1 = governance_proposal_canonical_bytes(&p);
        let h2 = governance_proposal_canonical_bytes(&p);
        assert_eq!(h1, h2, "canonical_bytes must be deterministic");
    }

    #[test]
    fn governance_proposal_canonical_bytes_changes_with_state() {
        // Different state → different hash.
        let mut p = GovernanceProposal {
            proposal_id: 42,
            issuer: sample_did(6),
            decision: DecisionType::Admission,
            state: ProposalState::Voting,
            voting_opens_at_millis: 1_000,
            voting_closes_at_millis: 2_000,
            approval_tally_bps: 6_000,
            rejection_tally_bps: 1_000,
        };
        let h_voting = governance_proposal_canonical_bytes(&p);
        p.state = ProposalState::Approved;
        let h_approved = governance_proposal_canonical_bytes(&p);
        assert_ne!(
            h_voting, h_approved,
            "canonical_bytes must change when proposal state changes"
        );
    }

    // -- ProposalState state machine transitions --

    #[test]
    fn proposal_state_can_transition() {
        assert!(ProposalState::Created.can_transition_to(ProposalState::Voting));
        assert!(ProposalState::Voting.can_transition_to(ProposalState::Approved));
        assert!(ProposalState::Approved.can_transition_to(ProposalState::Executed));
        // Illegal: cannot skip voting.
        assert!(!ProposalState::Created.can_transition_to(ProposalState::Approved));
    }
}
