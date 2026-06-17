//! Cross-platform admin attestation (mission 0855p-c-admin-attestation).
//!
//! Each DomainCoordinator periodically publishes a
//! `PlatformAdminAttest` envelope on the libp2p mesh under
//! `/dot/admin/{domain_id}/{platform}` containing a fresh proof
//! of admin status. Other DomainCoordinators verify and challenge
//! invalid attestations.
//!
//! ## Freshness
//!
//! - `MAX_ATTEST_AGE_EPOCHS = 100` (~100 minutes at 1-min epochs)
//! - `ATTEST_PERIOD_EPOCHS = 50` (publish every 50 minutes)
//! - `CHALLENGE_RESPONSE_EPOCHS = 10` (DC must respond within 10 epochs)

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Maximum age of an attestation (per mission spec).
pub const MAX_ATTEST_AGE_EPOCHS: u64 = 100;
/// Period between attestations.
pub const ATTEST_PERIOD_EPOCHS: u64 = 50;
/// Time for a DC to respond to a challenge.
pub const CHALLENGE_RESPONSE_EPOCHS: u64 = 10;

/// The platform identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    WhatsApp,
    Telegram,
    Matrix,
    Slack,
    Discord,
    Nostr,
    Custom,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::WhatsApp => "whatsapp",
            Platform::Telegram => "telegram",
            Platform::Matrix => "matrix",
            Platform::Slack => "slack",
            Platform::Discord => "discord",
            Platform::Nostr => "nostr",
            Platform::Custom => "custom",
        }
    }
}

/// A `PlatformAdminAttest` envelope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformAdminAttest {
    pub domain_id: String,
    pub platform: Platform,
    pub platform_group_id: String,
    pub dc_pubkey: Vec<u8>,
    /// A signed response from the platform API.
    pub proof: Vec<u8>,
    pub signed_at_epoch: u64,
}

impl PlatformAdminAttest {
    /// Returns true if the attest is fresh (within MAX_ATTEST_AGE_EPOCHS).
    pub fn is_fresh(&self, current_epoch: u64) -> bool {
        current_epoch.saturating_sub(self.signed_at_epoch) <= MAX_ATTEST_AGE_EPOCHS
    }

    /// Returns the age of the attest in epochs.
    pub fn age_epochs(&self, current_epoch: u64) -> u64 {
        current_epoch.saturating_sub(self.signed_at_epoch)
    }

    /// Returns true if a new attest should be published
    /// (older than ATTEST_PERIOD_EPOCHS).
    pub fn needs_renewal(&self, current_epoch: u64) -> bool {
        self.age_epochs(current_epoch) >= ATTEST_PERIOD_EPOCHS
    }
}

/// A challenge against a DC's attest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestChallenge {
    pub domain_id: String,
    pub dc_pubkey: Vec<u8>,
    pub reason: String,
    pub evidence: Vec<u8>,
    pub issued_at_epoch: u64,
    /// The deadline by which the DC must respond.
    pub response_deadline_epoch: u64,
}

impl AttestChallenge {
    /// Create a new challenge with the standard response deadline.
    pub fn new(
        domain_id: impl Into<String>,
        dc_pubkey: Vec<u8>,
        reason: impl Into<String>,
        evidence: Vec<u8>,
        issued_at_epoch: u64,
    ) -> Self {
        Self {
            domain_id: domain_id.into(),
            dc_pubkey,
            reason: reason.into(),
            evidence,
            issued_at_epoch,
            response_deadline_epoch: issued_at_epoch + CHALLENGE_RESPONSE_EPOCHS,
        }
    }

    /// Returns true if the deadline has elapsed (DC failed to
    /// respond in time).
    pub fn is_expired(&self, current_epoch: u64) -> bool {
        current_epoch > self.response_deadline_epoch
    }
}

/// Errors from attest verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformAdminAttestError {
    /// The attest is too old.
    Stale { age_epochs: u64, max: u64 },
    /// The platform API proof is invalid.
    InvalidProof,
    /// The DC pubkey in the attest does not match the expected DC.
    WrongDc,
}

/// Verify an attest is fresh and matches the expected DC.
pub fn verify_attest(
    attest: &PlatformAdminAttest,
    expected_dc_pubkey: &[u8],
    current_epoch: u64,
) -> Result<(), PlatformAdminAttestError> {
    let age = attest.age_epochs(current_epoch);
    if age > MAX_ATTEST_AGE_EPOCHS {
        return Err(PlatformAdminAttestError::Stale {
            age_epochs: age,
            max: MAX_ATTEST_AGE_EPOCHS,
        });
    }
    if attest.dc_pubkey != expected_dc_pubkey {
        return Err(PlatformAdminAttestError::WrongDc);
    }
    // Real proof verification is per-platform (WhatsApp, Telegram,
    // Matrix each have different admin verification APIs). This
    // module provides the freshness + DC-pubkey check; the
    // platform-specific proof check is delegated to the platform
    // adapter (out of scope for this mission).
    Ok(())
}

/// Derive the libp2p gossip topic for a DC's attest.
pub fn attest_topic(domain_id: &str, platform: Platform) -> String {
    format!("/dot/admin/{}/{}", domain_id, platform.as_str())
}

/// Current epoch seconds (for diagnostics).
pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_attest(epoch: u64) -> PlatformAdminAttest {
        PlatformAdminAttest {
            domain_id: "d1".into(),
            platform: Platform::WhatsApp,
            platform_group_id: "g1".into(),
            dc_pubkey: vec![0xAA],
            proof: vec![0xBB],
            signed_at_epoch: epoch,
        }
    }

    #[test]
    fn fresh_attest_passes() {
        let a = fresh_attest(100);
        assert!(a.is_fresh(150));
    }

    #[test]
    fn stale_attest_rejected() {
        let a = fresh_attest(100);
        assert!(!a.is_fresh(201)); // 101 epochs > 100
    }

    #[test]
    fn needs_renewal_after_period() {
        let a = fresh_attest(100);
        assert!(!a.needs_renewal(140)); // 40 epochs < 50
        assert!(a.needs_renewal(151)); // 51 epochs >= 50
    }

    #[test]
    fn verify_attest_fresh_and_matching() {
        let a = fresh_attest(100);
        assert!(verify_attest(&a, &[0xAA], 150).is_ok());
    }

    #[test]
    fn verify_attest_stale() {
        let a = fresh_attest(100);
        let result = verify_attest(&a, &[0xAA], 250);
        assert!(matches!(result, Err(PlatformAdminAttestError::Stale { .. })));
    }

    #[test]
    fn verify_attest_wrong_dc() {
        let a = fresh_attest(100);
        let result = verify_attest(&a, &[0xCC], 150);
        assert_eq!(result, Err(PlatformAdminAttestError::WrongDc));
    }

    #[test]
    fn verify_attest_at_max_age_boundary() {
        // age = MAX_ATTEST_AGE_EPOCHS (100) is still fresh.
        // age = MAX + 1 is stale.
        let a = fresh_attest(100);
        // age = 100 (exact MAX): fresh.
        assert!(verify_attest(&a, &[0xAA], 200).is_ok());
        // age = 101 (one over): stale.
        let result = verify_attest(&a, &[0xAA], 201);
        assert!(matches!(result, Err(PlatformAdminAttestError::Stale { .. })));
    }

    #[test]
    fn challenge_response_deadline() {
        let c = AttestChallenge::new("d1", vec![0xAA], "stale proof", vec![], 1000);
        assert_eq!(c.response_deadline_epoch, 1010);
        assert!(!c.is_expired(1005));
        assert!(!c.is_expired(1010));
        assert!(c.is_expired(1011));
    }

    #[test]
    fn topic_format() {
        let t = attest_topic("d1", Platform::WhatsApp);
        assert_eq!(t, "/dot/admin/d1/whatsapp");
    }

    #[test]
    fn platform_as_str() {
        assert_eq!(Platform::WhatsApp.as_str(), "whatsapp");
        assert_eq!(Platform::Matrix.as_str(), "matrix");
        assert_eq!(Platform::Custom.as_str(), "custom");
    }
}
