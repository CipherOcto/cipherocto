//! Platform-loss auto-rejoin (mission 0855p-c-auto-rejoin).
//!
//! A kicked member can request rejoin via a `REJOIN_REQUEST`
//! envelope; the DomainCoordinator signs a rejoin ticket if the
//! kick was unauthorized. Handles accidental mass-kick recovery
//! without requiring a full UNBIND+BIND cycle.
//!
//! ## Rate limit
//!
//! - `REJOIN_COOLDOWN_EPOCHS = 1000` (~16 hours) per peer
//! - Prevents rejoin abuse
//!
//! ## Ticket validity
//!
//! - `REJOIN_TICKET_VALID_EPOCHS = 100` (~100 minutes)

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Cooldown between rejoin requests per peer (per mission spec).
pub const REJOIN_COOLDOWN_EPOCHS: u64 = 1000;
/// How long a rejoin ticket is valid for.
pub const REJOIN_TICKET_VALID_EPOCHS: u64 = 100;

/// A `REJOIN_REQUEST` envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejoinRequest {
    pub domain_id: String,
    pub kicked_peer_id: String,
    /// Signed platform-API response showing the kick.
    pub kick_evidence: Vec<u8>,
    pub peer_pubkey: Vec<u8>,
    pub reason: String,
    pub signed_at_epoch: u64,
}

/// A `RejoinTicket` envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejoinTicket {
    pub domain_id: String,
    pub peer_id: String,
    /// The DC's signature as the rejoin token.
    pub rejoin_token: Vec<u8>,
    pub expires_at_epoch: u64,
}

impl RejoinTicket {
    /// Returns true if the ticket has expired.
    pub fn is_expired(&self, current_epoch: u64) -> bool {
        current_epoch > self.expires_at_epoch
    }

    /// Returns true if the ticket is currently valid.
    pub fn is_valid(&self, current_epoch: u64) -> bool {
        !self.is_expired(current_epoch)
    }
}

/// Errors for rejoin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RejoinError {
    /// Rejoin is rate-limited (last request within REJOIN_COOLDOWN_EPOCHS).
    RateLimited { last_request_epoch: u64 },
    /// The kick was authorized (e.g., the DC requested it).
    AuthorizedKick,
    /// The kick evidence is invalid.
    InvalidKickEvidence,
    /// The peer is unknown to the DC.
    UnknownPeer,
}

/// Tracks the cooldown for a peer's rejoin requests.
#[derive(Clone, Debug, Default)]
pub struct RejoinCooldown {
    last_request: std::collections::HashMap<String, u64>,
}

impl RejoinCooldown {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if a rejoin request is allowed for
    /// `peer_id` at `current_epoch`. If allowed, records the
    /// request.
    pub fn check_and_record(
        &mut self,
        peer_id: &str,
        current_epoch: u64,
    ) -> Result<(), RejoinError> {
        if let Some(&last) = self.last_request.get(peer_id) {
            if current_epoch.saturating_sub(last) < REJOIN_COOLDOWN_EPOCHS {
                return Err(RejoinError::RateLimited {
                    last_request_epoch: last,
                });
            }
        }
        self.last_request.insert(peer_id.to_string(), current_epoch);
        Ok(())
    }
}

/// Current Unix epoch in seconds (for diagnostics / operator
/// visibility).
///
/// WARNING: this is the Unix time in seconds, not a network
/// consensus epoch. For the rejoin cooldown window, callers
/// should pass a consensus-epoch clock (or convert at the
/// call site).
pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Deprecated alias for [`now_unix_seconds`].
#[deprecated(note = "renamed to now_unix_seconds for clarity")]
pub fn now_epoch() -> u64 {
    now_unix_seconds()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_first_request_allowed() {
        let mut cd = RejoinCooldown::new();
        assert!(cd.check_and_record("peer-1", 1000).is_ok());
    }

    #[test]
    fn cooldown_second_request_within_window_rejected() {
        let mut cd = RejoinCooldown::new();
        cd.check_and_record("peer-1", 1000).unwrap();
        let result = cd.check_and_record("peer-1", 1500);
        assert!(matches!(result, Err(RejoinError::RateLimited { .. })));
    }

    #[test]
    fn cooldown_after_window_allowed() {
        let mut cd = RejoinCooldown::new();
        cd.check_and_record("peer-1", 1000).unwrap();
        // 1000 epochs later, well past 1000 cooldown.
        assert!(cd.check_and_record("peer-1", 2001).is_ok());
    }

    #[test]
    fn cooldown_per_peer() {
        let mut cd = RejoinCooldown::new();
        cd.check_and_record("peer-1", 1000).unwrap();
        assert!(cd.check_and_record("peer-2", 1000).is_ok());
    }

    #[test]
    fn cooldown_at_exact_boundary_allowed() {
        // At exactly REJOIN_COOLDOWN_EPOCHS (1000) after the
        // last request, a new request is allowed.
        let mut cd = RejoinCooldown::new();
        cd.check_and_record("peer-1", 1000).unwrap();
        // 999 epochs later: still rate-limited (< 1000).
        assert!(cd.check_and_record("peer-1", 1999).is_err());
        // 1000 epochs later (exactly at boundary): allowed.
        assert!(cd.check_and_record("peer-1", 2000).is_ok());
    }

    #[test]
    fn ticket_validity() {
        let t = RejoinTicket {
            domain_id: "d1".into(),
            peer_id: "peer-1".into(),
            rejoin_token: vec![0xAA],
            expires_at_epoch: 1100,
        };
        assert!(t.is_valid(1000));
        assert!(t.is_valid(1100));
        assert!(!t.is_valid(1101));
    }

    #[test]
    fn rejoin_request_serde_roundtrip() {
        let r = RejoinRequest {
            domain_id: "d1".into(),
            kicked_peer_id: "peer-1".into(),
            kick_evidence: vec![0x01, 0x02],
            peer_pubkey: vec![0xAA],
            reason: "mass-kick by compromised admin".into(),
            signed_at_epoch: 1000,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: RejoinRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn constants() {
        assert_eq!(REJOIN_COOLDOWN_EPOCHS, 1000);
        assert_eq!(REJOIN_TICKET_VALID_EPOCHS, 100);
    }
}
