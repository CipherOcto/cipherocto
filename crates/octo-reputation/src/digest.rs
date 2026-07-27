//! `ReputationDigest` — 32-byte envelope digest over domain-separated BLAKE3.
//!
//! Wire form: `BLAKE3(domain || canonical_serialisation)`.
//! Domains are drawn from `constants::BLAKE3_REPUTATION_*_DOMAIN` and
//! `constants::BLAKE3_GOVERNANCE_*_DOMAIN`. Different domains over the same
//! bytes produce different digests, so an attacker cannot replay a digest
//! computed under one context into another.

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::constants::{BLAKE3_REPUTATION_AGGREGATE_DOMAIN, BLAKE3_REPUTATION_EVENT_DOMAIN};
use crate::types::{ReputationAggregate, SignalEvent};

/// 32-byte BLAKE3 digest. Wire form: `BLAKE3(domain || bytes)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReputationDigest(#[serde(with = "hex::serde")] pub [u8; 32]);

impl ReputationDigest {
    pub const ZERO: Self = Self([0u8; 32]);

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Debug for ReputationDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ReputationDigest({})", hex::encode(self.0))
    }
}

impl std::fmt::Display for ReputationDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

/// Compute a digest over a single `SignalEvent` under the EVENT domain.
pub fn digest_event(event: &SignalEvent) -> ReputationDigest {
    let bytes = event.canonical_bytes();
    let mut hasher = Hasher::new();
    hasher.update(BLAKE3_REPUTATION_EVENT_DOMAIN);
    hasher.update(&bytes);
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(out.as_bytes());
    ReputationDigest(arr)
}

/// Compute a digest over a `ReputationAggregate` under the AGGREGATE domain.
pub fn digest_aggregate(agg: &ReputationAggregate) -> ReputationDigest {
    let bytes = agg.canonical_bytes();
    let mut hasher = Hasher::new();
    hasher.update(BLAKE3_REPUTATION_AGGREGATE_DOMAIN);
    hasher.update(&bytes);
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(out.as_bytes());
    ReputationDigest(arr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_digest_is_32_zero_bytes() {
        assert_eq!(ReputationDigest::ZERO.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn distinct_events_produce_distinct_digests() {
        // Two events with same score but different `did` MUST produce
        // different digests because the canonical bytes differ.
        let e1 = SignalEvent::dummy_for_test(1, 100, 1.0);
        let e2 = SignalEvent::dummy_for_test(2, 100, 1.0);
        assert_ne!(digest_event(&e1), digest_event(&e2));
    }

    #[test]
    fn same_event_twice_yields_same_digest() {
        // Determinism contract — RFC-0104 bit-determinism.
        let e = SignalEvent::dummy_for_test(1, 100, 0.5);
        assert_eq!(digest_event(&e), digest_event(&e));
    }
}
