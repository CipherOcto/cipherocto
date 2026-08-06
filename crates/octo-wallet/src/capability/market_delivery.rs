// RFC-0959-A1 §Data Structures: market delivery envelope types.
//
// 6 structs + 1 enum + 1 newtype. All have manual redacting Debug impls
// per RFC-0957-A1 §Security. The BearerCapsule 3-field shape is the
// authoritative RFC version (not the mission text's 6-field shortform).
//
// `BearerCapsule` itself lives in `quota-router-storage::bearer_capsule_stub`
// (strategic placement: storage crate owns cipherocto-side persistence;
// wallet crate consumes the type via re-export).

use serde::{Deserialize, Serialize};

use super::bearer_capsule_re_export::BearerCapsule;

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

/// Role tag (typed enum, not string) — RFC-0959-A1 §Data Structures.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoleTag {
    Buyer,
    Seller,
    Router,
}

impl std::fmt::Debug for RoleTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Buyer => "Buyer",
            Self::Seller => "Seller",
            Self::Router => "Router",
        })
    }
}

/// DealSettled payload (signed). Hash chain input.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealSettledPayload {
    #[serde(with = "serde_bytes_32")]
    pub prev_chain_hash: [u8; 32],
    pub buyer_did: String,
    pub seller_did: String,
    #[serde(with = "serde_bytes_32")]
    pub ask_id: [u8; 32],
    #[serde(with = "serde_bytes_32")]
    pub bearer_capsule_hash: [u8; 32],
    #[serde(with = "serde_bytes_32")]
    pub cap_root_hash: [u8; 32],
    pub settled_at_unix: u64,
    pub role_tag: RoleTag,
}

impl std::fmt::Debug for DealSettledPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DealSettledPayload")
            .field("prev_chain_hash", &"<redacted 32 bytes>")
            .field("buyer_did", &self.buyer_did)
            .field("seller_did", &self.seller_did)
            .field("ask_id", &"<redacted 32 bytes>")
            .field("bearer_capsule_hash", &"<redacted 32 bytes>")
            .field("cap_root_hash", &"<redacted 32 bytes>")
            .field("settled_at_unix", &self.settled_at_unix)
            .field("role_tag", &self.role_tag)
            .finish()
    }
}

/// Signed DealSettled event. Joins Ask + SettlementEvent + SettlementReceipt.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealSettled {
    #[serde(with = "serde_bytes_32")]
    pub event_hash: [u8; 32],
    pub payload: DealSettledPayload,
    #[serde(with = "serde_bytes_64")]
    pub seller_signature: [u8; 64],
}

impl std::fmt::Debug for DealSettled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DealSettled")
            .field("event_hash", &"<redacted 32 bytes>")
            .field("payload", &self.payload)
            .field("seller_signature", &"<redacted 64 bytes>")
            .finish()
    }
}

/// Preimage for envelope_id derivation. `envelope_id` field is always zero
/// in the preimage (R10-N8 fix: avoid self-referential hash).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDeliveryEnvelopePreimage {
    #[serde(with = "serde_bytes_32")]
    pub envelope_id: [u8; 32],
    pub bearer: BearerCapsule,
    pub capability_token: String,
    pub deal_settled: DealSettled,
    pub created_at_unix: u64,
}

impl std::fmt::Debug for MarketDeliveryEnvelopePreimage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketDeliveryEnvelopePreimage")
            .field("envelope_id", &"<redacted 32 bytes>")
            .field("bearer", &"<redacted>")
            .field("capability_token", &"<redacted>")
            .field("deal_settled", &self.deal_settled)
            .field("created_at_unix", &self.created_at_unix)
            .finish()
    }
}

/// MarketDeliveryEnvelope (RFC-0959-A1 §Data Structures).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDeliveryEnvelope {
    #[serde(with = "serde_bytes_32")]
    pub envelope_id: [u8; 32],
    pub bearer: BearerCapsule,
    pub capability_token: String,
    pub deal_settled: DealSettled,
    pub created_at_unix: u64,
}

impl std::fmt::Debug for MarketDeliveryEnvelope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketDeliveryEnvelope")
            .field("envelope_id", &"<redacted 32 bytes>")
            .field("bearer", &self.bearer)
            .field("capability_token", &"<redacted>")
            .field("deal_settled", &self.deal_settled)
            .field("created_at_unix", &self.created_at_unix)
            .finish()
    }
}

/// Newtype for HashSet storage (RFC-0959-A1 §Data Structures).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnvelopeId(#[serde(with = "serde_bytes_32")] pub [u8; 32]);

impl EnvelopeId {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for EnvelopeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EnvelopeId")
            .field(&"<redacted 32 bytes>")
            .finish()
    }
}

/// DeliveryError (RFC-0959-A1 §Error Handling).
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error("chain-tip mismatch: expected <redacted 32 bytes>, actual <redacted 32 bytes>")]
    ChainTipMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    #[error("bearer insert failed: ask_id=<redacted 32 bytes>: {reason}")]
    BearerInsertFailed { ask_id: [u8; 32], reason: String },
    #[error("capability insert failed: ask_id=<redacted 32 bytes>: {reason}")]
    CapabilityInsertFailed { ask_id: [u8; 32], reason: String },
    /// Variant reserved for 0959-c (gossip retry loop).
    #[error("gossip failed after {attempts} attempts")]
    GossipFailed { attempts: u32 },
    #[error("replay detected: envelope_id=<redacted>")]
    ReplayDetected { envelope_id: EnvelopeId },
    #[error("chain hash broken: expected <redacted>, actual <redacted>")]
    ChainHashBroken {
        expected: [u8; 32],
        actual: [u8; 32],
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bearer() -> BearerCapsule {
        BearerCapsule::new([0x42; 32], vec![0x01, 0x02], [0x55; 64])
    }

    fn payload() -> DealSettledPayload {
        DealSettledPayload {
            prev_chain_hash: [0x00; 32],
            buyer_did: octo_ident::test_helpers::sample_did(237),
            seller_did: octo_ident::test_helpers::sample_did(106),
            ask_id: [0x33; 32],
            bearer_capsule_hash: [0x42; 32],
            cap_root_hash: [0x77; 32],
            settled_at_unix: 1_700_000_000_000,
            role_tag: RoleTag::Seller,
        }
    }

    #[test]
    fn envelope_id_hash_eq() {
        let a = EnvelopeId([0x01; 32]);
        let b = EnvelopeId([0x01; 32]);
        let c = EnvelopeId([0x02; 32]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        // Hash impl: usable in HashSet.
        let mut set = std::collections::HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn envelope_id_debug_is_redacted() {
        let e = EnvelopeId([0xFF; 32]);
        let s = format!("{e:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("FFFF"), "leaked bytes: {s}");
    }

    #[test]
    fn deal_settled_payload_debug_redacts_hashes() {
        let p = payload();
        let s = format!("{p:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("4242"), "leaked bearer_capsule_hash: {s}");
        assert!(!s.contains("3333"), "leaked ask_id: {s}");
        assert!(
            s.contains(&octo_ident::test_helpers::sample_did(106)),
            "DID should be visible: {s}"
        );
    }

    #[test]
    fn deal_settled_debug_redacts_signatures() {
        let d = DealSettled {
            event_hash: [0x11; 32],
            payload: payload(),
            seller_signature: [0x99; 64],
        };
        let s = format!("{d:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("9999"), "leaked signature bytes: {s}");
    }

    #[test]
    fn envelope_debug_redacts_capability_token() {
        let env = MarketDeliveryEnvelope {
            envelope_id: [0xAA; 32],
            bearer: bearer(),
            capability_token: "secret-token-bytes".into(),
            deal_settled: DealSettled {
                event_hash: [0x11; 32],
                payload: payload(),
                seller_signature: [0x99; 64],
            },
            created_at_unix: 1_700_000_000_000,
        };
        let s = format!("{env:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("secret-token"), "leaked capability_token: {s}");
    }

    #[test]
    fn delivery_error_variants_present() {
        let _ = DeliveryError::ChainTipMismatch {
            expected: [0; 32],
            actual: [1; 32],
        };
        let _ = DeliveryError::BearerInsertFailed {
            ask_id: [0x33; 32],
            reason: "test".into(),
        };
        let _ = DeliveryError::CapabilityInsertFailed {
            ask_id: [0x33; 32],
            reason: "test".into(),
        };
        let _ = DeliveryError::GossipFailed { attempts: 10 };
        let _ = DeliveryError::ReplayDetected {
            envelope_id: EnvelopeId([0xAA; 32]),
        };
        let _ = DeliveryError::ChainHashBroken {
            expected: [0; 32],
            actual: [1; 32],
        };
    }

    #[test]
    fn delivery_error_debug_redacts() {
        let e = DeliveryError::ReplayDetected {
            envelope_id: EnvelopeId([0xAA; 32]),
        };
        let s = format!("{e:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("AAAA"), "leaked envelope_id bytes: {s}");
    }
}
