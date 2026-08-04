//! Outbox table (RFC-0957-A1 §Outbox).
//!
//! Transactional at-least-once delivery. The outbox is in the same
//! transaction as the holder_registry inserts + settlement event append +
//! chain_tip CAS. A crash between commit and gossip leaves the outbox entry
//! durable; the outbox worker (separate sub-mission) replays it on restart.
//!
//! Mission 0957-c ships the schema + migration; the outbox WORKER is
//! owned by 0959-c (cross-mission dependency on RFC-0959-A1 §Outbox Worker).

use serde::{Deserialize, Serialize};

use std::time::Duration;

/// Maximum outbox retry attempts before the worker flags for operator intervention.
/// Per RFC-0957-A1 §Outbox constants (R7-N13 fix).
pub const MAX_OUTBOX_ATTEMPTS: u32 = 10;

/// Outbox worker scan period.
pub const OUTBOX_SCAN_PERIOD: Duration = Duration::from_secs(5);

/// Outbox row.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    /// Auto-increment row ID.
    pub id: Option<i64>,
    /// Buyer DID (RFC-0009).
    pub buyer_did: String,
    /// Payload bytes (canonical_ser of `MarketDeliveryEnvelope`).
    pub payload: Vec<u8>,
    /// Number of attempts so far.
    pub attempts: u32,
    /// Created-at millis-unix.
    pub created_at_millis_unix: i64,
    /// Last attempt millis-unix (None before first attempt).
    pub last_attempt_millis_unix: Option<i64>,
    /// Flagged for manual operator intervention (R7-N12: nullable; worker writes 1 on threshold).
    pub flagged_for_intervention: Option<bool>,
}

// Manual Debug redaction: payload is credential-bearing.
impl std::fmt::Debug for OutboxEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutboxEntry")
            .field("id", &self.id)
            .field("buyer_did", &self.buyer_did)
            .field(
                "payload",
                &format_args!("<redacted {} bytes>", self.payload.len()),
            )
            .field("attempts", &self.attempts)
            .field("created_at_millis_unix", &self.created_at_millis_unix)
            .field("last_attempt_millis_unix", &self.last_attempt_millis_unix)
            .field("flagged_for_intervention", &self.flagged_for_intervention)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_have_expected_values() {
        assert_eq!(MAX_OUTBOX_ATTEMPTS, 10);
        assert_eq!(OUTBOX_SCAN_PERIOD, Duration::from_secs(5));
    }

    #[test]
    fn debug_redacts_payload() {
        let e = OutboxEntry {
            id: Some(1),
            buyer_did: "did:octo:buyer".into(),
            payload: vec![0xAB; 256],
            attempts: 0,
            created_at_millis_unix: 1_700_000_000_000,
            last_attempt_millis_unix: None,
            flagged_for_intervention: None,
        };
        let s = format!("{:?}", e);
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("ABAB"), "leaked payload bytes: {s}");
    }
}
