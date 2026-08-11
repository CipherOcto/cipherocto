//! Identifier newtypes + governance primitives for RFC-0862 v1.3 + v1.4
//! cross-instance DID coordination.
//!
//! Per RFC-0862 v1.3 §Specification §Substrate types. All newtypes are 32
//! (or 16) bytes wide to align with the surrounding crypto and identifier
//! substrate; `WriterNodeId` / `ShardMissionId` / `ShardKey` /
//! `OperatorId` are 32 bytes for BLAKE3-256 / ed25519 compatibility.
//!
//! `ChainId` is intentionally NOT redefined here. The canonical
//! `ChainId` newtype lives in `octo-ident` per RFC-0010 v1.4
//! §ChainId Namespace Extension (typed `ChainNamespace` + 17-byte
//! canonical form). Substrate types that bind to a chain reference
//! `octo_ident::ChainId` via the traits in `octo_sync::substrate::wal`
//! (v1.3 substrate extends through the trait surface, not the newtype
//! surface).

use borsh::{BorshDeserialize, BorshSerialize};

/// 32-byte writer node identifier (per RFC-0862 v1.3 §Substrate types).
///
/// Stable for the lifetime of a writer lease. Bound to the node's
/// long-term public key via `BLAKE3(public_key)` derivation at
/// registration time (out of scope for substrate; substrate consumes
/// the value as opaque 32 bytes).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize,
)]
pub struct WriterNodeId(pub [u8; 32]);

/// 32-byte shard mission identifier (per RFC-0862 v1.3 §Substrate types).
///
/// Per R12 H7: 32-byte width matches existing `octo-sync`'s
/// `pub type MissionId = [u8; 32]`. No truncation/derivation needed;
/// `WriterIdentity.mission_id` constructed directly from existing
/// `MissionId` via `ShardMissionId(mission_id.0)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ShardMissionId(pub [u8; 32]);

/// 32-byte shard key (per RFC-0862 v1.3 §Substrate types).
///
/// Derived from a canonical record key via `derive_canonical` (BLAKE3-256
/// over the canonical bytes). All writes to a shard MUST be addressed by
/// the same `ShardKey`; the writer-election substrate rejects
/// cross-shard writes at the WAL layer (entry.shard_key verification).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ShardKey(pub [u8; 32]);

impl ShardKey {
    /// Derive a shard key from canonical record-key bytes.
    ///
    /// Per RFC-0862 v1.3 §Substrate types: `ShardKey::derive_canonical`
    /// defines the canonical mapping. The `record_key_canonical` input
    /// MUST be the canonical encoding of the record key (e.g., the
    /// canonical DID form per RFC-0010 v1.0 / v1.3) — substrate does
    /// NOT enforce canonicalization; callers must pre-canonicalize.
    pub fn derive_canonical(record_key_canonical: &[u8]) -> Self {
        Self(*blake3::hash(record_key_canonical).as_bytes())
    }
}

/// 32-byte operator identifier (per RFC-0862 v1.3 §Specification
/// §Governance).
///
/// Identification of a governance operator who may co-sign a
/// `GovernanceAttestation`. The `pubkey()` method returns the embedded
/// 32-byte ed25519 public key for signature verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct OperatorId(pub [u8; 32]);

impl OperatorId {
    /// Return the embedded 32-byte ed25519 public key.
    pub fn pubkey(&self) -> [u8; 32] {
        self.0
    }
}

/// 32-byte operator identifier + 64-byte signature pair (per RFC-0862
/// v1.3 §Specification §Governance).
///
/// Borsh-serialised form binds `operator_id` and `signature` together
/// for `GovernanceAttestation.signatures` round-trips.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct OperatorSignature {
    /// Operator who co-signed the attestation.
    pub operator_id: OperatorId,
    /// 64-byte ed25519 signature over `governance_signature_message`.
    pub signature: [u8; 64],
}

/// M-of-N governance operator set (per RFC-0862 v1.3 §Substrate types).
///
/// Per R10 H9: `OperatorSet` uses sorted canonical serialization for
/// stable binding across serializations. `operators` is sorted +
/// deduplicated at construction time (per R11 M3 config-time
/// validation).
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct OperatorSet {
    /// Sorted (lexicographic by 32-byte id) + deduplicated operator list.
    pub operators: Vec<OperatorId>,
    /// M: number of signatures required. MUST be `1 <= M <= operators.len()`.
    pub threshold: usize,
}

impl OperatorSet {
    /// Construct a new `OperatorSet` with config-time validation.
    ///
    /// Per RFC-0862 v1.3 R11 M3: sorts operators lexicographically,
    /// deduplicates, and validates `1 <= threshold <= operators.len()`.
    /// Returns `ConfigError::InvalidThreshold` on bad threshold.
    pub fn new(mut operators: Vec<OperatorId>, threshold: usize) -> Result<Self, ConfigError> {
        operators.sort_by_key(|o| o.0);
        operators.dedup();
        if threshold == 0 || threshold > operators.len() {
            return Err(ConfigError::InvalidThreshold {
                threshold,
                max: operators.len(),
            });
        }
        Ok(Self {
            operators,
            threshold,
        })
    }

    /// Canonical borsh serialization of the operator set.
    ///
    /// Per RFC-0862 v1.3 R10 H9: deterministic form for cross-instance
    /// signature binding. Used as input to governance signature
    /// verification (per R12 M23: deployment-binding via chain_id, but
    /// the operator set itself is also part of the canonical binding).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("OperatorSet serialization is infallible")
    }
}

/// Config-time validation errors (per RFC-0862 v1.3 §Supporting types
/// + error enums).
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// `threshold` is 0 or exceeds the operator-set size.
    #[error("invalid threshold: {threshold} > max {max}")]
    InvalidThreshold {
        /// The configured threshold.
        threshold: usize,
        /// The maximum valid threshold for this operator set (= operators.len()).
        max: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newtype_widths() {
        assert_eq!(std::mem::size_of::<WriterNodeId>(), 32);
        assert_eq!(std::mem::size_of::<ShardMissionId>(), 32);
        assert_eq!(std::mem::size_of::<ShardKey>(), 32);
        assert_eq!(std::mem::size_of::<OperatorId>(), 32);
    }

    #[test]
    fn shard_key_derive_canonical_deterministic() {
        let k1 = ShardKey::derive_canonical(b"did:oct:abc");
        let k2 = ShardKey::derive_canonical(b"did:oct:abc");
        assert_eq!(k1, k2);
        let k3 = ShardKey::derive_canonical(b"did:oct:xyz");
        assert_ne!(k1, k3);
    }

    #[test]
    fn operator_set_validates_threshold() {
        let ops = vec![
            OperatorId([0u8; 32]),
            OperatorId([1u8; 32]),
            OperatorId([2u8; 32]),
        ];
        assert!(OperatorSet::new(ops.clone(), 0).is_err());
        assert!(OperatorSet::new(ops.clone(), 4).is_err());
        let s = OperatorSet::new(ops.clone(), 2).unwrap();
        assert_eq!(s.threshold, 2);
        assert_eq!(s.operators.len(), 3);
    }

    #[test]
    fn operator_set_sorts_and_dedupes() {
        let ops = vec![
            OperatorId([2u8; 32]),
            OperatorId([0u8; 32]),
            OperatorId([2u8; 32]),
            OperatorId([1u8; 32]),
        ];
        let s = OperatorSet::new(ops, 1).unwrap();
        assert_eq!(s.operators[0].0, [0u8; 32]);
        assert_eq!(s.operators[1].0, [1u8; 32]);
        assert_eq!(s.operators[2].0, [2u8; 32]);
        assert_eq!(s.operators.len(), 3);
    }

    #[test]
    fn operator_set_canonical_bytes_stable() {
        let ops = vec![
            OperatorId([2u8; 32]),
            OperatorId([0u8; 32]),
            OperatorId([1u8; 32]),
        ];
        let s1 = OperatorSet::new(ops.clone(), 1).unwrap();
        // Construct again with same input — sorted form canonical.
        let s2 = OperatorSet::new(ops, 1).unwrap();
        assert_eq!(s1.canonical_bytes(), s2.canonical_bytes());
    }

    #[test]
    fn borsh_round_trips() {
        let wid = WriterNodeId([7u8; 32]);
        let bytes = borsh::to_vec(&wid).unwrap();
        let decoded: WriterNodeId = WriterNodeId::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded, wid);

        let sk = ShardKey::derive_canonical(b"x");
        let bytes = borsh::to_vec(&sk).unwrap();
        let decoded: ShardKey = ShardKey::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded, sk);

        let ops = vec![OperatorId([0u8; 32]), OperatorId([1u8; 32])];
        let s = OperatorSet::new(ops, 2).unwrap();
        let bytes = borsh::to_vec(&s).unwrap();
        let decoded: OperatorSet = OperatorSet::try_from_slice(&bytes).unwrap();
        assert_eq!(decoded.operators.len(), s.operators.len());
        assert_eq!(decoded.threshold, s.threshold);
    }
}
