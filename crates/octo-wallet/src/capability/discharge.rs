//! Discharge channels (RFC-0957 §3.6).
//!
//! Discharge macaroons are issued by **third-party channels** to satisfy
//! third-party caveats on the root capability. CipherOcto defines three
//! standard channels:
//! - **escrow** — settlement oracle (RFC-0959 v1.0)
//! - **revocation** — revocation oracle (per-RFC-0853)
//! - **rate-limit** — rate-limit oracle (per-RFC-0959 §Anti-fraud)
//!
//! Each channel has a distinct root secret held by the channel operator.
//! The root capability references the channel by name (string ID); the
//! verifier resolves the channel root secret via out-of-band discovery.

use serde::{Deserialize, Serialize};

/// Channel identifier (opaque string). Standard channels: "escrow", "revocation", "rate-limit".
pub type ChannelId = String;

/// Discharge macaroon issued by a third-party channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DischargeMacaroon {
    /// Channel identifier (`"escrow"`, `"revocation"`, etc.).
    pub channel: ChannelId,
    /// Discharge macaroon body (32-byte root secret hash + caveats).
    pub root_secret_hash: [u8; 32],
    /// Chain HMACs (same format as `Macaroon.chain`).
    pub chain: Vec<[u8; 32]>,
    /// Caveats on the discharge (e.g., time bounds).
    pub caveats: Vec<super::Caveat>,
}

/// Standard discharge channels (RFC-0957 §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DischargeChannel {
    /// Settlement oracle (escrow).
    Escrow,
    /// Revocation oracle.
    Revocation,
    /// Rate-limit oracle.
    RateLimit,
}

impl DischargeChannel {
    /// Wire-stable channel identifier.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Escrow => "escrow",
            Self::Revocation => "revocation",
            Self::RateLimit => "rate-limit",
        }
    }
}

impl std::str::FromStr for DischargeChannel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "escrow" => Ok(Self::Escrow),
            "revocation" => Ok(Self::Revocation),
            "rate-limit" => Ok(Self::RateLimit),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::caveat::Caveat;

    #[test]
    fn standard_channels_parse() {
        for s in ["escrow", "revocation", "rate-limit"] {
            let _: DischargeChannel = s.parse().unwrap();
        }
    }

    #[test]
    fn unknown_rejected() {
        assert!("nonsense".parse::<DischargeChannel>().is_err());
    }

    #[test]
    fn discharge_macaroon_construction() {
        // Cover the DischargeMacaroon struct fields per RFC-0957 §3.6.
        let dm = DischargeMacaroon {
            channel: "escrow".to_owned(),
            root_secret_hash: [0xab; 32],
            chain: vec![[0xcd; 32], [0xef; 32]],
            caveats: vec![Caveat::Before(1_700_000_000)],
        };
        assert_eq!(dm.channel, "escrow");
        assert_eq!(dm.root_secret_hash, [0xab; 32]);
        assert_eq!(dm.chain.len(), 2);
        assert_eq!(dm.caveats.len(), 1);
    }

    #[test]
    fn discharge_channel_serde_roundtrip() {
        for ch in [
            DischargeChannel::Escrow,
            DischargeChannel::Revocation,
            DischargeChannel::RateLimit,
        ] {
            let s = ch.as_str();
            let back: DischargeChannel = s.parse().unwrap();
            assert_eq!(ch, back);
        }
    }
}
