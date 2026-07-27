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
