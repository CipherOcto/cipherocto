//! Mission Governance (RFC-0855 §11)

use serde::{Deserialize, Serialize};

/// Governance models (RFC-0855 §11.1)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum GovernanceModel {
    Centralized = 0x0001,
    Dao = 0x0002,
    Federated = 0x0003,
    AiAssisted = 0x0004,
    Autonomous = 0x0005,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_governance_model_repr() {
        assert_eq!(GovernanceModel::Centralized as u16, 0x0001);
        assert_eq!(GovernanceModel::Autonomous as u16, 0x0005);
    }

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
}
