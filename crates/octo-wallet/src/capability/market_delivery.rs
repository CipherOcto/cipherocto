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

/// Role tag (typed enum, not string) — RFC-0959-A1 §Data Structures +
/// RFC-0971 §Roles alignment (mission 0959-b1 AC-D2).
///
/// Variants aligned to RFC-0971 canonical set:
/// `Asker` (was `Buyer` — RFC-0971 role-binding), `TokenIssuer` (was
/// `Seller` — RFC-0971 role-binding), `Router` (unchanged).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoleTag {
    Asker,
    TokenIssuer,
    Router,
}

impl std::fmt::Debug for RoleTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Asker => "Asker",
            Self::TokenIssuer => "TokenIssuer",
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

/// DeliveryError (RFC-0959-A1 §Error Handling — full 14-variant cascade
/// per mission 0959-b1 AC-D1).
#[derive(thiserror::Error)]
pub enum DeliveryError {
    #[error("chain-tip mismatch: expected {expected:?}, actual {actual:?}")]
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
    #[error("chain hash broken: expected {expected:?}, actual {actual:?}")]
    ChainHashBroken {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    // --- 0959-b1 AC-D1 additions: 8 variants + SettlementChainError wrapper ---
    #[error("ask not found: ask_id=<redacted 32 bytes>")]
    AskNotFound { ask_id: [u8; 32] },
    #[error("gossip error after {attempts} attempts: {reason}")]
    GossipError { attempts: u32, reason: String },
    #[error(
        "invalid settled_at_unix: observed {observed}, expected window {expected_window_secs}s"
    )]
    InvalidSettledAtUnix {
        observed: u64,
        expected_window_secs: u64,
    },
    #[error("role binding mismatch: {role}")]
    RoleBindingMismatch { role: String },
    #[error("stoolap transaction error: {reason}")]
    StoolapTxnError { reason: String },
    #[error("stoolap database error: {reason}")]
    StoolapDbError { reason: String },
    #[error("CAS error: {reason}")]
    CasError { reason: String },
    #[error("outbox error: {reason}")]
    OutboxError { reason: String },
    #[error("chain error: {reason}")]
    ChainError { reason: String },
    #[error("serialization error: {reason}")]
    SerializationError { reason: String },
    #[error("registry error: {reason}")]
    RegistryError { reason: String },
    #[error("chain append error: expected_hash {expected_hash:?}, actual_hash {actual_hash:?}")]
    ChainAppendError {
        expected_hash: [u8; 32],
        actual_hash: [u8; 32],
    },
    /// `SettlementChainError` 4 sub-variants wrapped for delivery error
    /// cascade (RFC-0959-A1 §Error Handling).
    #[error("settlement chain error: {0}")]
    SettlementChainError(SettlementChainError),
}

/// `SettlementChainError` (RFC-0959-A1 §Error Handling — settlement
/// chain cascade per mission 0959-b1 AC-D1).
#[derive(thiserror::Error, Clone, PartialEq, Eq)]
pub enum SettlementChainError {
    #[error("settlement chain tip mismatch: expected {expected:?}, actual {actual:?}")]
    TipMismatch {
        expected: [u8; 32],
        actual: [u8; 32],
    },
    #[error("settlement chain append failed: {reason}")]
    AppendFailed { reason: String },
    #[error("settlement chain reorg detected at height {height}")]
    ReorgDetected { height: u64 },
    #[error("settlement chain unknown parent: parent_hash {parent_hash:?}")]
    UnknownParent { parent_hash: [u8; 32] },
}

// Manual redacting Debug (RFC-0959-A1 §Security + mission 0959-b1 AC-D1):
// credential material (ask_id, expected/actual hashes) is redacted;
// operational metadata (attempts, reason, observed/expected_window_secs,
// role name, error kind) is preserved for forensics.
impl std::fmt::Debug for DeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChainTipMismatch { expected, actual } => f
                .debug_struct("ChainTipMismatch")
                .field("expected", expected)
                .field("actual", actual)
                .finish(),
            Self::BearerInsertFailed { reason, .. } => f
                .debug_struct("BearerInsertFailed")
                .field("ask_id", &"<redacted 32 bytes>")
                .field("reason", reason)
                .finish(),
            Self::CapabilityInsertFailed { reason, .. } => f
                .debug_struct("CapabilityInsertFailed")
                .field("ask_id", &"<redacted 32 bytes>")
                .field("reason", reason)
                .finish(),
            Self::GossipFailed { attempts } => f
                .debug_struct("GossipFailed")
                .field("attempts", attempts)
                .finish(),
            Self::ReplayDetected { .. } => f
                .debug_struct("ReplayDetected")
                .field("envelope_id", &"<redacted>")
                .finish(),
            Self::ChainHashBroken { expected, actual } => f
                .debug_struct("ChainHashBroken")
                .field("expected", expected)
                .field("actual", actual)
                .finish(),
            Self::AskNotFound { .. } => f
                .debug_struct("AskNotFound")
                .field("ask_id", &"<redacted 32 bytes>")
                .finish(),
            Self::GossipError { attempts, reason } => f
                .debug_struct("GossipError")
                .field("attempts", attempts)
                .field("reason", reason)
                .finish(),
            Self::InvalidSettledAtUnix {
                observed,
                expected_window_secs,
            } => f
                .debug_struct("InvalidSettledAtUnix")
                .field("observed", observed)
                .field("expected_window_secs", expected_window_secs)
                .finish(),
            Self::RoleBindingMismatch { role } => f
                .debug_struct("RoleBindingMismatch")
                .field("role", role)
                .finish(),
            Self::StoolapTxnError { reason } => f
                .debug_struct("StoolapTxnError")
                .field("reason", reason)
                .finish(),
            Self::StoolapDbError { reason } => f
                .debug_struct("StoolapDbError")
                .field("reason", reason)
                .finish(),
            Self::CasError { reason } => {
                f.debug_struct("CasError").field("reason", reason).finish()
            }
            Self::OutboxError { reason } => f
                .debug_struct("OutboxError")
                .field("reason", reason)
                .finish(),
            Self::ChainError { reason } => f
                .debug_struct("ChainError")
                .field("reason", reason)
                .finish(),
            Self::SerializationError { reason } => f
                .debug_struct("SerializationError")
                .field("reason", reason)
                .finish(),
            Self::RegistryError { reason } => f
                .debug_struct("RegistryError")
                .field("reason", reason)
                .finish(),
            Self::ChainAppendError {
                expected_hash,
                actual_hash,
            } => f
                .debug_struct("ChainAppendError")
                .field("expected_hash", expected_hash)
                .field("actual_hash", actual_hash)
                .finish(),
            Self::SettlementChainError(e) => {
                f.debug_tuple("SettlementChainError").field(e).finish()
            }
        }
    }
}

impl std::fmt::Debug for SettlementChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TipMismatch { expected, actual } => f
                .debug_struct("TipMismatch")
                .field("expected", expected)
                .field("actual", actual)
                .finish(),
            Self::AppendFailed { reason } => f
                .debug_struct("AppendFailed")
                .field("reason", reason)
                .finish(),
            Self::ReorgDetected { height } => f
                .debug_struct("ReorgDetected")
                .field("height", height)
                .finish(),
            Self::UnknownParent { parent_hash } => f
                .debug_struct("UnknownParent")
                .field("parent_hash", parent_hash)
                .finish(),
        }
    }
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
            role_tag: RoleTag::TokenIssuer,
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

    /// **F6 (Round 1 fix):** chain hashes are operational forensics
    /// (public chain-state data, not credential material). Operators MUST
    /// see them to reconcile forks. `ChainTipMismatch` + `ChainHashBroken`
    /// preserve hashes in Debug; only `ask_id` / `envelope_id` /
    /// `bearer_capsule_hash` / `cap_root_hash` are credential material
    /// and stay redacted.
    #[test]
    fn delivery_error_chain_tip_mismatch_preserves_hashes() {
        let e = DeliveryError::ChainTipMismatch {
            expected: [0xAA; 32],
            actual: [0xBB; 32],
        };
        let s = format!("{e:?}");
        assert!(s.contains("ChainTipMismatch"));
        // Debug renders [u8; 32] as decimal byte values — `0xAA = 170`,
        // `0xBB = 187`. Assert no redaction marker present (chain hashes
        // preserved) AND a non-trivial value is visible (not all zeros).
        assert!(
            !s.contains("redacted"),
            "chain hashes MUST NOT be redacted: {s}"
        );
        assert!(s.contains("170"), "expected hash preserved: {s}");
        assert!(s.contains("187"), "actual hash preserved: {s}");
    }

    #[test]
    fn delivery_error_chain_hash_broken_preserves_hashes() {
        let e = DeliveryError::ChainHashBroken {
            expected: [0xAA; 32],
            actual: [0xBB; 32],
        };
        let s = format!("{e:?}");
        assert!(s.contains("ChainHashBroken"));
        assert!(
            !s.contains("redacted"),
            "chain hashes MUST NOT be redacted: {s}"
        );
        assert!(s.contains("170"), "expected hash preserved: {s}");
        assert!(s.contains("187"), "actual hash preserved: {s}");
    }

    #[test]
    fn delivery_error_chain_append_error_preserves_hashes() {
        let e = DeliveryError::ChainAppendError {
            expected_hash: [0xCC; 32],
            actual_hash: [0xDD; 32],
        };
        let s = format!("{e:?}");
        assert!(s.contains("ChainAppendError"));
        assert!(
            !s.contains("redacted"),
            "chain hashes MUST NOT be redacted: {s}"
        );
        // 0xCC = 204, 0xDD = 221
        assert!(s.contains("204"), "expected_hash preserved: {s}");
        assert!(s.contains("221"), "actual_hash preserved: {s}");
    }

    #[test]
    fn settlement_chain_error_preserves_hashes() {
        let tip = SettlementChainError::TipMismatch {
            expected: [0xEE; 32],
            actual: [0xFF; 32],
        };
        let s = format!("{tip:?}");
        assert!(
            !s.contains("redacted"),
            "chain hashes MUST NOT be redacted: {s}"
        );
        // 0xEE = 238, 0xFF = 255
        assert!(s.contains("238"), "expected preserved: {s}");
        assert!(s.contains("255"), "actual preserved: {s}");

        let unk = SettlementChainError::UnknownParent {
            parent_hash: [0x11; 32],
        };
        let s = format!("{unk:?}");
        assert!(
            !s.contains("redacted"),
            "parent_hash MUST NOT be redacted: {s}"
        );
        assert!(s.contains("17"), "parent_hash preserved: {s}");
    }

    // --- 0959-b1 AC-D1: new DeliveryError variants Debug redaction ---

    #[test]
    fn delivery_error_ask_not_found_redacts_ask_id() {
        let e = DeliveryError::AskNotFound { ask_id: [0xCC; 32] };
        let s = format!("{e:?}");
        assert!(s.contains("AskNotFound"));
        assert!(s.contains("redacted"));
        assert!(!s.contains("cccccccc"), "leaked ask_id bytes: {s}");
    }

    #[test]
    fn delivery_error_invalid_settled_at_unix_preserves_metadata() {
        let e = DeliveryError::InvalidSettledAtUnix {
            observed: 1_700_000_000,
            expected_window_secs: 60,
        };
        let s = format!("{e:?}");
        assert!(s.contains("InvalidSettledAtUnix"));
        assert!(s.contains("1700000000"), "observed preserved: {s}");
        assert!(s.contains("60"), "expected_window_secs preserved: {s}");
    }

    #[test]
    fn delivery_error_settlement_chain_error_wraps() {
        let inner = SettlementChainError::ReorgDetected { height: 12345 };
        let outer = DeliveryError::SettlementChainError(inner);
        let s = format!("{outer:?}");
        assert!(s.contains("SettlementChainError"));
        assert!(s.contains("ReorgDetected"));
        assert!(s.contains("12345"), "height preserved: {s}");
    }

    #[test]
    fn settlement_chain_error_tip_mismatch_preserves_hashes() {
        // **F6 (Round 1 fix):** chain hashes are operational forensics,
        // not credential material. Operators MUST see them for fork
        // reconciliation. Renamed from `tip_mismatch_redacts_hashes`
        // (pre-fix) to `tip_mismatch_preserves_hashes` (post-fix).
        let e = SettlementChainError::TipMismatch {
            expected: [0xDD; 32],
            actual: [0xEE; 32],
        };
        let s = format!("{e:?}");
        assert!(s.contains("TipMismatch"));
        assert!(
            !s.contains("redacted"),
            "chain hashes MUST NOT be redacted: {s}"
        );
        // 0xDD = 221, 0xEE = 238
        assert!(s.contains("221"), "expected hash MUST be preserved: {s}");
        assert!(s.contains("238"), "actual hash MUST be preserved: {s}");
    }

    // --- 0959-b1 AC-D2: RoleTag variants aligned to RFC-0971 ---

    #[test]
    fn role_tag_variants_aligned_to_rfc_0971() {
        // RFC-0959-A1 §Data Structures + RFC-0971 §Roles alignment:
        // Asker + TokenIssuer + Router. Buyer + Seller were renamed.
        assert_eq!(RoleTag::Asker, RoleTag::Asker);
        assert_eq!(RoleTag::TokenIssuer, RoleTag::TokenIssuer);
        assert_eq!(RoleTag::Router, RoleTag::Router);
        assert_ne!(RoleTag::Asker, RoleTag::TokenIssuer);
        assert_ne!(RoleTag::TokenIssuer, RoleTag::Router);
    }

    #[test]
    fn role_tag_debug_preserves_variant_name() {
        assert_eq!(format!("{:?}", RoleTag::Asker), "Asker");
        assert_eq!(format!("{:?}", RoleTag::TokenIssuer), "TokenIssuer");
        assert_eq!(format!("{:?}", RoleTag::Router), "Router");
    }

    #[test]
    fn payload_role_tag_field_accepts_rfc_0971_variants() {
        let p = DealSettledPayload {
            prev_chain_hash: [0; 32],
            buyer_did: octo_ident::test_helpers::sample_did(1),
            seller_did: octo_ident::test_helpers::sample_did(2),
            ask_id: [0; 32],
            bearer_capsule_hash: [0; 32],
            cap_root_hash: [0; 32],
            settled_at_unix: 0,
            role_tag: RoleTag::TokenIssuer,
        };
        assert_eq!(p.role_tag, RoleTag::TokenIssuer);
    }
}
