//! Receipt — signed canonical envelope after settlement (S04 Step 4).

use serde::{Deserialize, Serialize};

/// Receipt — proof that settlement completed successfully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// Settlement hash (BLAKE3 32 bytes).
    pub settlement_hash: [u8; 32],
    /// Router identity (signs the receipt).
    pub router_id: String,
    /// Ed25519 signature over `canonical_ser(settlement_hash || asker_did || holder_did || timestamp_unix)`.
    #[serde(with = "ed25519_sig_serde")]
    pub router_sig: ed25519_dalek::Signature,
    /// Unix timestamp.
    pub timestamp_unix: u64,
}

/// Ed25519 Signature serde shim.
mod ed25519_sig_serde {
    use ed25519_dalek::Signature;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(sig: &Signature, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_bytes(&sig.to_bytes())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Signature, D::Error> {
        let bytes: Vec<u8> = Deserialize::deserialize(de)?;
        Signature::from_slice(&bytes).map_err(serde::de::Error::custom)
    }
}

/// Canonical receipt bytes for signing.
#[must_use]
pub fn canonical_receipt_bytes(
    settlement_hash: &[u8; 32],
    asker_did: &str,
    holder_did: &str,
    timestamp: u64,
) -> Vec<u8> {
    let mut msg = Vec::with_capacity(32 + asker_did.len() + holder_did.len() + 8);
    msg.extend_from_slice(settlement_hash);
    msg.extend_from_slice(asker_did.as_bytes());
    msg.extend_from_slice(holder_did.as_bytes());
    msg.extend_from_slice(&timestamp.to_le_bytes());
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bytes_stable() {
        let h = [0xab; 32];
        let a = canonical_receipt_bytes(&h, "did:octo:a", "did:octo:h", 1_700_000_000);
        let b = canonical_receipt_bytes(&h, "did:octo:a", "did:octo:h", 1_700_000_000);
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_bytes_changes_with_timestamp() {
        let h = [0xab; 32];
        let a = canonical_receipt_bytes(&h, "did:octo:a", "did:octo:h", 1_700_000_000);
        let b = canonical_receipt_bytes(&h, "did:octo:a", "did:octo:h", 1_700_000_001);
        assert_ne!(a, b);
    }
}
