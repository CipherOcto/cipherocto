//! Mission Membership (RFC-0855 §4)

use serde::{Deserialize, Serialize};

mod serde_signature {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        serde_bytes::serialize(sig.as_ref(), s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: Vec<u8> = serde_bytes::deserialize(d)?;
        if v.len() != 64 {
            return Err(serde::de::Error::invalid_length(v.len(), &"64 bytes"));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&v);
        Ok(arr)
    }
}

/// Mission node — a participant in a mission overlay.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MissionNode {
    pub peer_id: [u8; 32],
    pub role_flags: u64,
    pub trust_score: u32,
    pub capability_root: [u8; 32],
    pub join_epoch: u64,
    #[serde(with = "serde_signature")]
    pub membership_signature: [u8; 64],
}

/// Role flag bits (RFC-0855 §4.2)
pub const ROLE_COORDINATOR: u64 = 0x0001;
pub const ROLE_EXECUTOR: u64 = 0x0002;
pub const ROLE_RELAY: u64 = 0x0004;
pub const ROLE_VALIDATOR: u64 = 0x0008;
pub const ROLE_OBSERVER: u64 = 0x0010;
pub const ROLE_ARCHIVIST: u64 = 0x0020;
pub const ROLE_PROVER: u64 = 0x0040;
pub const ROLE_AGGREGATOR: u64 = 0x0080;

/// Maximum roles per node.
pub const MAX_ROLES_PER_NODE: u32 = 4;

/// Bitmask of all defined role flags. Any bits outside this
/// mask are considered unknown roles and rejected.
pub const KNOWN_ROLE_MASK: u64 =
    ROLE_COORDINATOR
        | ROLE_EXECUTOR
        | ROLE_RELAY
        | ROLE_VALIDATOR
        | ROLE_OBSERVER
        | ROLE_ARCHIVIST
        | ROLE_PROVER
        | ROLE_AGGREGATOR;

/// Minimum trust score for Coordinator role.
pub const MIN_TRUST_COORDINATOR: u32 = 500;
/// Minimum trust score for Validator role.
pub const MIN_TRUST_VALIDATOR: u32 = 300;

/// Validate that a node's trust score meets the requirements for its assigned roles.
pub fn validate_role_assignment(
    role_flags: u64,
    trust_score: u32,
) -> Result<(), crate::mon::error::MonError> {
    if role_flags & ROLE_COORDINATOR != 0 && trust_score < MIN_TRUST_COORDINATOR {
        return Err(crate::mon::error::MonError::InvalidRoleAssignment {
            reason: format!(
                "Coordinator requires trust_score >= {}, got {}",
                MIN_TRUST_COORDINATOR, trust_score
            ),
        });
    }
    if role_flags & ROLE_VALIDATOR != 0 && trust_score < MIN_TRUST_VALIDATOR {
        return Err(crate::mon::error::MonError::InvalidRoleAssignment {
            reason: format!(
                "Validator requires trust_score >= {}, got {}",
                MIN_TRUST_VALIDATOR, trust_score
            ),
        });
    }
    Ok(())
}

/// Check if role combination is valid (RFC-0855 §4.2 constraints).
pub fn is_valid_role_combination(role_flags: u64) -> bool {
    // Reject unknown role bits. Without this, a malicious or
    // outdated node could submit role_flags with high bits set
    // that don't correspond to any defined role.
    if role_flags & !KNOWN_ROLE_MASK != 0 {
        return false;
    }
    let role_count = role_flags.count_ones();
    if role_count == 0 {
        return false;
    }

    let has_coordinator = role_flags & ROLE_COORDINATOR != 0;
    let has_prover = role_flags & ROLE_PROVER != 0;
    let has_aggregator = role_flags & ROLE_AGGREGATOR != 0;
    let has_observer = role_flags & ROLE_OBSERVER != 0;

    // Forbidden combinations
    if has_coordinator && has_prover {
        return false;
    }
    if has_coordinator && has_aggregator {
        return false;
    }
    if has_observer && has_coordinator {
        return false;
    }

    // Max 4 roles
    role_count <= MAX_ROLES_PER_NODE
}

/// Admission policy (RFC-0855 §4.3)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum AdmissionPolicy {
    Open = 0x0001,
    InviteOnly = 0x0002,
    StakeGated = 0x0003,
    TrustGated = 0x0004,
    CapabilityGated = 0x0005,
}

/// Compute membership commitment: BLAKE3-256(mission_id || peer_id || role_flags || join_epoch)
pub fn compute_membership_commitment(
    mission_id: &[u8; 32],
    peer_id: &[u8; 32],
    role_flags: u64,
    join_epoch: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(mission_id);
    hasher.update(peer_id);
    hasher.update(&role_flags.to_be_bytes());
    hasher.update(&join_epoch.to_be_bytes());
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_role_combinations() {
        assert!(is_valid_role_combination(ROLE_COORDINATOR));
        assert!(is_valid_role_combination(
            ROLE_EXECUTOR | ROLE_RELAY | ROLE_VALIDATOR
        ));
        assert!(is_valid_role_combination(ROLE_OBSERVER));
        assert!(is_valid_role_combination(
            ROLE_RELAY | ROLE_VALIDATOR | ROLE_ARCHIVIST | ROLE_PROVER
        ));
    }

    #[test]
    fn test_forbidden_role_combinations() {
        assert!(!is_valid_role_combination(ROLE_COORDINATOR | ROLE_PROVER));
        assert!(!is_valid_role_combination(
            ROLE_COORDINATOR | ROLE_AGGREGATOR
        ));
        assert!(!is_valid_role_combination(ROLE_OBSERVER | ROLE_COORDINATOR));
    }

    #[test]
    fn test_max_roles_enforced() {
        let five_roles =
            ROLE_COORDINATOR | ROLE_EXECUTOR | ROLE_RELAY | ROLE_VALIDATOR | ROLE_ARCHIVIST;
        assert!(!is_valid_role_combination(five_roles));
    }

    #[test]
    fn test_membership_commitment_deterministic() {
        let mission_id = [1u8; 32];
        let peer_id = [2u8; 32];
        let c1 = compute_membership_commitment(&mission_id, &peer_id, 5, 100);
        let c2 = compute_membership_commitment(&mission_id, &peer_id, 5, 100);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_membership_commitment_different_inputs() {
        let mission_id = [1u8; 32];
        let peer1 = [2u8; 32];
        let peer2 = [3u8; 32];
        let c1 = compute_membership_commitment(&mission_id, &peer1, 5, 100);
        let c2 = compute_membership_commitment(&mission_id, &peer2, 5, 100);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_admission_policy_repr() {
        assert_eq!(AdmissionPolicy::Open as u16, 0x0001);
        assert_eq!(AdmissionPolicy::CapabilityGated as u16, 0x0005);
    }

    #[test]
    fn test_zero_roles_invalid() {
        assert!(!is_valid_role_combination(0));
    }

    #[test]
    fn test_unknown_role_bits_rejected() {
        // High bits beyond KNOWN_ROLE_MASK must be rejected.
        // Without this guard, a node could submit role_flags
        // with unknown bits set and be admitted.
        assert!(!is_valid_role_combination(0x8000_0000_0000_0000));
        assert!(!is_valid_role_combination(ROLE_COORDINATOR | 0x0100));
        assert!(!is_valid_role_combination(0xFFFF_FFFF_FFFF_FFFF));
        // All-known bits are still subject to the max-4-roles
        // rule, so KNOWN_ROLE_MASK (8 bits) is rejected by
        // count. Use a known-4-bits combination instead.
        assert!(is_valid_role_combination(
            ROLE_EXECUTOR | ROLE_RELAY | ROLE_VALIDATOR | ROLE_ARCHIVIST
        ));
    }

    #[test]
    fn test_validate_role_assignment_coordinator_sufficient() {
        assert!(validate_role_assignment(ROLE_COORDINATOR, 500).is_ok());
        assert!(validate_role_assignment(ROLE_COORDINATOR, 600).is_ok());
    }

    #[test]
    fn test_validate_role_assignment_coordinator_insufficient() {
        assert!(validate_role_assignment(ROLE_COORDINATOR, 499).is_err());
    }

    #[test]
    fn test_validate_role_assignment_validator_sufficient() {
        assert!(validate_role_assignment(ROLE_VALIDATOR, 300).is_ok());
        assert!(validate_role_assignment(ROLE_VALIDATOR, 500).is_ok());
    }

    #[test]
    fn test_validate_role_assignment_validator_insufficient() {
        assert!(validate_role_assignment(ROLE_VALIDATOR, 299).is_err());
    }

    #[test]
    fn test_validate_role_assignment_no_trust_required_roles() {
        // Executor and Relay have no minimum trust requirement
        assert!(validate_role_assignment(ROLE_EXECUTOR | ROLE_RELAY, 0).is_ok());
    }
}
