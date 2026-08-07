//! `DestinationNonceStore` (RFC-0970 §Replay Defense).
//!
//! Append-only nonce store keyed on `[u8; 32]` envelope nonces. Used by
//! `unwrap_at_destination` to reject duplicate submissions. Thread-safe
//! via `Mutex<HashSet<[u8; 32]>>` — `octo-wallet` is single-threaded
//! today, but the mutex keeps the API forward-compatible.

use std::collections::HashSet;
use std::sync::Mutex;

use thiserror::Error;

/// Errors emitted by [`DestinationNonceStore`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NonceError {
    #[error("nonce already recorded (replay attempt)")]
    AlreadyRecorded,
}

/// Append-only nonce store for hop envelope replay defense.
#[derive(Debug, Default)]
pub struct DestinationNonceStore {
    seen: Mutex<HashSet<[u8; 32]>>,
}

impl DestinationNonceStore {
    /// Create an empty nonce store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a nonce. Returns `AlreadyRecorded` if the nonce is already
    /// in the store — the caller MUST treat this as a replay attempt.
    ///
    /// # Errors
    /// Returns [`NonceError::AlreadyRecorded`] on duplicate.
    pub fn record(&self, nonce: &[u8; 32]) -> Result<(), NonceError> {
        let mut seen = self.seen.lock().expect("nonce store mutex poisoned");
        if !seen.insert(*nonce) {
            return Err(NonceError::AlreadyRecorded);
        }
        Ok(())
    }

    /// Query-only check: is `nonce` already recorded? Does NOT mutate state.
    #[must_use]
    pub fn is_seen(&self, nonce: &[u8; 32]) -> bool {
        self.seen
            .lock()
            .expect("nonce store mutex poisoned")
            .contains(nonce)
    }

    /// Number of recorded nonces (test/diagnostic helper).
    #[must_use]
    pub fn len(&self) -> usize {
        self.seen.lock().expect("nonce store mutex poisoned").len()
    }

    /// True if the store has no recorded nonces.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_then_is_seen() {
        let store = DestinationNonceStore::new();
        let nonce = [0xAA; 32];
        assert!(!store.is_seen(&nonce));
        store.record(&nonce).unwrap();
        assert!(store.is_seen(&nonce));
    }

    #[test]
    fn record_duplicate_rejects() {
        let store = DestinationNonceStore::new();
        let nonce = [0xBB; 32];
        store.record(&nonce).unwrap();
        assert_eq!(store.record(&nonce), Err(NonceError::AlreadyRecorded));
    }

    #[test]
    fn record_distinct_nonces_succeeds() {
        let store = DestinationNonceStore::new();
        store.record(&[0x01; 32]).unwrap();
        store.record(&[0x02; 32]).unwrap();
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn is_empty_default() {
        let store = DestinationNonceStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }
}
