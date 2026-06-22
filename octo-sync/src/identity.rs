//! Identity derivation for the cipherocto sync engine (per RFC-0862 §4.3.1).
//!
//! Defines:
//! - [`SyncNodeId`] — `BLAKE3(public_key || mission_id)`, 32 bytes. The local
//!   node's stable identifier for the duration of a sync session.
//! - [`SyncPeerId`] — opaque 32-byte identifier for a remote peer. The encoding
//!   is the same as `SyncNodeId` (we use the same `BLAKE3(public_key || mission_id)`
//!   scheme on both sides), so the types are interchangeable for hashing purposes
//!   but distinct at the type level to prevent confusion.
//!
//! # Why two types?
//!
//! The cipherocto convention is to use distinct types for "me" and "them" even
//! when they share the same wire format. The reader of the code can immediately
//! tell which is which without checking the variable name. See
//! `crates/octo-network/src/dot/adapters/coordinator_admin.rs:127` for the
//! `PeerId(pub String)` precedent.

use blake3::Hash;

use crate::types::MissionId;

/// A sync node's stable identifier.
///
/// Computed as `BLAKE3(public_key || mission_id)` per RFC-0862 §4.3.1. MUST be
/// stable for the lifetime of a sync session (per RFC-0862 §Implicit Assumptions
/// Audit row 5).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SyncNodeId(pub [u8; 32]);

/// A sync peer's identifier.
///
/// Computed as `BLAKE3(public_key || mission_id)` per RFC-0862 §4.3.1. The wire
/// format is identical to [`SyncNodeId`]; the type distinction is purely for
/// code clarity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SyncPeerId(pub [u8; 32]);

impl SyncNodeId {
    /// Derive a `SyncNodeId` from a public key and a mission ID.
    ///
    /// `BLAKE3(public_key || mission_id)` per RFC-0862 §4.3.1.
    pub fn derive(public_key: &[u8], mission_id: &MissionId) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(public_key);
        hasher.update(mission_id);
        let hash: Hash = hasher.finalize();
        Self(*hash.as_bytes())
    }

    /// Return the underlying 32 bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl SyncPeerId {
    /// Derive a `SyncPeerId` from a remote peer's public key and the local mission ID.
    ///
    /// `BLAKE3(public_key || mission_id)` per RFC-0862 §4.3.1.
    pub fn derive(public_key: &[u8], mission_id: &MissionId) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(public_key);
        hasher.update(mission_id);
        let hash: Hash = hasher.finalize();
        Self(*hash.as_bytes())
    }

    /// Return the underlying 32 bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pubkey() -> Vec<u8> {
        // 32-byte ed25519 public key (sample)
        let mut k = vec![0u8; 32];
        k[0] = 0x01;
        k[31] = 0xFF;
        k
    }

    fn sample_mission() -> MissionId {
        let mut m = [0u8; 32];
        m[0] = 0xAB;
        m
    }

    #[test]
    fn derive_is_deterministic() {
        let pk = sample_pubkey();
        let m = sample_mission();
        let id1 = SyncNodeId::derive(&pk, &m);
        let id2 = SyncNodeId::derive(&pk, &m);
        assert_eq!(id1, id2);
    }

    #[test]
    fn different_pubkey_yields_different_id() {
        let m = sample_mission();
        let id1 = SyncNodeId::derive(&[0u8; 32], &m);
        let id2 = SyncNodeId::derive(&[1u8; 32], &m);
        assert_ne!(id1, id2);
    }

    #[test]
    fn different_mission_yields_different_id() {
        let pk = sample_pubkey();
        let mut m2 = sample_mission();
        m2[0] = 0xCD;
        let id1 = SyncNodeId::derive(&pk, &sample_mission());
        let id2 = SyncNodeId::derive(&pk, &m2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn sync_node_id_and_sync_peer_id_have_same_format() {
        // Both use BLAKE3(public_key || mission_id)
        let pk = sample_pubkey();
        let m = sample_mission();
        let node_id = SyncNodeId::derive(&pk, &m);
        let peer_id = SyncPeerId::derive(&pk, &m);
        assert_eq!(node_id.as_bytes(), peer_id.as_bytes());
    }

    #[test]
    fn types_are_distinct() {
        // Compile-time check: SyncNodeId and SyncPeerId are distinct types.
        fn _accepts_node(_: SyncNodeId) {}
        fn _accepts_peer(_: SyncPeerId) {}
        let id = SyncNodeId([0u8; 32]);
        _accepts_node(id);
        // The following would NOT compile (intentional):
        // _accepts_peer(id);
    }
}
