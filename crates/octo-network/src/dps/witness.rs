//! Witness types for proof generation (RFC-0854 §6)

use crate::dps::DpsError;

/// A witness for proof generation.
///
/// Implementations provide the private inputs needed
/// to generate a proof for a specific circuit.
pub trait Witness: Send + Sync {
    /// Serialize the witness to canonical bytes.
    fn to_canonical_bytes(&self) -> Vec<u8>;

    /// Validate witness consistency before proving.
    fn validate(&self) -> Result<(), DpsError>;
}

/// A typed witness input with versioned serialization.
#[derive(Debug, Clone)]
pub struct WitnessInput {
    /// Circuit-specific input identifier
    pub input_id: [u8; 32],
    /// Serialized private inputs
    pub private_inputs: Vec<u8>,
    /// Serialized public inputs
    pub public_inputs: Vec<u8>,
    /// Input format version
    pub version: u32,
}

impl WitnessInput {
    /// Create a new witness input.
    pub fn new(input_id: [u8; 32], private_inputs: Vec<u8>, public_inputs: Vec<u8>) -> Self {
        Self {
            input_id,
            private_inputs,
            public_inputs,
            version: 1,
        }
    }

    /// Compute BLAKE3-256 hash of all inputs for commitment.
    pub fn commitment_hash(&self) -> [u8; 32] {
        use blake3::Hasher;
        let mut h = Hasher::new();
        h.update(&self.input_id);
        h.update(&self.private_inputs);
        h.update(&self.public_inputs);
        h.update(&self.version.to_be_bytes());
        *h.finalize().as_bytes()
    }

    /// Validate input sizes are non-zero.
    pub fn validate_sizes(&self) -> Result<(), DpsError> {
        if self.private_inputs.is_empty() {
            return Err(DpsError::WitnessGenerationFailed {
                reason: "empty private inputs",
            });
        }
        if self.public_inputs.is_empty() {
            return Err(DpsError::WitnessGenerationFailed {
                reason: "empty public inputs",
            });
        }
        Ok(())
    }
}

impl Witness for WitnessInput {
    fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.input_id);
        buf.extend_from_slice(&(self.private_inputs.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.private_inputs);
        buf.extend_from_slice(&(self.public_inputs.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.public_inputs);
        buf.extend_from_slice(&self.version.to_be_bytes());
        buf
    }

    fn validate(&self) -> Result<(), DpsError> {
        self.validate_sizes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_witness_input_new() {
        let wi = WitnessInput::new([1u8; 32], vec![10, 20], vec![30, 40]);
        assert_eq!(wi.version, 1);
        assert_eq!(wi.private_inputs, vec![10, 20]);
    }

    #[test]
    fn test_witness_input_commitment_hash_deterministic() {
        let wi = WitnessInput::new([1u8; 32], vec![10, 20], vec![30, 40]);
        let h1 = wi.commitment_hash();
        let h2 = wi.commitment_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_witness_input_commitment_hash_different_inputs() {
        let wi1 = WitnessInput::new([1u8; 32], vec![10], vec![20]);
        let wi2 = WitnessInput::new([2u8; 32], vec![10], vec![20]);
        assert_ne!(wi1.commitment_hash(), wi2.commitment_hash());
    }

    #[test]
    fn test_witness_input_validate_ok() {
        let wi = WitnessInput::new([1u8; 32], vec![10], vec![20]);
        assert!(wi.validate().is_ok());
    }

    #[test]
    fn test_witness_input_validate_empty_private() {
        let wi = WitnessInput::new([1u8; 32], vec![], vec![20]);
        assert!(wi.validate().is_err());
    }

    #[test]
    fn test_witness_input_validate_empty_public() {
        let wi = WitnessInput::new([1u8; 32], vec![10], vec![]);
        assert!(wi.validate().is_err());
    }

    #[test]
    fn test_witness_input_canonical_bytes_deterministic() {
        let wi = WitnessInput::new([1u8; 32], vec![10, 20], vec![30, 40]);
        let b1 = wi.to_canonical_bytes();
        let b2 = wi.to_canonical_bytes();
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_witness_input_canonical_bytes_different_inputs() {
        let wi1 = WitnessInput::new([1u8; 32], vec![10], vec![20]);
        let wi2 = WitnessInput::new([1u8; 32], vec![10], vec![30]);
        assert_ne!(wi1.to_canonical_bytes(), wi2.to_canonical_bytes());
    }

    #[test]
    fn test_witness_input_validate_sizes_ok() {
        let wi = WitnessInput::new([0u8; 32], vec![1, 2, 3], vec![4, 5, 6]);
        assert!(wi.validate_sizes().is_ok());
    }
}
