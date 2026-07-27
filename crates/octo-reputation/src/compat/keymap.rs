//! `recorder_did` ↔ legacy `coordinator_pubkey` keymap.
//!
//! Per mission 0855p-b Round 7 cross-mission finding: gossip and slash-binding
//! keys on canonical `recorder_did` / stable lineage identifier, NEVER on
//! `coordinator_pubkey`. This keymap is therefore ONLY used for legacy
//! compat reads — it does not introduce a canonical identity. Legacy
//! `SlashReputationStore` may index by coordinator_pubkey for back-compat
//! with pre-RFC-0968 callers; the compat adapter translates between the two.

use std::collections::HashMap;

use crate::RecorderDid;

/// One mapping row: a canonical recorder DID and its legacy coordinator
/// public-key equivalent. The pubkey is `[u8; 32]` (ed25519) for slash,
/// `[u8; 32]` (BLAKE3 over pubkey) for DC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatMapping {
    pub recorder_did: RecorderDid,
    pub coordinator_pubkey: [u8; 32],
}

#[derive(Default)]
pub struct CompatKeymap {
    by_did: HashMap<[u8; 52], [u8; 32]>,
    by_pubkey: HashMap<[u8; 32], [u8; 52]>,
}

impl CompatKeymap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, mapping: CompatMapping) {
        self.by_did
            .insert(*mapping.recorder_did.as_bytes(), mapping.coordinator_pubkey);
        self.by_pubkey
            .insert(mapping.coordinator_pubkey, *mapping.recorder_did.as_bytes());
    }

    pub fn pubkey_for(&self, did: &RecorderDid) -> Option<[u8; 32]> {
        self.by_did.get(did.as_bytes()).copied()
    }

    pub fn did_for(&self, pubkey: &[u8; 32]) -> Option<RecorderDid> {
        self.by_pubkey
            .get(pubkey)
            .map(|arr| RecorderDid::from_array(*arr))
    }

    pub fn len(&self) -> usize {
        self.by_did.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_did.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_did_to_pubkey() {
        let mut km = CompatKeymap::new();
        let did = RecorderDid::from_array([7u8; 52]);
        let pk = [1u8; 32];
        km.insert(CompatMapping {
            recorder_did: did,
            coordinator_pubkey: pk,
        });
        assert_eq!(km.pubkey_for(&did), Some(pk));
        assert_eq!(km.did_for(&pk), Some(did));
    }

    #[test]
    fn missing_did_returns_none() {
        let km = CompatKeymap::new();
        let did = RecorderDid::from_array([7u8; 52]);
        assert!(km.pubkey_for(&did).is_none());
    }
}
