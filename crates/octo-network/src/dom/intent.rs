//! Overlay Intent Model — RFC-0857 §1

/// Intent type discriminants (RFC-0857 §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum IntentType {
    Transaction = 0x0001,
    MissionCommand = 0x0002,
    AIExecution = 0x0003,
    ConsensusVote = 0x0004,
    ProofSubmission = 0x0005,
    ResourceLease = 0x0006,
    GovernanceProposal = 0x0007,
    RelayCommitment = 0x0008,
}

/// Execution class hierarchy (RFC-0857 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum ExecutionClass {
    CriticalConsensus = 0x0000,
    Consensus = 0x0001,
    MissionCritical = 0x0002,
    Economic = 0x0003,
    Standard = 0x0004,
    Bulk = 0x0005,
    Archive = 0x0006,
}

/// Default ExecutionClass for each IntentType (RFC-0857 §6.1).
pub fn intent_type_to_class(intent_type: IntentType) -> ExecutionClass {
    match intent_type {
        IntentType::ConsensusVote => ExecutionClass::Consensus,
        IntentType::GovernanceProposal => ExecutionClass::Consensus,
        IntentType::MissionCommand => ExecutionClass::MissionCritical,
        IntentType::ProofSubmission => ExecutionClass::MissionCritical,
        IntentType::Transaction => ExecutionClass::Economic,
        IntentType::ResourceLease => ExecutionClass::Economic,
        IntentType::AIExecution => ExecutionClass::Standard,
        IntentType::RelayCommitment => ExecutionClass::Standard,
    }
}

/// OverlayIntent — the core data structure of DOM (RFC-0857 §1).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct OverlayIntent {
    /// BLAKE3-256 of intent contents (canonical id)
    pub intent_id: [u8; 32],
    /// Intent type discriminant
    pub intent_type: u16,
    /// Mission this intent belongs to
    pub mission_id: [u8; 32],
    /// Sender identity
    pub sender_id: [u8; 32],
    /// Monotonically increasing per (sender_id, mission_id)
    pub sequence: u64,
    /// Logical timestamp (DGP-ordered)
    pub logical_timestamp: u64,
    /// Intent expiration (logical_timestamp + domain_TTL)
    pub expiration: u64,
    /// BLAKE3-256 of payload (payload transmitted via DGP wrapper)
    pub payload_root: [u8; 32],
    /// Economic weight for fee calculation and ordering
    pub economic_weight: u64,
    /// Execution class discriminant
    pub execution_class: u16,
    /// Ed25519 signature over canonical intent bytes
    pub signature: [u8; 64],
}

impl OverlayIntent {
    /// Canonical signing bytes for Ed25519 signature verification.
    /// Excludes the signature field itself.
    pub fn to_signing_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(128);
        buf.extend_from_slice(&self.intent_id);
        buf.extend_from_slice(&self.intent_type.to_be_bytes());
        buf.extend_from_slice(&self.mission_id);
        buf.extend_from_slice(&self.sender_id);
        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.logical_timestamp.to_be_bytes());
        buf.extend_from_slice(&self.expiration.to_be_bytes());
        buf.extend_from_slice(&self.payload_root);
        buf.extend_from_slice(&self.economic_weight.to_be_bytes());
        buf.extend_from_slice(&self.execution_class.to_be_bytes());
        buf
    }
}

/// Default TTL per scope (RFC-0857 §1).
pub fn default_ttl_for_scope(scope: u16) -> u64 {
    match scope {
        0x0001 => 20, // GLOBAL
        0x0002 => 10, // REGIONAL
        0x0003 => 5,  // MISSION
        0x0004 => 3,  // PRIVATE
        0x0005 => 3,  // LOCAL
        0x0006 => 10, // CONSENSUS
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_type_repr() {
        assert_eq!(IntentType::Transaction as u16, 0x0001);
        assert_eq!(IntentType::RelayCommitment as u16, 0x0008);
    }

    #[test]
    fn test_execution_class_ordering() {
        assert!(ExecutionClass::CriticalConsensus < ExecutionClass::Consensus);
        assert!(ExecutionClass::Consensus < ExecutionClass::Archive);
    }

    #[test]
    fn test_intent_type_to_class() {
        assert_eq!(
            intent_type_to_class(IntentType::ConsensusVote),
            ExecutionClass::Consensus
        );
        assert_eq!(
            intent_type_to_class(IntentType::Transaction),
            ExecutionClass::Economic
        );
        assert_eq!(
            intent_type_to_class(IntentType::AIExecution),
            ExecutionClass::Standard
        );
    }

    #[test]
    fn test_default_ttl() {
        assert_eq!(default_ttl_for_scope(0x0001), 20); // GLOBAL
        assert_eq!(default_ttl_for_scope(0x0003), 5); // MISSION
        assert_eq!(default_ttl_for_scope(0x0004), 3); // PRIVATE
    }
}
