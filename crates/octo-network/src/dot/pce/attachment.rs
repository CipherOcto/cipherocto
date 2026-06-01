//! Proof Type Registry and Attachment Protocol (RFC-0859 §4, §6)
//!
//! Maps proof types to verification functions and defines how proofs
//! attach to DOM intents.

use std::collections::BTreeMap;

use crate::dot::pce::proof_type::{ProofType, ProofTypeValue};
use crate::dot::pce::PceError;

/// Verification function signature.
///
/// Takes proof blob and public inputs, returns Ok(()) if valid.
pub type VerifyFn = fn(&[u8], &[[u8; 32]]) -> Result<(), PceError>;

/// Entry in the proof type registry.
#[derive(Debug, Clone)]
pub struct ProofTypeEntry {
    /// The proof type
    pub proof_type: ProofTypeValue,
    /// Human-readable name
    pub name: &'static str,
    /// Verification function
    pub verify_fn: VerifyFn,
    /// Whether this proof type requires aggregation
    pub requires_aggregation: bool,
}

/// Proof type registry — maps proof types to verification functions.
///
/// Uses BTreeMap for deterministic iteration order (Class A requirement).
#[derive(Debug, Clone)]
pub struct ProofTypeRegistry {
    entries: BTreeMap<u16, ProofTypeEntry>,
}

impl ProofTypeRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Register a proof type with its verification function.
    pub fn register(&mut self, entry: ProofTypeEntry) {
        self.entries.insert(entry.proof_type.as_u16(), entry);
    }

    /// Look up a proof type entry by raw u16 value.
    pub fn lookup(&self, proof_type: u16) -> Option<&ProofTypeEntry> {
        self.entries.get(&proof_type)
    }

    /// Check if a proof type is registered.
    pub fn is_registered(&self, proof_type: u16) -> bool {
        self.entries.contains_key(&proof_type)
    }

    /// Get the verification function for a proof type.
    pub fn get_verify_fn(&self, proof_type: u16) -> Option<VerifyFn> {
        self.entries.get(&proof_type).map(|e| e.verify_fn)
    }

    /// Number of registered proof types.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ProofTypeRegistry {
    fn default() -> Self {
        let mut reg = Self::new();

        // Register standard proof types with stub verification functions
        reg.register(ProofTypeEntry {
            proof_type: ProofTypeValue::Standard(ProofType::InferenceProof),
            name: "AI Inference Proof",
            verify_fn: stub_verify,
            requires_aggregation: false,
        });
        reg.register(ProofTypeEntry {
            proof_type: ProofTypeValue::Standard(ProofType::DatasetIntegrityProof),
            name: "Dataset Integrity Proof",
            verify_fn: stub_verify,
            requires_aggregation: false,
        });
        reg.register(ProofTypeEntry {
            proof_type: ProofTypeValue::Standard(ProofType::MissionExecutionProof),
            name: "Mission Execution Proof",
            verify_fn: stub_verify,
            requires_aggregation: false,
        });
        reg.register(ProofTypeEntry {
            proof_type: ProofTypeValue::Standard(ProofType::RelayProof),
            name: "Relay Proof",
            verify_fn: stub_verify,
            requires_aggregation: false,
        });
        reg.register(ProofTypeEntry {
            proof_type: ProofTypeValue::Standard(ProofType::ValidatorAttestation),
            name: "Validator Attestation",
            verify_fn: stub_verify,
            requires_aggregation: false,
        });
        reg.register(ProofTypeEntry {
            proof_type: ProofTypeValue::Standard(ProofType::AggregatedProof),
            name: "Aggregated Proof",
            verify_fn: stub_verify,
            requires_aggregation: true,
        });
        reg.register(ProofTypeEntry {
            proof_type: ProofTypeValue::Standard(ProofType::MembershipProof),
            name: "Membership Proof",
            verify_fn: stub_verify,
            requires_aggregation: false,
        });
        reg.register(ProofTypeEntry {
            proof_type: ProofTypeValue::Standard(ProofType::StateTransitionProof),
            name: "State Transition Proof",
            verify_fn: stub_verify,
            requires_aggregation: false,
        });

        reg
    }
}

/// Stub verification function — always returns Ok.
/// Replaced by actual backends when registered.
fn stub_verify(_proof_blob: &[u8], _public_inputs: &[[u8; 32]]) -> Result<(), PceError> {
    Ok(())
}

/// Proof attachment to a DOM intent (RFC-0859 §6).
///
/// An `IntentProofAttachment` binds a proof to a specific intent,
/// enabling verification that the proof is valid for that intent's type.
#[derive(Debug, Clone)]
pub struct IntentProofAttachment {
    /// The intent ID this proof is attached to
    pub intent_id: [u8; 32],
    /// The proof type (must match the intent's expected proof type)
    pub proof_type: u16,
    /// BLAKE3-256 commitment to the proof blob
    pub proof_commitment: [u8; 32],
    /// The proof blob itself
    pub proof_blob: Vec<u8>,
    /// Public inputs for verification
    pub public_inputs: Vec<[u8; 32]>,
}

impl IntentProofAttachment {
    /// Create a new proof attachment.
    pub fn new(
        intent_id: [u8; 32],
        proof_type: u16,
        proof_blob: Vec<u8>,
        public_inputs: Vec<[u8; 32]>,
    ) -> Self {
        let proof_commitment = *blake3::hash(&proof_blob).as_bytes();
        Self {
            intent_id,
            proof_type,
            proof_commitment,
            proof_blob,
            public_inputs,
        }
    }

    /// Verify the proof commitment matches the blob.
    pub fn verify_commitment(&self) -> bool {
        let expected = *blake3::hash(&self.proof_blob).as_bytes();
        self.proof_commitment == expected
    }

    /// Validate this attachment against a proof type registry.
    ///
    /// Checks:
    /// 1. Proof type is registered
    /// 2. Proof commitment matches blob
    /// 3. Proof blob is non-empty
    /// 4. Public inputs are non-empty
    pub fn validate(&self, registry: &ProofTypeRegistry) -> Result<(), PceError> {
        if !registry.is_registered(self.proof_type) {
            return Err(PceError::UnsupportedSystem(self.proof_type));
        }
        if !self.verify_commitment() {
            return Err(PceError::CommitmentMismatch);
        }
        if self.proof_blob.is_empty() {
            return Err(PceError::MalformedProof("empty proof_blob".into()));
        }
        if self.public_inputs.is_empty() {
            return Err(PceError::MalformedProof("empty public_inputs".into()));
        }
        Ok(())
    }

    /// Verify the proof using the registry's verification function.
    pub fn verify(&self, registry: &ProofTypeRegistry) -> Result<(), PceError> {
        self.validate(registry)?;
        let verify_fn = registry
            .get_verify_fn(self.proof_type)
            .ok_or(PceError::UnsupportedSystem(self.proof_type))?;
        verify_fn(&self.proof_blob, &self.public_inputs)
    }
}

/// Validate that a proof type matches the expected type for an intent type.
///
/// Maps intent types to their expected proof types per RFC-0859 §6.
pub fn validate_proof_type_for_intent(intent_type: u16, proof_type: u16) -> Result<(), PceError> {
    let expected = expected_proof_type(intent_type);
    if expected != proof_type {
        return Err(PceError::MalformedProof(format!(
            "intent type {:#06x} expects proof type {:#06x}, got {:#06x}",
            intent_type, expected, proof_type
        )));
    }
    Ok(())
}

/// Get the expected proof type for a given intent type.
///
/// Per RFC-0859 §6.1 mapping table.
fn expected_proof_type(intent_type: u16) -> u16 {
    match intent_type {
        0x0001 => ProofType::InferenceProof as u16, // Transaction → Inference
        0x0002 => ProofType::MissionExecutionProof as u16, // MissionCommand → Execution
        0x0003 => ProofType::InferenceProof as u16, // AIExecution → Inference
        0x0004 => ProofType::ValidatorAttestation as u16, // ConsensusVote → Attestation
        0x0005 => ProofType::AggregatedProof as u16, // ProofSubmission → Aggregated
        0x0006 => ProofType::StateTransitionProof as u16, // ResourceLease → State
        0x0007 => ProofType::MembershipProof as u16, // GovernanceProposal → Membership
        0x0008 => ProofType::RelayProof as u16,     // RelayCommitment → Relay
        _ => ProofType::StateTransitionProof as u16, // Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- ProofTypeRegistry tests --

    #[test]
    fn test_registry_default_has_all_standard_types() {
        let reg = ProofTypeRegistry::default();
        assert_eq!(reg.len(), 8);
        assert!(reg.is_registered(ProofType::InferenceProof as u16));
        assert!(reg.is_registered(ProofType::StateTransitionProof as u16));
    }

    #[test]
    fn test_registry_lookup() {
        let reg = ProofTypeRegistry::default();
        let entry = reg.lookup(ProofType::InferenceProof as u16).unwrap();
        assert_eq!(entry.name, "AI Inference Proof");
    }

    #[test]
    fn test_registry_custom_type() {
        let mut reg = ProofTypeRegistry::default();
        reg.register(ProofTypeEntry {
            proof_type: ProofTypeValue::Custom(0x8001),
            name: "Custom Proof",
            verify_fn: stub_verify,
            requires_aggregation: false,
        });
        assert!(reg.is_registered(0x8001));
        assert_eq!(reg.len(), 9);
    }

    #[test]
    fn test_registry_get_verify_fn() {
        let reg = ProofTypeRegistry::default();
        assert!(reg
            .get_verify_fn(ProofType::InferenceProof as u16)
            .is_some());
        assert!(reg.get_verify_fn(0x0099).is_none());
    }

    // -- ProofTypeValue tests --

    #[test]
    fn test_proof_type_value_standard() {
        let val = ProofTypeValue::from_u16(0x0001);
        assert_eq!(val, ProofTypeValue::Standard(ProofType::InferenceProof));
        assert_eq!(val.as_u16(), 0x0001);
    }

    #[test]
    fn test_proof_type_value_custom() {
        let val = ProofTypeValue::from_u16(0x8001);
        assert_eq!(val, ProofTypeValue::Custom(0x8001));
        assert_eq!(val.as_u16(), 0x8001);
    }

    // -- IntentProofAttachment tests --

    #[test]
    fn test_attachment_new() {
        let blob = vec![1, 2, 3];
        let att = IntentProofAttachment::new(
            [0xAA; 32],
            ProofType::InferenceProof as u16,
            blob.clone(),
            vec![[0xBB; 32]],
        );
        assert_eq!(att.intent_id, [0xAA; 32]);
        assert_eq!(att.proof_type, ProofType::InferenceProof as u16);
        assert_eq!(att.proof_commitment, *blake3::hash(&blob).as_bytes());
    }

    #[test]
    fn test_attachment_verify_commitment() {
        let att = IntentProofAttachment::new(
            [0xAA; 32],
            ProofType::InferenceProof as u16,
            vec![1, 2, 3],
            vec![[0xBB; 32]],
        );
        assert!(att.verify_commitment());
    }

    #[test]
    fn test_attachment_verify_commitment_tampered() {
        let mut att = IntentProofAttachment::new(
            [0xAA; 32],
            ProofType::InferenceProof as u16,
            vec![1, 2, 3],
            vec![[0xBB; 32]],
        );
        att.proof_commitment = [0xFF; 32];
        assert!(!att.verify_commitment());
    }

    #[test]
    fn test_attachment_validate_ok() {
        let reg = ProofTypeRegistry::default();
        let att = IntentProofAttachment::new(
            [0xAA; 32],
            ProofType::InferenceProof as u16,
            vec![1, 2, 3],
            vec![[0xBB; 32]],
        );
        assert!(att.validate(&reg).is_ok());
    }

    #[test]
    fn test_attachment_validate_unsupported_type() {
        let reg = ProofTypeRegistry::default();
        let att = IntentProofAttachment::new(
            [0xAA; 32],
            0x0099, // unsupported
            vec![1, 2, 3],
            vec![[0xBB; 32]],
        );
        assert!(att.validate(&reg).is_err());
    }

    #[test]
    fn test_attachment_validate_empty_blob() {
        let reg = ProofTypeRegistry::default();
        let att = IntentProofAttachment::new(
            [0xAA; 32],
            ProofType::InferenceProof as u16,
            vec![],
            vec![[0xBB; 32]],
        );
        assert!(att.validate(&reg).is_err());
    }

    #[test]
    fn test_attachment_verify_ok() {
        let reg = ProofTypeRegistry::default();
        let att = IntentProofAttachment::new(
            [0xAA; 32],
            ProofType::InferenceProof as u16,
            vec![1, 2, 3],
            vec![[0xBB; 32]],
        );
        // stub_verify always returns Ok
        assert!(att.verify(&reg).is_ok());
    }

    // -- Proof type for intent validation tests --

    #[test]
    fn test_validate_proof_type_for_intent_valid() {
        // Transaction (0x0001) expects InferenceProof (0x0001)
        assert!(validate_proof_type_for_intent(0x0001, ProofType::InferenceProof as u16).is_ok());
    }

    #[test]
    fn test_validate_proof_type_for_intent_mismatch() {
        // Transaction (0x0001) does NOT expect RelayProof (0x0004)
        assert!(validate_proof_type_for_intent(0x0001, ProofType::RelayProof as u16).is_err());
    }

    #[test]
    fn test_validate_proof_type_all_intent_types() {
        // All 8 intent types should have a mapping
        for intent_type in 1..=8u16 {
            assert!(
                validate_proof_type_for_intent(intent_type, expected_proof_type(intent_type))
                    .is_ok(),
                "intent type {:#06x} should map to a proof type",
                intent_type
            );
        }
    }

    #[test]
    fn test_expected_proof_type_unknown() {
        // Unknown intent type maps to StateTransitionProof
        assert_eq!(
            expected_proof_type(0x0099),
            ProofType::StateTransitionProof as u16
        );
    }
}
