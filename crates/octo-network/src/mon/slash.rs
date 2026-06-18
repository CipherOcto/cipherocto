//! Slash reason codes and SlashEnvelope (RFC-0855p-b §B + mission 0851p-a-bootstrap-slashing).
//!
//! Slash reason codes are 16-bit identifiers allocated by the
//! mission overlay network governance:
//!
//! - `0x0001` = `founder-squat` (mission 0855p-b)
//! - `0x0002` = `evidence-tampering` (mission 0855p-b)
//! - `0x0003` = `transport-lying` (mission 0855p-b)
//! - `0x000A` = `transport-binding-lie` (mission 0850p-c §6)
//! - `0x000B` = `transport-route-misroute` (mission 0850p-c §6)
//! - `0x000C, 0x000E-0xFFFF` = reserved (0x000D, 0x000F in use)
//! - `0x000D` = `bootstrap_node_misbehavior` (mission 0851p-a)
//!
//! ## Mission 0851p-a-bootstrap-slashing
//!
//! The new `0x000D` code covers bootstrap node misbehavior with
//! sub-codes stored in `slash_reason_data`:
//! - `0x000D.01` = `withholds_peers`
//! - `0x000D.02` = `stale_data`
//! - `0x000D.03` = `censors_legit_peer`
//! - `0x000D.04` = `false_reachability_claim`
//!
//! The sub-code is encoded as `(0x000D << 16) | sub_code` in the
//! 32-bit `slash_reason_data` field.

use serde::{Deserialize, Serialize};

/// Bootstrap node misbehavior sub-codes (mission 0851p-a).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum BootstrapMisbehavior {
    /// `0x000D.01` — claims 0 reachable peers when it has > 0.
    WithholdsPeers = 0x0001,
    /// `0x000D.02` — serves seed list older than MAX_SEED_AGE_EPOCHS.
    StaleData = 0x0002,
    /// `0x000D.03` — refuses to include a specific peer that other seeds have.
    CensorsLegitPeer = 0x0003,
    /// `0x000D.04` — claims a peer is reachable when it is not.
    FalseReachabilityClaim = 0x0004,
}

/// Slash reason code constants.
pub mod slash_code {
    /// Bootstrap node misbehavior (mission 0851p-a).
    pub const BOOTSTRAP_NODE_MISBEHAVIOR: u16 = 0x000D;
    /// Transport binding lie (mission 0850p-c §6).
    pub const TRANSPORT_BINDING_LIE: u16 = 0x000A;
    /// Transport route misroute (mission 0850p-c §6).
    pub const TRANSPORT_ROUTE_MISROUTE: u16 = 0x000B;
}

/// A slash envelope cast by a witness.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashEnvelope {
    /// The domain being slashed.
    pub domain_id: String,
    /// The slash instance id (unique per slash event).
    pub slash_id: String,
    /// The slash reason code (see `slash_code` constants).
    pub slash_reason: u16,
    /// Optional sub-code (used by `0x000D` for sub-codes like
    /// `WithholdsPeers`).
    #[serde(default)]
    pub slash_reason_data: u32,
    /// The slashed peer_id.
    pub target_peer: String,
    /// The witness's signature.
    pub signature: Vec<u8>,
    /// Unix epoch seconds.
    pub cast_at: u64,
}

impl SlashEnvelope {
    /// Create a bootstrap-misbehavior slash envelope with the
    /// given sub-code.
    pub fn bootstrap_misbehavior(
        domain_id: impl Into<String>,
        slash_id: impl Into<String>,
        target_peer: impl Into<String>,
        sub_code: BootstrapMisbehavior,
        signature: Vec<u8>,
        cast_at: u64,
    ) -> Self {
        let slash_reason_data =
            ((slash_code::BOOTSTRAP_NODE_MISBEHAVIOR as u32) << 16) | (sub_code as u32);
        Self {
            domain_id: domain_id.into(),
            slash_id: slash_id.into(),
            slash_reason: slash_code::BOOTSTRAP_NODE_MISBEHAVIOR,
            slash_reason_data,
            target_peer: target_peer.into(),
            signature,
            cast_at,
        }
    }

    /// Returns the bootstrap sub-code if this envelope uses the
    /// `0x000D` code, else None.
    pub fn bootstrap_sub_code(&self) -> Option<BootstrapMisbehavior> {
        if self.slash_reason != slash_code::BOOTSTRAP_NODE_MISBEHAVIOR {
            return None;
        }
        let sub = (self.slash_reason_data & 0xFFFF) as u16;
        match sub {
            0x0001 => Some(BootstrapMisbehavior::WithholdsPeers),
            0x0002 => Some(BootstrapMisbehavior::StaleData),
            0x0003 => Some(BootstrapMisbehavior::CensorsLegitPeer),
            0x0004 => Some(BootstrapMisbehavior::FalseReachabilityClaim),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_misbehavior_sub_code_roundtrip() {
        let env = SlashEnvelope::bootstrap_misbehavior(
            "d1",
            "s1",
            "peer-abc",
            BootstrapMisbehavior::StaleData,
            vec![],
            1700000000,
        );
        assert_eq!(env.slash_reason, 0x000D);
        assert_eq!(env.bootstrap_sub_code(), Some(BootstrapMisbehavior::StaleData));
    }

    #[test]
    fn non_bootstrap_slash_has_no_sub_code() {
        let env = SlashEnvelope {
            domain_id: "d1".into(),
            slash_id: "s1".into(),
            slash_reason: slash_code::TRANSPORT_BINDING_LIE,
            slash_reason_data: 0,
            target_peer: "p".into(),
            signature: vec![],
            cast_at: 0,
        };
        assert!(env.bootstrap_sub_code().is_none());
    }

    #[test]
    fn unknown_bootstrap_sub_code_returns_none() {
        let env = SlashEnvelope {
            domain_id: "d1".into(),
            slash_id: "s1".into(),
            slash_reason: 0x000D,
            slash_reason_data: 0x0099, // unknown
            target_peer: "p".into(),
            signature: vec![],
            cast_at: 0,
        };
        assert!(env.bootstrap_sub_code().is_none());
    }

    #[test]
    fn slash_envelope_serde_roundtrip() {
        let env = SlashEnvelope::bootstrap_misbehavior(
            "d1",
            "s1",
            "peer",
            BootstrapMisbehavior::WithholdsPeers,
            vec![1, 2, 3],
            1700000000,
        );
        let json = serde_json::to_string(&env).unwrap();
        let back: SlashEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, env);
    }
}
