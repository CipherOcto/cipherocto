//! Mission Proof Policy (RFC-0859 §8)

/// Mission-scoped proof requirements (RFC-0859 §8.1).
///
/// Each Mission Overlay Network (RFC-0855) MAY define its own proof requirements.
#[derive(Debug, Clone)]
pub struct MissionProofPolicy {
    /// Mission identifier
    pub mission_id: [u8; 32],
    /// Required proof types for this mission
    pub required_proof_types: Vec<u16>,
    /// Allowed proof systems
    pub allowed_proof_systems: Vec<u16>,
    /// Minimum security level (proof size in bits)
    pub min_security_level: u16,
    /// Whether recursive aggregation is required
    pub require_aggregation: bool,
    /// Maximum proof age (in logical timestamps)
    pub max_proof_age: u64,
}

impl MissionProofPolicy {
    /// Check if a proof system is allowed by this policy.
    pub fn is_system_allowed(&self, system: u16) -> bool {
        self.allowed_proof_systems.contains(&system)
    }

    /// Check if a proof type is required by this policy.
    pub fn is_type_required(&self, proof_type: u16) -> bool {
        self.required_proof_types.contains(&proof_type)
    }

    /// Check if a proof age is within the policy limit.
    pub fn is_age_valid(&self, proof_age: u64) -> bool {
        proof_age <= self.max_proof_age
    }

    /// Validate a proof against this policy.
    /// Returns Ok(()) if valid, Err with reason if invalid.
    pub fn validate(
        &self,
        proof_system: u16,
        proof_type: u16,
        proof_age: u64,
        is_aggregated: bool,
    ) -> Result<(), String> {
        // Check if the proof type is required by this mission
        if !self.is_type_required(proof_type) {
            return Err(format!(
                "proof type {:#06x} not required by this mission",
                proof_type
            ));
        }
        if !self.is_system_allowed(proof_system) {
            return Err(format!(
                "proof system {:#06x} not in allowed list",
                proof_system
            ));
        }
        if !self.is_age_valid(proof_age) {
            return Err(format!(
                "proof age {} exceeds max {}",
                proof_age, self.max_proof_age
            ));
        }
        if self.require_aggregation && !is_aggregated {
            return Err("mission requires aggregated proof".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dot::pce::proof_type::{ProofSystemId, ProofType};

    fn default_policy() -> MissionProofPolicy {
        MissionProofPolicy {
            mission_id: [0xAAu8; 32],
            required_proof_types: vec![ProofType::InferenceProof as u16],
            allowed_proof_systems: vec![ProofSystemId::STWO as u16, ProofSystemId::PLONK as u16],
            min_security_level: 128,
            require_aggregation: false,
            max_proof_age: 1000,
        }
    }

    #[test]
    fn test_policy_system_allowed() {
        let policy = default_policy();
        assert!(policy.is_system_allowed(ProofSystemId::STWO as u16));
        assert!(policy.is_system_allowed(ProofSystemId::PLONK as u16));
        assert!(!policy.is_system_allowed(ProofSystemId::Groth16 as u16));
    }

    #[test]
    fn test_policy_type_required() {
        let policy = default_policy();
        assert!(policy.is_type_required(ProofType::InferenceProof as u16));
        assert!(!policy.is_type_required(ProofType::RelayProof as u16));
    }

    #[test]
    fn test_policy_age_valid() {
        let policy = default_policy();
        assert!(policy.is_age_valid(500));
        assert!(policy.is_age_valid(1000));
        assert!(!policy.is_age_valid(1001));
    }

    #[test]
    fn test_policy_validate_valid() {
        let policy = default_policy();
        assert!(policy
            .validate(
                ProofSystemId::STWO as u16,
                ProofType::InferenceProof as u16,
                500,
                false,
            )
            .is_ok());
    }

    #[test]
    fn test_policy_validate_wrong_system() {
        let policy = default_policy();
        assert!(policy
            .validate(
                ProofSystemId::Groth16 as u16,
                ProofType::InferenceProof as u16,
                500,
                false,
            )
            .is_err());
    }

    #[test]
    fn test_policy_validate_expired() {
        let policy = default_policy();
        assert!(policy
            .validate(
                ProofSystemId::STWO as u16,
                ProofType::InferenceProof as u16,
                2000,
                false,
            )
            .is_err());
    }

    #[test]
    fn test_policy_validate_requires_aggregation() {
        let mut policy = default_policy();
        policy.require_aggregation = true;
        assert!(policy
            .validate(
                ProofSystemId::STWO as u16,
                ProofType::InferenceProof as u16,
                500,
                false,
            )
            .is_err());
        assert!(policy
            .validate(
                ProofSystemId::STWO as u16,
                ProofType::InferenceProof as u16,
                500,
                true,
            )
            .is_ok());
    }

    #[test]
    fn test_policy_validate_boundary_age() {
        let policy = default_policy();
        assert!(policy
            .validate(
                ProofSystemId::STWO as u16,
                ProofType::InferenceProof as u16,
                1000,
                false,
            )
            .is_ok());
        assert!(policy
            .validate(
                ProofSystemId::STWO as u16,
                ProofType::InferenceProof as u16,
                1001,
                false,
            )
            .is_err());
    }

    #[test]
    fn test_policy_validate_wrong_proof_type() {
        let policy = default_policy();
        // RelayProof is not in required_proof_types
        assert!(policy
            .validate(
                ProofSystemId::STWO as u16,
                ProofType::RelayProof as u16,
                500,
                false,
            )
            .is_err());
    }
}
