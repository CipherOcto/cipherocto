//! Forwarding Proof (RFC-0860 §3.1)

#[allow(unused_imports)]
use serde::{Deserialize, Serialize};

mod serde_sig {
    use serde::{Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(sig: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        sig.as_slice().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: Vec<u8> = serde_bytes::deserialize(d)?;
        v.try_into()
            .map_err(|_| serde::de::Error::custom("expected exactly 64 bytes for signature"))
    }
}

/// Forwarding Proof — proves a gateway correctly forwarded an envelope.
///
/// Privacy: reveals only envelope_hash (not payload) and destination (gateway ID).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[repr(C)]
pub struct ForwardingProof {
    /// Gateway that performed the relay
    pub relay_gateway: [u8; 32],
    /// BLAKE3-256 of the forwarded envelope (NOT the payload)
    pub envelope_hash: [u8; 32],
    /// Destination domain or next-hop gateway
    pub destination: [u8; 32],
    /// Timestamp of forwarding (logical, per RFC-0850)
    pub logical_timestamp: u64,
    /// Sequence number (monotonic per gateway)
    pub sequence: u64,
    /// BLAKE3-256(destination || logical_timestamp || sequence)
    pub commitment: [u8; 32],
    /// Ed25519 signature over (relay_gateway || envelope_hash || commitment)
    #[serde(with = "serde_sig")]
    pub signature: [u8; 64],
}

impl ForwardingProof {
    /// Compute commitment = BLAKE3-256(destination || logical_timestamp || sequence)
    pub fn compute_commitment(
        destination: &[u8; 32],
        logical_timestamp: u64,
        sequence: u64,
    ) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(destination);
        hasher.update(&logical_timestamp.to_be_bytes());
        hasher.update(&sequence.to_be_bytes());
        *hasher.finalize().as_bytes()
    }

    /// Compute the message to sign: relay_gateway || envelope_hash || commitment
    pub fn signing_message(&self) -> [u8; 96] {
        let mut msg = [0u8; 96];
        msg[0..32].copy_from_slice(&self.relay_gateway);
        msg[32..64].copy_from_slice(&self.envelope_hash);
        msg[64..96].copy_from_slice(&self.commitment);
        msg
    }

    /// Verify the Ed25519 signature over the signing message.
    pub fn verify_signature(&self, public_key: &[u8; 32]) -> bool {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let vk = match VerifyingKey::from_bytes(public_key) {
            Ok(vk) => vk,
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&self.signature);
        vk.verify(&self.signing_message(), &sig).is_ok()
    }

    /// Verify the stored commitment against recomputed value.
    pub fn verify_commitment(&self) -> bool {
        let expected =
            Self::compute_commitment(&self.destination, self.logical_timestamp, self.sequence);
        self.commitment == expected
    }

    /// Full verification: signature + commitment + sequence monotonicity.
    pub fn verify_full(&self, public_key: &[u8; 32], previous_sequence: u64) -> bool {
        self.sequence > previous_sequence
            && self.verify_commitment()
            && self.verify_signature(public_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commitment_deterministic() {
        let dest = [0xAAu8; 32];
        let c1 = ForwardingProof::compute_commitment(&dest, 100, 1);
        let c2 = ForwardingProof::compute_commitment(&dest, 100, 1);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_commitment_different_inputs() {
        let dest = [0xAAu8; 32];
        let c1 = ForwardingProof::compute_commitment(&dest, 100, 1);
        let c2 = ForwardingProof::compute_commitment(&dest, 100, 2);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_signing_message_size() {
        let proof = ForwardingProof {
            relay_gateway: [0u8; 32],
            envelope_hash: [0u8; 32],
            destination: [0u8; 32],
            logical_timestamp: 0,
            sequence: 0,
            commitment: [0u8; 32],
            signature: [0u8; 64],
        };
        assert_eq!(proof.signing_message().len(), 96);
    }
}
