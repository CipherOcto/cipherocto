//! Governance / suspension / slash authorisation types.
//!
//! Per RFC-0968 §21: every authoritative signature or registration carries
//! one of these types and is shape-validated before any chain-side effect.
//! Real signature verification is stubbed (`verify_governance_suspension`,
//! `slash_recorder`) pending a later mission that owns governance key
//! provisioning. The shape, freshness, and quorum checks all land now so the
//! production signer can be swapped in later without API churn.

use serde::{Deserialize, Serialize};

use crate::constants::{
    BLAKE3_GOVERNANCE_SET_DOMAIN, GOVERNANCE_QUORUM, MAX_GOVERNANCE_SNAPSHOT_AGE_SECS,
};
use crate::error::ReputationError;
use crate::types::{RecorderDid, RecorderId};

/// A snapshot of the governance committee at a given moment. Every
/// authoritative proof references one of these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceSnapshot {
    pub finalized_at_unix: u64,
    pub governance_set_hash: [u8; 32],
    pub members: Vec<[u8; 32]>,
}

impl GovernanceSnapshot {
    pub fn age_secs(&self, now_unix: u64) -> u64 {
        now_unix.saturating_sub(self.finalized_at_unix)
    }

    pub fn is_fresh(&self, now_unix: u64) -> bool {
        self.age_secs(now_unix) <= MAX_GOVERNANCE_SNAPSHOT_AGE_SECS
    }

    pub fn quorum_count(&self) -> u32 {
        self.members.len() as u32
    }
}

/// Where slashed tokens are routed. RFC-0968 §21 amendment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SlashDestination {
    /// Protocol treasury.
    Treasury,
    /// Tokens are destroyed.
    Burn,
    /// Rewarded to the named validator DID.
    RewardValidator { did: RecorderDid },
}

impl SlashDestination {
    pub fn discriminant(self) -> u8 {
        match self {
            Self::Treasury => 0x01,
            Self::Burn => 0x02,
            Self::RewardValidator { .. } => 0x03,
        }
    }

    pub fn matches_field(self, field: u8) -> bool {
        self.discriminant() == field
    }
}

/// Asset tag for the slashed amount. RFC-0968 §21.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetTag {
    None = 0x00,
    Octo = 0x01,
    RoleToken = 0x02,
}

impl AssetTag {
    pub fn from_discriminant(d: u8) -> Result<Self, ReputationError> {
        Ok(match d {
            0x00 => Self::None,
            0x01 => Self::Octo,
            0x02 => Self::RoleToken,
            _other => return Err(ReputationError::ChainRefInvalid("asset_tag")),
        })
    }
}

/// Authorisation for suspending or slashing a recorder. Carries a fresh
/// governance snapshot + 3 distinct signatures (quorum). The signatures are
/// not verified here — that is the deferred responsibility of the signer
/// subsystem. The shape, freshness, and quorum checks are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceProof {
    /// Public key of the primary signer (32 bytes).
    pub governance_pubkey: [u8; 32],
    /// Recorder being suspended or slashed.
    pub recorder_id: RecorderId,
    /// BLAKE3 over the action payload — binds signature to reason.
    pub reason_hash: [u8; 32],
    /// Signature over `BLAKE3_GOVERNANCE_PROOF_DOMAIN || reason_hash`.
    pub signature: Vec<u8>,
    /// Snapshot under which this proof is valid.
    pub snapshot: GovernanceSnapshot,
    /// Hash of the governance set at snapshot time. Must match
    /// `snapshot.governance_set_hash`.
    pub governance_set_hash: [u8; 32],
    /// Slash-specific fields. `None` for suspension proofs.
    pub slash_destination: Option<SlashDestination>,
    pub slash_amount: u64,
    pub slash_asset: AssetTag,
}

/// Authorisation for `verify_governance_suspension` (read-side). Carries
/// `(auth: &SuspensionAuth, snapshot: &GovernanceSnapshot, now_unix)` per
/// the canonical `ReputationStore` trait signature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuspensionAuth {
    pub governance_pubkey: [u8; 32],
    pub recorder_id: RecorderId,
    pub reason_hash: [u8; 32],
    pub signature: Vec<u8>,
    pub snapshot: GovernanceSnapshot,
    pub governance_set_hash: [u8; 32],
}

/// 8-field ChainRef verification contract per RFC-0968 §21 Review Round 8.
/// Every recorder registration carries one. Each field must validate before
/// the 3-guard stake check is evaluated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainRef {
    pub chain_id: u32,
    pub block_height: u64,
    pub tx_hash: [u8; 32],
    pub recorder_did: RecorderDid,
    pub octo_stake: u64,
    pub role_stake: u64,
    pub role_token_kind: u32,
    pub lock_until_unix: u64,
}

impl ChainRef {
    /// 8-field validation. Each field has a structural rule; failure returns
    /// `ChainRefInvalid("field_name")`.
    pub fn verify(&self) -> Result<(), ReputationError> {
        if self.chain_id == 0 {
            return Err(ReputationError::ChainRefInvalid("chain_id"));
        }
        if self.block_height == 0 {
            return Err(ReputationError::ChainRefInvalid("block_height"));
        }
        if self.tx_hash == [0u8; 32] {
            return Err(ReputationError::ChainRefInvalid("tx_hash"));
        }
        if self.octo_stake == 0 {
            return Err(ReputationError::ChainRefInvalid("octo_stake"));
        }
        if self.role_stake == 0 {
            return Err(ReputationError::ChainRefInvalid("role_stake"));
        }
        if self.role_token_kind == 0 {
            return Err(ReputationError::ChainRefInvalid("role_token_kind"));
        }
        if self.lock_until_unix == 0 {
            return Err(ReputationError::ChainRefInvalid("lock_until_unix"));
        }
        Ok(())
    }
}

/// Compute the governance set hash under the governance-set domain.
/// `BLAKE3(BLAKE3_GOVERNANCE_SET_DOMAIN || sorted_member_pubkeys_concat)`.
pub fn governance_set_hash(members: &[[u8; 32]]) -> [u8; 32] {
    let mut sorted: Vec<[u8; 32]> = members.to_vec();
    sorted.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    hasher.update(BLAKE3_GOVERNANCE_SET_DOMAIN);
    for m in &sorted {
        hasher.update(m);
    }
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(out.as_bytes());
    arr
}

/// Required quorum size per amendment 24.
pub fn required_quorum() -> u32 {
    GOVERNANCE_QUORUM
}

// ---------------------------------------------------------------------------
// Attestor types — RFC-0968 §12 + amendments 22, 28
//
// An Attestor is a replication peer that signs `Attestation` records
// indicating it has observed a `SignalEvent` gossiped from another node.
// Attestors are NOT authoritative — the recorder's signature is the only
// authority for the event itself. Attestor signatures are transport
// metadata that boost a `reputation_event` from "seen by 1 node" to "seen
// by N nodes" for quorum purposes.
// ---------------------------------------------------------------------------

use crate::types::EventId;

/// 52-byte attestor DID, structurally identical to `RecorderDid` but kept
/// as a distinct newtype so the type system prevents a recorder from
/// passing as an attestor (or vice versa) without explicit conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttestorId(#[serde(with = "hex::serde")] [u8; 52]);

impl AttestorId {
    pub const fn from_array(arr: [u8; 52]) -> Self {
        Self(arr)
    }

    pub fn as_bytes(&self) -> &[u8; 52] {
        &self.0
    }
}

/// Lightweight attestor registration record per RFC-0968 §12 amendment
/// 22. Stored in the `reputation_attestors` table. `peer_set_id` is the
/// libp2p peer-set identifier; the same attestor may register multiple
/// peer-set IDs over its lifetime (e.g., after key rotation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttestorRegistration {
    /// Canonical attestor DID.
    pub attestor_did: AttestorId,
    /// ed25519 public key of the attestor.
    pub pubkey: [u8; 32],
    /// libp2p peer-set identifier (32 bytes, opaque).
    pub peer_set_id: [u8; 32],
    /// Unix seconds at registration request.
    pub requested_at_unix: u64,
    /// Unix seconds at registration finalization.
    pub registered_at_unix: u64,
}

/// Attestor authentication envelope carried in gossip frames. Real
/// signature verification is deferred to the signer subsystem; the
/// shape + freshness checks land now so the production signer can be
/// swapped in later without API churn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttestorAuth {
    /// ed25519 public key of the attestor.
    pub attestor_pubkey: [u8; 32],
    /// Attestor DID — must satisfy `attestor_did == derived from attestor_pubkey`.
    pub attestor_did: AttestorId,
    /// `BLAKE3(BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN || attestor_did || event_id || observed_at_unix)`.
    pub event_digest: [u8; 32],
    /// ed25519 signature over `BLAKE3(BLAKE3_REPUTATION_AUDIT_NONCE_DOMAIN || attestor_did || event_id || observed_at_unix)`.
    pub signature: Vec<u8>,
    /// Unix seconds when the attestor observed the event.
    pub observed_at_unix: u64,
    /// Source mission this attestation came from (cross-mission bridge).
    pub source_mission: String,
    /// Source domain within the source mission.
    pub source_domain: String,
}

/// A single attestation record — one row in `reputation_attestations`.
/// Records that a specific `AttestorId` observed `event_id` at a specific
/// `observed_at_unix`. Multiple attestors per event are stored as
/// multiple rows; the `attestor_quorum_reached` count distinct rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attestation {
    /// Storage-assigned monotonic attestation id.
    pub attestation_id: u64,
    /// Attestor that observed the event.
    pub attestor: AttestorId,
    /// Event being attested.
    pub event_id: EventId,
    /// ed25519 signature from the attestor.
    pub signature: Vec<u8>,
    /// Unix seconds when the attestor observed the event.
    pub observed_at_unix: u64,
    /// Unix seconds when the attestation was received by the local store.
    pub received_at_unix: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::MAX_GOVERNANCE_SNAPSHOT_AGE_SECS;

    fn dummy_snapshot(now: u64) -> GovernanceSnapshot {
        GovernanceSnapshot {
            finalized_at_unix: now,
            governance_set_hash: [1u8; 32],
            members: vec![[1u8; 32], [2u8; 32], [3u8; 32]],
        }
    }

    #[test]
    fn snapshot_age_zero_is_fresh() {
        let s = dummy_snapshot(1000);
        assert_eq!(s.age_secs(1000), 0);
        assert!(s.is_fresh(1000));
    }

    #[test]
    fn snapshot_stale_after_max_age() {
        let s = dummy_snapshot(1000);
        let stale = 1000 + MAX_GOVERNANCE_SNAPSHOT_AGE_SECS + 1;
        assert!(!s.is_fresh(stale));
    }

    #[test]
    fn quorum_is_three() {
        assert_eq!(required_quorum(), 3);
    }

    #[test]
    fn chain_ref_rejects_zero_chain_id() {
        let cr = ChainRef {
            chain_id: 0,
            block_height: 1,
            tx_hash: [1u8; 32],
            recorder_did: RecorderDid::from_array([0u8; 52]),
            octo_stake: 4000,
            role_stake: 1000,
            role_token_kind: 1,
            lock_until_unix: 9999999999,
        };
        let err = cr.verify().unwrap_err();
        assert_eq!(err.discriminant(), 0x29);
    }

    #[test]
    fn chain_ref_accepts_well_formed() {
        let cr = ChainRef {
            chain_id: 7,
            block_height: 100,
            tx_hash: [1u8; 32],
            recorder_did: RecorderDid::from_array([0u8; 52]),
            octo_stake: 4000,
            role_stake: 1000,
            role_token_kind: 1,
            lock_until_unix: 9_999_999_999,
        };
        assert!(cr.verify().is_ok());
    }

    #[test]
    fn governance_set_hash_is_order_independent() {
        let a = [[1u8; 32], [2u8; 32], [3u8; 32]];
        let b = [[3u8; 32], [1u8; 32], [2u8; 32]];
        assert_eq!(governance_set_hash(&a), governance_set_hash(&b));
    }
}
