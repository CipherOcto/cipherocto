//! Cross-domain DC slash (mission 0855p-c-cross-domain-slash).
//!
//! When a DomainCoordinator misbehaves, the mission-level
//! coordinator slashes the DC. The slash is recorded in the DC's
//! cross-domain reputation.
//!
//! ## Slash reason code
//!
//! - `0x000F` = `domain_coordinator_misbehavior`
//! - Sub-codes:
//!   - `0x000F.01` = `invalid_bind_envelope`
//!   - `0x000F.02` = `failed_attest`
//!   - `0x000F.03` = `censored_legit_member`
//!   - `0x000F.04` = `signed_malicious_envelope`
//!
//! ## Cool-down
//!
//! `2^slash_count` epochs.

use serde::{Deserialize, Serialize};

/// 0x000F slash reason code (mission 0855p-c-cross-domain-slash).
pub const DC_SLASH_REASON_DOMAIN_COORDINATOR_MISBEHAVIOR: u16 = 0x000F;

/// DC misbehavior sub-codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum DcSlashSubCode {
    /// Signed a BIND that violated the binding rules.
    InvalidBindEnvelope = 0x0001,
    /// Didn't respond to ATTEST_CHALLENGE within CHALLENGE_RESPONSE_EPOCHS.
    FailedAttest = 0x0002,
    /// Refused to sign a legitimate admission.
    CensoredLegitMember = 0x0003,
    /// Signed an envelope that violated the mission's policy.
    SignedMaliciousEnvelope = 0x0004,
}

/// The reason for a DC slash (combines code + sub-code).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DcMisbehavior {
    InvalidBindEnvelope,
    FailedAttest,
    CensoredLegitMember,
    SignedMaliciousEnvelope,
}

impl DcMisbehavior {
    pub fn sub_code(self) -> DcSlashSubCode {
        match self {
            DcMisbehavior::InvalidBindEnvelope => DcSlashSubCode::InvalidBindEnvelope,
            DcMisbehavior::FailedAttest => DcSlashSubCode::FailedAttest,
            DcMisbehavior::CensoredLegitMember => DcSlashSubCode::CensoredLegitMember,
            DcMisbehavior::SignedMaliciousEnvelope => DcSlashSubCode::SignedMaliciousEnvelope,
        }
    }
}

/// A `DC_SLASH` envelope (mission-level coordinator slashes a DC).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DcSlashEnvelope {
    pub dc_pubkey: Vec<u8>,
    pub slash_reason: u16,
    pub slash_reason_data: u32,
    /// The domains affected by this slash.
    pub domains: Vec<String>,
    /// Witness signatures (2/3 of mission-level witnesses).
    pub witness_signatures: Vec<Vec<u8>>,
    pub signed_at_epoch: u64,
}

impl DcSlashEnvelope {
    /// Build a slash envelope for the given misbehavior.
    pub fn new(
        dc_pubkey: Vec<u8>,
        misbehavior: DcMisbehavior,
        domains: Vec<String>,
        witness_signatures: Vec<Vec<u8>>,
        signed_at_epoch: u64,
    ) -> Self {
        let sub = misbehavior.sub_code() as u32;
        Self {
            dc_pubkey,
            slash_reason: DC_SLASH_REASON_DOMAIN_COORDINATOR_MISBEHAVIOR,
            slash_reason_data: ((DC_SLASH_REASON_DOMAIN_COORDINATOR_MISBEHAVIOR as u32) << 16) | sub,
            domains,
            witness_signatures,
            signed_at_epoch,
        }
    }

    /// Returns the misbehavior sub-code.
    pub fn misbehavior(&self) -> Option<DcMisbehavior> {
        if self.slash_reason != DC_SLASH_REASON_DOMAIN_COORDINATOR_MISBEHAVIOR {
            return None;
        }
        let sub = (self.slash_reason_data & 0xFFFF) as u16;
        match sub {
            0x0001 => Some(DcMisbehavior::InvalidBindEnvelope),
            0x0002 => Some(DcMisbehavior::FailedAttest),
            0x0003 => Some(DcMisbehavior::CensoredLegitMember),
            0x0004 => Some(DcMisbehavior::SignedMaliciousEnvelope),
            _ => None,
        }
    }
}

/// Errors from DC slash processing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DcSlashError {
    /// Insufficient witness signatures (need 2/3 of N).
    InsufficientWitnesses { provided: usize, required: usize },
    /// The DC pubkey is empty.
    EmptyDcPubkey,
    /// The slash reason is not 0x000F.
    InvalidSlashReason(u16),
    /// The slash_reason_data has an unrecognized sub-code.
    InvalidSlashReasonData(u32),
}

/// The outcome of a DC slash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DcSlashOutcome {
    pub dc_pubkey: Vec<u8>,
    pub domains_affected: usize,
    pub cool_down_epochs: u64,
    pub final_state: DcFinalState,
}

/// The final state of a slashed DC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DcFinalState {
    /// The DC enters cool-down; after cool-down, can re-stand.
    Cooldown,
    /// The DC has accumulated too many slashes and is permanently banned.
    PermanentBan,
    /// Appeal succeeded: reputation restored.
    Appealed,
}

/// Compute the cool-down for a DC based on its slash count.
/// `cool_down = 2^slash_count` epochs. At `slash_count >= 64`
/// the result would overflow u64, so we return `u64::MAX`
/// (effectively permanent).
pub fn cool_down_epochs(slash_count: u32) -> u64 {
    if slash_count >= 64 {
        return u64::MAX; // effectively permanent
    }
    1u64 << slash_count
}

/// Validate and process a DC slash envelope.
pub fn process_dc_slash(
    envelope: &DcSlashEnvelope,
    total_witnesses: usize,
    current_dc_slash_count: u32,
) -> Result<DcSlashOutcome, DcSlashError> {
    if envelope.dc_pubkey.is_empty() {
        return Err(DcSlashError::EmptyDcPubkey);
    }
    if envelope.slash_reason != DC_SLASH_REASON_DOMAIN_COORDINATOR_MISBEHAVIOR {
        return Err(DcSlashError::InvalidSlashReason(envelope.slash_reason));
    }
    // Reject envelopes with an unrecognized sub-code. Without
    // this check, an attacker who controls a 2/3 witness quorum
    // could submit slash_reason_data=0x0099 (no matching
    // DcMisbehavior variant) and the operator would have no
    // way to know what the slash was for.
    if envelope.misbehavior().is_none() {
        return Err(DcSlashError::InvalidSlashReasonData(
            envelope.slash_reason_data,
        ));
    }
    // 2/3 of total witnesses required.
    let required = (total_witnesses * 2).div_ceil(3);
    if envelope.witness_signatures.len() < required {
        return Err(DcSlashError::InsufficientWitnesses {
            provided: envelope.witness_signatures.len(),
            required,
        });
    }
    let cooldown = cool_down_epochs(current_dc_slash_count + 1);
    let final_state = if current_dc_slash_count + 1 >= 5 {
        DcFinalState::PermanentBan
    } else {
        DcFinalState::Cooldown
    };
    Ok(DcSlashOutcome {
        dc_pubkey: envelope.dc_pubkey.clone(),
        domains_affected: envelope.domains.len(),
        cool_down_epochs: cooldown,
        final_state,
    })
}

/// Build the gossip topic.
pub fn dc_slash_topic(dc_pubkey_hex: &str) -> String {
    format!("/dot/slash/dc/{dc_pubkey_hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_sub_code_roundtrip() {
        let env = DcSlashEnvelope::new(
            vec![0xAA],
            DcMisbehavior::FailedAttest,
            vec!["d1".into(), "d2".into()],
            vec![vec![0x01], vec![0x02]],
            1000,
        );
        assert_eq!(env.slash_reason, 0x000F);
        assert_eq!(env.misbehavior(), Some(DcMisbehavior::FailedAttest));
    }

    #[test]
    fn unknown_sub_code_returns_none() {
        let env = DcSlashEnvelope {
            dc_pubkey: vec![0xAA],
            slash_reason: 0x000F,
            slash_reason_data: 0x0099, // unknown
            domains: vec![],
            witness_signatures: vec![],
            signed_at_epoch: 0,
        };
        assert!(env.misbehavior().is_none());
    }

    #[test]
    fn process_with_2_of_3_witnesses() {
        let env = DcSlashEnvelope::new(
            vec![0xAA],
            DcMisbehavior::InvalidBindEnvelope,
            vec!["d1".into()],
            vec![vec![1], vec![2]], // 2 of 3
            1000,
        );
        let outcome = process_dc_slash(&env, 3, 0).unwrap();
        assert_eq!(outcome.cool_down_epochs, 2); // 2^1
        assert_eq!(outcome.domains_affected, 1);
        assert_eq!(outcome.final_state, DcFinalState::Cooldown);
    }

    #[test]
    fn process_with_1_of_3_witnesses_rejected() {
        let env = DcSlashEnvelope::new(
            vec![0xAA],
            DcMisbehavior::InvalidBindEnvelope,
            vec!["d1".into()],
            vec![vec![1]], // 1 of 3
            1000,
        );
        let result = process_dc_slash(&env, 3, 0);
        assert!(matches!(
            result,
            Err(DcSlashError::InsufficientWitnesses { .. })
        ));
    }

    #[test]
    fn process_with_5th_slash_permanent_ban() {
        let env = DcSlashEnvelope::new(
            vec![0xAA],
            DcMisbehavior::CensoredLegitMember,
            vec!["d1".into()],
            vec![vec![1], vec![2], vec![3], vec![4], vec![5]],
            1000,
        );
        // 5 of 5 witnesses signed; current slash count is 4
        // (about to be 5).
        let outcome = process_dc_slash(&env, 5, 4).unwrap();
        assert_eq!(outcome.cool_down_epochs, 32); // 2^5
        assert_eq!(outcome.final_state, DcFinalState::PermanentBan);
    }

    #[test]
    fn empty_dc_pubkey_rejected() {
        let env = DcSlashEnvelope::new(
            vec![],
            DcMisbehavior::FailedAttest,
            vec!["d1".into()],
            vec![vec![1], vec![2]],
            0,
        );
        let result = process_dc_slash(&env, 3, 0);
        assert_eq!(result, Err(DcSlashError::EmptyDcPubkey));
    }

    #[test]
    fn invalid_slash_reason_rejected() {
        let env = DcSlashEnvelope {
            dc_pubkey: vec![0xAA],
            slash_reason: 0x0001, // wrong reason
            slash_reason_data: 0,
            domains: vec![],
            witness_signatures: vec![vec![1], vec![2]],
            signed_at_epoch: 0,
        };
        let result = process_dc_slash(&env, 3, 0);
        assert_eq!(result, Err(DcSlashError::InvalidSlashReason(0x0001)));
    }

    #[test]
    fn invalid_slash_reason_data_rejected() {
        // slash_reason is correct (0x000F) but slash_reason_data
        // has an unrecognized sub-code. Must be rejected, not
        // silently processed.
        let env = DcSlashEnvelope {
            dc_pubkey: vec![0xAA],
            slash_reason: 0x000F,
            slash_reason_data: 0x0099, // unknown sub-code
            domains: vec!["d1".into()],
            witness_signatures: vec![vec![1], vec![2]], // 2 of 3 witnesses
            signed_at_epoch: 1000,
        };
        let result = process_dc_slash(&env, 3, 0);
        assert_eq!(
            result,
            Err(DcSlashError::InvalidSlashReasonData(0x0099))
        );
    }

    #[test]
    fn cool_down_doubles() {
        assert_eq!(cool_down_epochs(0), 1);
        assert_eq!(cool_down_epochs(1), 2);
        assert_eq!(cool_down_epochs(2), 4);
        assert_eq!(cool_down_epochs(3), 8);
        assert_eq!(cool_down_epochs(4), 16);
        assert_eq!(cool_down_epochs(5), 32);
    }

    #[test]
    fn cool_down_at_63_is_2_pow_63() {
        // Largest slash_count that does not overflow.
        // 1u64 << 63 = 0x8000_0000_0000_0000.
        assert_eq!(cool_down_epochs(63), 1u64 << 63);
    }

    #[test]
    fn cool_down_overflow_safe() {
        // 1u64 << 64 would overflow; must saturate.
        assert_eq!(cool_down_epochs(64), u64::MAX);
        assert_eq!(cool_down_epochs(u32::MAX), u64::MAX);
    }

    #[test]
    fn topic_format() {
        assert_eq!(dc_slash_topic("dc-1"), "/dot/slash/dc/dc-1");
    }
}
