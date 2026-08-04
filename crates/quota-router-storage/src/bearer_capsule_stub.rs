//! `BearerCapsule` stub (RFC-0959-A1 §Data Structures).
//!
//! Mission 0957-c deviation: `HolderRecord::from_bearer(BearerCapsule, ...)`
//! requires `BearerCapsule`. RFC-0959-A1 defines the type, but the
//! implementation has not landed (mission 0959-a1 is open, not claimed).
//!
//! 0957-c ships a structural stub here so the `from_bearer` constructor
//! can compile. 0959-a1 owns the real type with full cryptographic semantics
//! (X25519 + ChaCha20-Poly1305 + Ed25519 signature).
//!
//! **Wire compatibility:** the stub uses the same 3-field shape that
//! RFC-0959-A1 §Data Structures mandates (`bearer_capsule_hash`, `encrypted_capsule`,
//! `seller_signature`). 0959-a1 may extend with additional fields; any
//! extension must coordinate via the `#[non_exhaustive]` marker.

/// Stub of RFC-0959-A1 §Data Structures `BearerCapsule`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BearerCapsule {
    /// 32-byte BLAKE3 hash of the encrypted capsule bytes.
    #[serde(with = "serde_bytes_32")]
    pub bearer_capsule_hash: [u8; 32],

    /// Capsule bytes (encrypted with buyer's encryption pubkey per RFC-0009).
    pub encrypted_capsule: Vec<u8>,

    /// 64-byte Ed25519 signature over the canonical_ser of the capsule bytes,
    /// signed by seller's identity.
    #[serde(with = "serde_bytes_64")]
    pub seller_signature: [u8; 64],
}

// Manual Debug redaction per RFC-0957-A1 §Security (cross-applied to the stub).
impl std::fmt::Debug for BearerCapsule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BearerCapsule")
            .field("bearer_capsule_hash", &"<redacted 32 bytes>")
            .field(
                "encrypted_capsule",
                &format_args!("<redacted {} bytes>", self.encrypted_capsule.len()),
            )
            .field("seller_signature", &"<redacted 64 bytes>")
            .finish()
    }
}

use serde::{Deserialize, Serialize};

/// Serde adapter for `[u8; 32]` via `serde_bytes::ByteArray`.
mod serde_bytes_32 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        serde_bytes::Bytes::new(bytes).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let v: serde_bytes::ByteArray<32> = serde_bytes::ByteArray::deserialize(d)?;
        Ok(v.into_array())
    }
}

/// Serde adapter for `[u8; 64]`.
mod serde_bytes_64 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        serde_bytes::Bytes::new(bytes).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: serde_bytes::ByteArray<64> = serde_bytes::ByteArray::deserialize(d)?;
        Ok(v.into_array())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_has_expected_field_count() {
        let c = BearerCapsule {
            bearer_capsule_hash: [0x42; 32],
            encrypted_capsule: vec![0x01, 0x02, 0x03],
            seller_signature: [0x55; 64],
        };
        assert_eq!(c.bearer_capsule_hash.len(), 32);
        assert_eq!(c.encrypted_capsule.len(), 3);
        assert_eq!(c.seller_signature.len(), 64);
    }

    #[test]
    fn debug_does_not_leak_credential_material() {
        let c = BearerCapsule {
            bearer_capsule_hash: [0x42; 32],
            encrypted_capsule: vec![0xAB; 100],
            seller_signature: [0xCD; 64],
        };
        let s = format!("{:?}", c);
        assert!(s.contains("redacted"), "expected redaction marker: {s}");
        assert!(!s.contains("4242"), "leaked hash bytes: {s}");
        assert!(!s.contains("ABAB"), "leaked capsule bytes: {s}");
        assert!(!s.contains("CDCD"), "leaked signature bytes: {s}");
    }
}
