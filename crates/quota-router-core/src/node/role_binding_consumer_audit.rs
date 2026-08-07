//! `RoleBindingConsumerAuditLog` (mission 0971-a1 AC-B1, RFC-0971 §Adversary A16).
//!
//! Consumer-side replay audit log. The producer-side is
//! `octo_wallet::capability::audit_replay_log::AuditReplayLog` (mission
//! 0970-a1, `audit_replay_log` Band A closure 2026-08-07). This module
//! adds the consumer-side wiring: when a destination node receives a
//! replay detection event from the producer-side, it records the event
//! into `RoleBindingConsumerAuditLog` for offline forensics.
//!
//! **Audit log semantics:** append-only, bounded by `capacity`. Once
//! full, additional `record_replay_detection()` calls return
//! `ConsumerAuditError::Full`. The log is consulted by offline forensics
//! only; the runtime replay defense path goes through the producer-side
//! `DestinationNonceStore` at `octo_wallet::capability::destination_nonce_store`.
//!
//! **Security (RFC-0957-A1 §Security):** `node_did` + `envelope_id` + `nonce`
//! are redacted in Debug output. The fields are identifiers, not secrets,
//! but exposing them in panic/log lines helps an attacker correlate
//! envelope IDs across log destinations. Per §Security defense-in-depth,
//! the manual `Debug` impls preserve `role_tag` or `at_millis_unix` for
//! forensics but hash the 32-byte fields.
//!
//! **Cross-mission contract:** this module is the canonical consumer-side
//! substrate for RFC-0971 §Role-binding audit trail. Mission 0970-a1
//! produces replay-detection events; this module consumes them.

use std::sync::Mutex;

use thiserror::Error;

/// Errors emitted by [`RoleBindingConsumerAuditLog`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConsumerAuditError {
    #[error("consumer audit log full (capacity {0} entries)")]
    Full(usize),
}

/// A single replay-detection entry (consumer-side).
///
/// `node_did` redacted in Debug per RFC-0957-A1 §Security (operator-facing
/// identifier; log lines MUST NOT print it raw). `envelope_id` +
/// `nonce` redacted in Debug per RFC-0957-A1 §Security (correlation
/// defense). `at_millis_unix` preserved for forensics.
#[derive(Clone, PartialEq, Eq)]
pub struct ConsumerReplayAuditEntry {
    pub node_did: String,
    pub envelope_id: [u8; 32],
    pub nonce: [u8; 32],
    pub at_millis_unix: i64,
}

impl std::fmt::Debug for ConsumerReplayAuditEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsumerReplayAuditEntry")
            .field("node_did", &"[REDACTED did]")
            .field("envelope_id", &"<redacted 32 bytes>")
            .field("nonce", &"<redacted 32 bytes>")
            .field("at_millis_unix", &self.at_millis_unix)
            .finish()
    }
}

/// Append-only consumer-side replay audit log. Bounded by `capacity` to
/// prevent unbounded growth; once full, additional `record_replay_detection()`
/// calls return `ConsumerAuditError::Full`.
pub struct RoleBindingConsumerAuditLog {
    entries: Mutex<Vec<ConsumerReplayAuditEntry>>,
    capacity: usize,
}

impl std::fmt::Debug for RoleBindingConsumerAuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoleBindingConsumerAuditLog")
            .field("capacity", &self.capacity)
            .field("entries", &format_args!("<redacted; len={}>", self.len()))
            .finish()
    }
}

impl RoleBindingConsumerAuditLog {
    /// Create a new consumer audit log with the given capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            capacity,
        }
    }

    /// Append a replay-detection entry.
    ///
    /// # Errors
    /// Returns [`ConsumerAuditError::Full`] when the log has reached
    /// capacity.
    pub fn record_replay_detection(
        &self,
        node_did: &str,
        envelope_id: [u8; 32],
        nonce: [u8; 32],
        at_millis_unix: i64,
    ) -> Result<(), ConsumerAuditError> {
        let mut entries = self
            .entries
            .lock()
            .expect("consumer audit log mutex poisoned");
        if entries.len() >= self.capacity {
            return Err(ConsumerAuditError::Full(self.capacity));
        }
        entries.push(ConsumerReplayAuditEntry {
            node_did: node_did.to_string(),
            envelope_id,
            nonce,
            at_millis_unix,
        });
        Ok(())
    }

    /// Snapshot the entries for offline analysis. Returns owned copies so
    /// the caller can iterate without holding the lock.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ConsumerReplayAuditEntry> {
        self.entries
            .lock()
            .expect("consumer audit log mutex poisoned")
            .clone()
    }

    /// Number of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .expect("consumer audit log mutex poisoned")
            .len()
    }

    /// True if the log has no recorded entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for RoleBindingConsumerAuditLog {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_then_len() {
        let log = RoleBindingConsumerAuditLog::new(8);
        log.record_replay_detection("did:octo:r1", [0xAA; 32], [0xBB; 32], 1_700_000_000_000)
            .unwrap();
        assert_eq!(log.len(), 1);
        assert!(!log.is_empty());
    }

    #[test]
    fn full_returns_err() {
        let log = RoleBindingConsumerAuditLog::new(2);
        log.record_replay_detection("did:octo:r1", [0x01; 32], [0x02; 32], 1)
            .unwrap();
        log.record_replay_detection("did:octo:r2", [0x03; 32], [0x04; 32], 2)
            .unwrap();
        assert_eq!(
            log.record_replay_detection("did:octo:r3", [0x05; 32], [0x06; 32], 3),
            Err(ConsumerAuditError::Full(2))
        );
        // still 2 entries
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn snapshot_returns_owned_copies() {
        let log = RoleBindingConsumerAuditLog::new(4);
        log.record_replay_detection("did:octo:r1", [0x11; 32], [0x22; 32], 100)
            .unwrap();
        let snap = log.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].node_did, "did:octo:r1");
        assert_eq!(snap[0].at_millis_unix, 100);
        assert_eq!(snap[0].envelope_id, [0x11; 32]);
        assert_eq!(snap[0].nonce, [0x22; 32]);
    }

    #[test]
    fn replay_entry_debug_redacts() {
        let entry = ConsumerReplayAuditEntry {
            node_did: "did:octo:redact-test".to_string(),
            envelope_id: [0xAA; 32],
            nonce: [0xBB; 32],
            at_millis_unix: 1_700_000_000_000,
        };
        let s = format!("{entry:?}");
        assert!(s.contains("REDACTED"), "expected node_did redaction: {s}");
        assert!(
            s.contains("redacted"),
            "expected 32-byte field redaction: {s}"
        );
        assert!(!s.contains("redact-test"), "leaked node_did: {s}");
        assert!(!s.contains("AAAA"), "leaked envelope_id: {s}");
        assert!(!s.contains("BBBB"), "leaked nonce: {s}");
    }

    #[test]
    fn audit_log_debug_redacts_entries() {
        let log = RoleBindingConsumerAuditLog::new(4);
        log.record_replay_detection("did:octo:r1", [0x11; 32], [0x22; 32], 100)
            .unwrap();
        let s = format!("{log:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(s.contains("capacity"), "expected capacity field: {s}");
        assert!(
            !s.contains("did:octo:r1"),
            "leaked node_did in log Debug: {s}"
        );
    }

    #[test]
    fn is_empty_default() {
        let log = RoleBindingConsumerAuditLog::new(8);
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn default_constructor_is_empty() {
        let log = RoleBindingConsumerAuditLog::default();
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
    }
}
