//! `audit_replay_log` (RFC-0970 §Forensics).
//!
//! Append-only log of replay detections. Each entry records the offending
//! envelope's identity, the rejected nonce, the destination node DID, and
//! a wall-clock timestamp. Used for offline forensics; NOT consulted by
//! the runtime replay defense path (which goes through
//! [`DestinationNonceStore`]).
//!
//! **Security:** manual `Debug` impl redacts the 32-byte envelope_id + nonce
//! in the formatted output (per RFC-0957-A1 §Security; the fields are
//! identifiers, not secrets, but exposing them in panic/log lines helps an
//! attacker correlate envelope IDs across log destinations).

use std::sync::Mutex;

use thiserror::Error;

/// Errors emitted by [`AuditReplayLog`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AuditError {
    #[error("audit log full (capacity {0} entries)")]
    Full(usize),
}

/// A single replay-detection entry.
#[derive(Clone, PartialEq, Eq)]
pub struct ReplayEntry {
    pub envelope_id: [u8; 32],
    pub nonce: [u8; 32],
    pub node_did: String,
    pub at_millis_unix: u64,
}

impl std::fmt::Debug for ReplayEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplayEntry")
            .field("envelope_id", &"<redacted 32 bytes>")
            .field("nonce", &"<redacted 32 bytes>")
            .field("node_did", &self.node_did)
            .field("at_millis_unix", &self.at_millis_unix)
            .finish()
    }
}

/// Append-only replay audit log. Bounded by `capacity` to prevent unbounded
/// growth; once full, additional `record()` calls return `AuditError::Full`.
pub struct AuditReplayLog {
    entries: Mutex<Vec<ReplayEntry>>,
    capacity: usize,
}

impl std::fmt::Debug for AuditReplayLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditReplayLog")
            .field("capacity", &self.capacity)
            .field("entries", &format_args!("<redacted; len={}>", self.len()))
            .finish()
    }
}

impl AuditReplayLog {
    /// Create a new audit log with the given capacity.
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
    /// Returns [`AuditError::Full`] when the log has reached capacity.
    pub fn record(
        &self,
        envelope_id: [u8; 32],
        nonce: [u8; 32],
        node_did: &str,
        at_millis_unix: u64,
    ) -> Result<(), AuditError> {
        let mut entries = self.entries.lock().expect("audit log mutex poisoned");
        if entries.len() >= self.capacity {
            return Err(AuditError::Full(self.capacity));
        }
        entries.push(ReplayEntry {
            envelope_id,
            nonce,
            node_did: node_did.to_string(),
            at_millis_unix,
        });
        Ok(())
    }

    /// Number of recorded entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().expect("audit log mutex poisoned").len()
    }

    /// True if the log has no recorded entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Snapshot the entries for offline analysis. Returns owned copies so
    /// the caller can iterate without holding the lock.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ReplayEntry> {
        self.entries
            .lock()
            .expect("audit log mutex poisoned")
            .clone()
    }
}

impl Default for AuditReplayLog {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_then_len() {
        let log = AuditReplayLog::new(8);
        log.record([0xAA; 32], [0xBB; 32], "did:octo:r1", 1_700_000_000_000)
            .unwrap();
        assert_eq!(log.len(), 1);
        assert!(!log.is_empty());
    }

    #[test]
    fn full_returns_err() {
        let log = AuditReplayLog::new(2);
        log.record([0x01; 32], [0x02; 32], "did:octo:r1", 1)
            .unwrap();
        log.record([0x03; 32], [0x04; 32], "did:octo:r2", 2)
            .unwrap();
        assert_eq!(
            log.record([0x05; 32], [0x06; 32], "did:octo:r3", 3),
            Err(AuditError::Full(2))
        );
        // still 2 entries
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn snapshot_returns_owned_copies() {
        let log = AuditReplayLog::new(4);
        log.record([0x11; 32], [0x22; 32], "did:octo:r1", 100)
            .unwrap();
        let snap = log.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].node_did, "did:octo:r1");
        assert_eq!(snap[0].at_millis_unix, 100);
    }

    #[test]
    fn replay_entry_debug_redacts() {
        let entry = ReplayEntry {
            envelope_id: [0xAA; 32],
            nonce: [0xBB; 32],
            node_did: "did:octo:redact-test".to_string(),
            at_millis_unix: 1_700_000_000_000,
        };
        let s = format!("{entry:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(!s.contains("AAAA"), "leaked envelope_id: {s}");
        assert!(!s.contains("BBBB"), "leaked nonce: {s}");
    }

    #[test]
    fn audit_log_debug_redacts_entries() {
        let log = AuditReplayLog::new(4);
        log.record([0x11; 32], [0x22; 32], "did:octo:r1", 100)
            .unwrap();
        let s = format!("{log:?}");
        assert!(s.contains("redacted"), "expected redaction: {s}");
        assert!(s.contains("capacity"), "expected capacity field: {s}");
    }
}
